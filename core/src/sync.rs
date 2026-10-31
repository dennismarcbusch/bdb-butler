//! Sync-Client für den Go-Server (Protokoll: `docs/ARCHITEKTUR.md`).
//!
//! [`sync`] holt zuerst alle neuen Einträge vom Server und spielt sie lokal ein (mit Zusammenführung),
//! dann sendet es die lokalen Änderungen. Antwortet der Server mit einem Konflikt (409), wird der aktuelle
//! Serverstand eingespielt, zusammengeführt und erneut gesendet.
//!
//! Vor dem Holen prüft [`sync`], ob der Server noch zu den Folgenummern dieses Rechners passt. Hat er Stände
//! verloren (leer, ältere Sicherung), bricht der Abgleich mit [`SyncError::ServerReset`] ab, bevor etwas
//! eingespielt oder gesendet wird. Dann hilft nur „Server neu befüllen“ (siehe `Service::refill_server`).

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
    #[error("Die App wurde während des Abgleichs gesperrt")]
    Locked,
    #[error("{0}")]
    ServerReset(ResetReason),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Warum der Server nicht mehr zu den Folgenummern dieses Rechners passt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    /// Auf dem Server liegt kein Keyring mehr (neu aufgesetzt, Datenbank verloren).
    Empty,
    /// Der Server hat einen älteren Stand als dieser Rechner (zum Beispiel aus einer Sicherung zurückgespielt).
    Behind,
    /// Ein anderer Rechner hat den Server neu befüllt (neue Bestandskennung im Keyring).
    Refilled,
}

impl std::fmt::Display for ResetReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ResetReason::Empty => {
                "Der Server ist leer, er hat seinen Bestand verloren. Dieser Rechner kann ihn mit „Server neu \
                 befüllen“ (Einstellungen, Abgleich) wieder in Betrieb nehmen."
            }
            ResetReason::Behind => {
                "Der Server hat einen älteren Stand als dieser Rechner (zum Beispiel aus einer Sicherung \
                 zurückgespielt). Bitte in den Einstellungen unter Abgleich „Server neu befüllen“ wählen."
            }
            ResetReason::Refilled => {
                "Ein anderer Rechner hat den Server neu befüllt. Bitte auch hier in den Einstellungen unter \
                 Abgleich „Server neu befüllen“ wählen."
            }
        })
    }
}

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
    /// Hinweise für den Nutzer, zum Beispiel zu Einträgen, die sich nicht einspielen ließen.
    pub warnings: Vec<String>,
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

/// Zugriff auf die lokale Datenbank während eines Abgleichs. Jeder Aufruf von [`Local::with`] ist ein kurzer
/// Abschnitt ohne Netzwerk. Dazwischen laufen die Anfragen an den Server, damit die App (Oberfläche und KI-Zugriff)
/// nicht auf den Server warten muss. Das Protokoll verträgt Änderungen zwischen zwei Abschnitten: Einspielen führt
/// mit lokalen Änderungen zusammen, und ein Eintrag, der sich während des Sendens ändert, bleibt vorgemerkt.
pub trait Local {
    fn with<R>(&mut self, f: impl FnOnce(&mut Store, &Vault, &mut Clock) -> Result<R>) -> Result<R>;
}

/// Direkter Zugriff (Tests, Werkzeuge): alles in einem Stück.
struct Direct<'a> {
    store: &'a mut Store,
    vault: &'a Vault,
    clock: &'a mut Clock,
}

impl Local for Direct<'_> {
    fn with<R>(&mut self, f: impl FnOnce(&mut Store, &Vault, &mut Clock) -> Result<R>) -> Result<R> {
        f(self.store, self.vault, self.clock)
    }
}

/// Gleicht lokalen Stand und Server ab.
pub fn sync(store: &mut Store, vault: &Vault, clock: &mut Clock, client: &SyncClient) -> Result<SyncReport> {
    sync_with(&mut Direct { store, vault, clock }, client)
}

/// Wie [`sync`], greift aber über [`Local`] in kurzen Abschnitten auf die Datenbank zu.
pub fn sync_with(local: &mut impl Local, client: &SyncClient) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    check_server(local, client)?;
    pull_all(local, client, &mut report)?;

    for _ in 0..MAX_ROUNDS {
        let pending = local.with(|store, _, _| Ok(store.pending_ids()?))?;
        if pending.is_empty() {
            return Ok(report);
        }
        for id in pending {
            push_entry(local, client, &id, &mut report)?;
        }
    }
    // Neue Konflikteinträge entstehen nur beim Zusammenführen. Bleibt nach mehreren Runden etwas übrig,
    // wird es beim nächsten Abgleich gesendet.
    Ok(report)
}

/// Passt der Server noch zu dem, was dieser Rechner von ihm weiß? Zwei Prüfungen mit einer Anfrage:
///
/// - `latest` des Servers darf nicht unter der höchsten bekannten Folgenummer liegen ([`Store::known_seq`]).
///   Sonst fehlen dem Server Stände, und neue Schreibvorgänge bekämen Folgenummern, die dieser Rechner für bekannt
///   hält und nie holen würde.
/// - Unter der Folgenummer des Zeugen ([`Store::witness`]) liegt auf einem unveränderten Server genau dessen Blob
///   oder nichts (der Eintrag wurde seitdem neu geschrieben). Ein anderer Eintrag oder Blob dort heißt: Der Server
///   wurde zurückgespielt, und danach wurde schon wieder geschrieben.
fn check_server(local: &mut impl Local, client: &SyncClient) -> Result<()> {
    let (known, witness) = local.with(|store, _, _| Ok((store.known_seq()?, store.witness()?)))?;
    if known == 0 {
        return Ok(());
    }
    let since = witness.as_ref().map_or(known, |w| w.seq - 1);
    let page = client.pull(since, 1)?;
    if page.latest < known {
        return Err(SyncError::ServerReset(ResetReason::Behind));
    }
    if let (Some(witness), Some(found)) = (&witness, page.entries.first()) {
        if found.seq == witness.seq && (found.id != witness.id || found.blob != witness.blob) {
            return Err(SyncError::ServerReset(ResetReason::Behind));
        }
    }
    Ok(())
}

/// Holt alle neuen Einträge. Ein Eintrag, der sich nicht lesen oder einspielen lässt (beschädigt, aus einer neueren
/// App-Fassung, mit anderem Typ als lokal), hält den Abgleich nicht auf: Er wird übersprungen und gemeldet. Der
/// gespeicherte Cursor bleibt vor dem ersten übersprungenen Eintrag stehen, damit er beim nächsten Abgleich (zum
/// Beispiel nach einem Update der App) erneut versucht wird. Alle anderen Einträge werden trotzdem eingespielt.
fn pull_all(local: &mut impl Local, client: &SyncClient, report: &mut SyncReport) -> Result<()> {
    let mut since = local.with(|store, _, _| Ok(store.cursor()?))?;
    let mut keep_before: Option<i64> = None;
    let mut skipped: Vec<String> = Vec::new();
    loop {
        let page = client.pull(since, PAGE_SIZE)?;
        local.with(|store, vault, clock| {
            for entry in &page.entries {
                match apply(store, vault, clock, entry, report) {
                    Ok(()) => {}
                    Err(SyncError::Store(StoreError::Crypto(_) | StoreError::Record(_))) => {
                        skipped.push(entry.id.clone());
                        keep_before.get_or_insert(entry.seq - 1);
                    }
                    Err(e) => return Err(e),
                }
                since = since.max(entry.seq);
            }
            store.set_cursor(keep_before.unwrap_or(since))?;
            Ok(())
        })?;
        if !page.more || page.entries.is_empty() {
            break;
        }
    }
    if !skipped.is_empty() {
        let shown: Vec<&str> = skipped.iter().take(5).map(String::as_str).collect();
        report.warnings.push(format!(
            "{} Einträge vom Server ließen sich nicht einspielen (beschädigt oder aus einer neueren App-Fassung) und \
             werden beim nächsten Abgleich erneut versucht: {}{}",
            skipped.len(),
            shown.join(", "),
            if skipped.len() > shown.len() { " …" } else { "" },
        ));
    }
    Ok(())
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

fn push_entry(local: &mut impl Local, client: &SyncClient, id: &str, report: &mut SyncReport) -> Result<()> {
    for _ in 0..MAX_RETRIES_PER_ENTRY {
        let Some(item) = local.with(|store, _, _| Ok(store.pending_entry(id)?))? else {
            return Ok(());
        };
        match client.push(&item.id, item.base_seq, &item.blob)? {
            PushResult::Accepted { seq } => {
                local.with(|store, _, _| Ok(store.mark_pushed(&item.id, seq, &item.blob)?))?;
                report.pushed += 1;
                return Ok(());
            }
            PushResult::Conflict { current: Some(current) } => {
                local.with(|store, vault, clock| apply(store, vault, clock, &current, report))?;
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
