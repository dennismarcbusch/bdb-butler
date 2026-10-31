//! Sync-Client für den Go-Server (Protokoll: `docs/ARCHITEKTUR.md`).
//!
//! [`sync`] holt zuerst alle neuen Einträge vom Server und spielt sie lokal ein (mit Zusammenführung),
//! dann sendet es die lokalen Änderungen. Antwortet der Server mit einem Konflikt (409), wird der aktuelle
//! Serverstand eingespielt, zusammengeführt und erneut gesendet.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::crypto::{self, KdfParams, Keyring, RecoveryKey, Vault};
use crate::record::Clock;
use crate::store::{ApplyOutcome, RemoteEntry, Store, StoreError};

const PAGE_SIZE: u32 = 500;
const MAX_ROUNDS: usize = 10;
const MAX_RETRIES_PER_ENTRY: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Server nicht erreichbar: {0}")]
    Network(String),
    #[error("Der Server hat das Token abgelehnt")]
    Unauthorized,
    #[error("Unerwartete Antwort des Servers (Status {0})")]
    Status(u16),
    #[error("Ungültige Antwort des Servers: {0}")]
    Protocol(String),
    #[error("Auf dem Server gibt es schon einen Tresor")]
    VaultExists,
    #[error("Auf dem Server gibt es noch keinen Tresor")]
    NoVault,
    #[error("Der Keyring auf dem Server wurde inzwischen geändert")]
    KeyringChanged,
    #[error("Der Eintrag {0} lässt sich nach mehreren Versuchen nicht synchronisieren")]
    TooManyConflicts(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
}

pub type Result<T> = std::result::Result<T, SyncError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushResult {
    Accepted { seq: i64 },
    /// Der Server hat einen anderen Stand. `current` ist leer, wenn er den Eintrag nicht kennt.
    Conflict { current: Option<RemoteEntry> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<RemoteEntry>,
    pub latest: i64,
    pub more: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub pulled: usize,
    pub pushed: usize,
    pub merged: usize,
    pub conflicts: usize,
}

#[derive(Deserialize)]
struct WireEntry {
    id: String,
    seq: i64,
    blob: String,
}

impl WireEntry {
    fn into_remote(self) -> Result<RemoteEntry> {
        let blob = B64
            .decode(self.blob)
            .map_err(|_| SyncError::Protocol("Blob ist kein Base64".into()))?;
        Ok(RemoteEntry { id: self.id, seq: self.seq, blob })
    }
}

#[derive(Deserialize)]
struct WirePage {
    entries: Vec<WireEntry>,
    latest: i64,
    more: bool,
}

#[derive(Deserialize)]
struct WirePush {
    seq: i64,
}

#[derive(Deserialize)]
struct WireConflict {
    current: Option<WireEntry>,
}

#[derive(Deserialize)]
struct WireKeyring {
    version: i64,
    keyring: Value,
}

#[derive(Deserialize)]
struct WireVersion {
    version: i64,
}

#[derive(Serialize)]
struct PushBody<'a> {
    base_seq: i64,
    blob: &'a str,
}

#[derive(Serialize)]
struct KeyringBody<'a> {
    base_version: i64,
    keyring: &'a Value,
}

pub struct SyncClient {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl SyncClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            agent: config.into(),
        }
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn net(e: ureq::Error) -> SyncError {
        SyncError::Network(e.to_string())
    }

    /// Prüft das Ergebnis und gibt für 401 und unerwartete Status passende Fehler zurück.
    fn check(status: u16, expected: &[u16]) -> Result<()> {
        if expected.contains(&status) {
            Ok(())
        } else if status == 401 {
            Err(SyncError::Unauthorized)
        } else {
            Err(SyncError::Status(status))
        }
    }

    /// Keyring vom Server (`None`, wenn es noch keinen gibt) mit seiner Versionsnummer.
    pub fn get_keyring(&self) -> Result<Option<(Keyring, i64)>> {
        let mut resp = self
            .agent
            .get(&format!("{}/v1/keyring", self.base_url))
            .header("Authorization", self.auth())
            .call()
            .map_err(Self::net)?;
        let status = resp.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        Self::check(status, &[200])?;
        let wire: WireKeyring = resp
            .body_mut()
            .read_json()
            .map_err(|e| SyncError::Protocol(e.to_string()))?;
        let keyring = Keyring::from_json(&wire.keyring.to_string())?;
        Ok(Some((keyring, wire.version)))
    }

    /// Speichert den Keyring. `base_version` ist die Version, auf der die Änderung beruht (0 für den ersten).
    /// Gibt die neue Version zurück.
    pub fn put_keyring(&self, base_version: i64, keyring: &Keyring) -> Result<i64> {
        let value: Value = serde_json::from_str(&keyring.to_json())
            .map_err(|e| SyncError::Protocol(e.to_string()))?;
        let mut resp = self
            .agent
            .put(&format!("{}/v1/keyring", self.base_url))
            .header("Authorization", self.auth())
            .send_json(KeyringBody { base_version, keyring: &value })
            .map_err(Self::net)?;
        let status = resp.status().as_u16();
        if status == 409 {
            return Err(SyncError::KeyringChanged);
        }
        Self::check(status, &[200])?;
        let wire: WireVersion = resp
            .body_mut()
            .read_json()
            .map_err(|e| SyncError::Protocol(e.to_string()))?;
        Ok(wire.version)
    }

    /// Einträge mit Folgenummer größer als `since`.
    pub fn pull(&self, since: i64, limit: u32) -> Result<Page> {
        let mut resp = self
            .agent
            .get(&format!("{}/v1/entries?since={since}&limit={limit}", self.base_url))
            .header("Authorization", self.auth())
            .call()
            .map_err(Self::net)?;
        Self::check(resp.status().as_u16(), &[200])?;
        let wire: WirePage = resp
            .body_mut()
            .read_json()
            .map_err(|e| SyncError::Protocol(e.to_string()))?;
        let entries = wire
            .entries
            .into_iter()
            .map(WireEntry::into_remote)
            .collect::<Result<Vec<_>>>()?;
        Ok(Page { entries, latest: wire.latest, more: wire.more })
    }

    /// Schreibt einen Eintrag. `base_seq` ist die Folgenummer des Standes, auf dem er beruht (0 für neu).
    pub fn push(&self, id: &str, base_seq: i64, blob: &[u8]) -> Result<PushResult> {
        let encoded = B64.encode(blob);
        let mut resp = self
            .agent
            .put(&format!("{}/v1/entries/{id}", self.base_url))
            .header("Authorization", self.auth())
            .send_json(PushBody { base_seq, blob: &encoded })
            .map_err(Self::net)?;
        let status = resp.status().as_u16();
        if status == 409 {
            let wire: WireConflict = resp
                .body_mut()
                .read_json()
                .map_err(|e| SyncError::Protocol(e.to_string()))?;
            let current = wire.current.map(WireEntry::into_remote).transpose()?;
            return Ok(PushResult::Conflict { current });
        }
        Self::check(status, &[200])?;
        let wire: WirePush = resp
            .body_mut()
            .read_json()
            .map_err(|e| SyncError::Protocol(e.to_string()))?;
        Ok(PushResult::Accepted { seq: wire.seq })
    }
}

/// Legt einen neuen Tresor an: Datenschlüssel und Keyring erzeugen, Keyring auf dem Server und lokal ablegen.
/// Gibt den entsperrten Tresor und den Recovery-Key zurück, der dem Nutzer jetzt angezeigt werden muss.
pub fn create_vault(
    store: &Store,
    client: &SyncClient,
    password: &str,
    params: KdfParams,
) -> Result<(Vault, RecoveryKey)> {
    if client.get_keyring()?.is_some() {
        return Err(SyncError::VaultExists);
    }
    let (vault, keyring, recovery) = Vault::create(password, params)?;
    let version = client.put_keyring(0, &keyring)?;
    store.set_keyring(&keyring, version)?;
    Ok((vault, recovery))
}

/// Richtet einen weiteren Rechner ein: Keyring vom Server holen, mit dem Passwort entsperren und erst dann
/// lokal ablegen. Bei falschem Passwort bleibt lokal nichts zurück, der Nutzer kann es erneut versuchen.
pub fn join_vault(store: &Store, client: &SyncClient, password: &str) -> Result<Vault> {
    let (keyring, version) = client.get_keyring()?.ok_or(SyncError::NoVault)?;
    let vault = Vault::unlock_with_password(&keyring, password)?;
    store.set_keyring(&keyring, version)?;
    Ok(vault)
}

/// Gleicht lokalen Stand und Server ab.
pub fn sync(store: &mut Store, vault: &Vault, clock: &mut Clock, client: &SyncClient) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    pull_all(store, vault, clock, client, &mut report)?;

    for _ in 0..MAX_ROUNDS {
        let pending = store.pending()?;
        if pending.is_empty() {
            return Ok(report);
        }
        for item in pending {
            push_entry(store, vault, clock, client, &item.id, &mut report)?;
        }
    }
    // Neue Konflikteinträge entstehen nur beim Zusammenführen. Bleibt nach mehreren Runden etwas übrig,
    // wird es beim nächsten Abgleich gesendet.
    Ok(report)
}

fn pull_all(
    store: &mut Store,
    vault: &Vault,
    clock: &mut Clock,
    client: &SyncClient,
    report: &mut SyncReport,
) -> Result<()> {
    loop {
        let since = store.cursor()?;
        let page = client.pull(since, PAGE_SIZE)?;
        let mut highest = since;
        for entry in &page.entries {
            apply(store, vault, clock, entry, report)?;
            highest = highest.max(entry.seq);
        }
        store.set_cursor(highest)?;
        if !page.more || page.entries.is_empty() {
            return Ok(());
        }
    }
}

fn apply(
    store: &mut Store,
    vault: &Vault,
    clock: &mut Clock,
    entry: &RemoteEntry,
    report: &mut SyncReport,
) -> Result<()> {
    match store.apply_remote(vault, clock, entry)? {
        ApplyOutcome::Inserted | ApplyOutcome::Replaced => report.pulled += 1,
        ApplyOutcome::Merged { conflicts, .. } => {
            report.pulled += 1;
            report.merged += 1;
            report.conflicts += conflicts;
        }
        ApplyOutcome::Skipped => {}
    }
    Ok(())
}

fn push_entry(
    store: &mut Store,
    vault: &Vault,
    clock: &mut Clock,
    client: &SyncClient,
    id: &str,
    report: &mut SyncReport,
) -> Result<()> {
    for _ in 0..MAX_RETRIES_PER_ENTRY {
        let Some(item) = store.pending()?.into_iter().find(|p| p.id == id) else {
            return Ok(());
        };
        match client.push(&item.id, item.base_seq, &item.blob)? {
            PushResult::Accepted { seq } => {
                store.mark_pushed(&item.id, seq, &item.blob)?;
                report.pushed += 1;
                return Ok(());
            }
            PushResult::Conflict { current: Some(current) } => {
                apply(store, vault, clock, &current, report)?;
            }
            PushResult::Conflict { current: None } => {
                return Err(SyncError::Protocol(format!(
                    "Der Server kennt den Eintrag {id} nicht, obwohl lokal ein Serverstand vorliegt"
                )));
            }
        }
    }
    Err(SyncError::TooManyConflicts(id.to_string()))
}
