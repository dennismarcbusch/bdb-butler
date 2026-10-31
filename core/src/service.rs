//! Dienstschicht: der gemeinsame Einstiegspunkt für Oberfläche, MCP-Server und Tests.
//!
//! Ein [`Service`] gehört zu einem Datenordner (lokale Datenbank und `settings.json`). Er verwaltet den
//! Zustand „eingerichtet“, „gesperrt“ und „entsperrt“ und bietet Einträge nach den Definitionen aus
//! [`crate::schema`] an. Der Datenschlüssel bleibt im Speicher des Dienstes und verlässt ihn nicht.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::crypto::{self, KdfParams, Vault};
use crate::mcp::McpConfig;
use crate::record::{self, Clock, Record};
use crate::schema::{self, FieldType, KindDef};
use crate::store::{Store, StoreError, CONFLICT_KIND};
use crate::sync::{self, SyncClient, SyncError, SyncReport};

/// Mindestlänge des Master-Passworts in Zeichen.
pub const MIN_PASSWORD_CHARS: usize = 10;

const SETTINGS_FILE: &str = "settings.json";
const MCP_FILE: &str = "mcp.json";
const AUDIT_FILE: &str = "mcp-audit.log";
const DB_FILE: &str = "bdb.sqlite";
/// Ab dieser Größe wird das Protokoll des KI-Zugriffs rotiert.
const AUDIT_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Die App ist gesperrt")]
    Locked,
    #[error("Es gibt noch keinen Tresor")]
    NoVault,
    #[error("Es gibt schon einen Tresor")]
    VaultExists,
    #[error("Das Master-Passwort oder der Recovery-Key ist falsch")]
    WrongSecret,
    #[error("Das Master-Passwort muss mindestens {MIN_PASSWORD_CHARS} Zeichen haben")]
    WeakPassword,
    #[error("Server und Token sind noch nicht eingetragen")]
    NotConfigured,
    #[error("Unbekannter Typ „{0}“")]
    UnknownKind(String),
    #[error("Eintrag nicht gefunden")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("Auf dem Server liegt ein anderer Tresor als lokal")]
    OtherVaultOnServer,
    #[error("Einstellungen: {0}")]
    Settings(String),
    #[error(transparent)]
    Sync(#[from] SyncError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("Verschlüsselung: {0}")]
    Crypto(crypto::Error),
}

impl From<crypto::Error> for ServiceError {
    fn from(e: crypto::Error) -> Self {
        match e {
            crypto::Error::WrongSecret => ServiceError::WrongSecret,
            other => ServiceError::Crypto(other),
        }
    }
}

impl From<schema::ValueError> for ServiceError {
    fn from(e: schema::ValueError) -> Self {
        let schema::ValueError::Invalid(msg) = e;
        ServiceError::Invalid(msg)
    }
}

impl From<record::RecordError> for ServiceError {
    fn from(e: record::RecordError) -> Self {
        ServiceError::Store(StoreError::Record(e))
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

/// Einstellungen dieses Rechners. Sie werden nicht synchronisiert (Pfade gelten je Rechner).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_url: String,
    pub token: String,
    /// Ordner für PDFs und Anhänge (wird zum Beispiel von Nextcloud synchronisiert).
    pub attachments_dir: String,
}

impl Config {
    fn validate(&self) -> Result<()> {
        let url = self.server_url.trim();
        if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ServiceError::Settings("Die Server-Adresse muss mit http:// oder https:// beginnen".into()));
        }
        if !url.is_empty() && self.token.trim().is_empty() {
            return Err(ServiceError::Settings("Zum Server gehört ein Token".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Server und Token sind eingetragen.
    pub configured: bool,
    /// Lokal gibt es einen Tresor.
    pub has_vault: bool,
    pub unlocked: bool,
}

/// Ein Eintrag für die Oberfläche.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordView {
    pub id: String,
    pub kind: String,
    pub fields: Map<String, Value>,
}

struct Session {
    vault: Vault,
    clock: Clock,
}

pub struct Service {
    dir: PathBuf,
    store: Store,
    config: Config,
    mcp: McpConfig,
    kdf: KdfParams,
    session: Option<Session>,
}

impl Service {
    /// Öffnet den Datenordner (legt ihn an, wenn er fehlt). `kdf` bestimmt die Argon2id-Kosten beim Anlegen
    /// eines Tresors oder Ändern des Passworts.
    pub fn open(dir: &Path, kdf: KdfParams) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|e| ServiceError::Settings(e.to_string()))?;
        let store = Store::open(&dir.join(DB_FILE))?;
        let config = match fs::read(dir.join(SETTINGS_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| ServiceError::Settings(format!("settings.json ist ungültig: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(ServiceError::Settings(e.to_string())),
        };
        let mut service = Self { dir: dir.to_path_buf(), store, config, mcp: McpConfig::default(), kdf, session: None };
        service.load_mcp_config()?;
        Ok(service)
    }

    // ----- KI-Zugriff (MCP) ------------------------------------------------------------------------------

    fn write_private(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir.join(name);
        fs::write(&path, bytes).map_err(|e| ServiceError::Settings(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn load_mcp_config(&mut self) -> Result<()> {
        let mut config: McpConfig = match fs::read(self.dir.join(MCP_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| ServiceError::Settings(format!("mcp.json ist ungültig: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => McpConfig::default(),
            Err(e) => return Err(ServiceError::Settings(e.to_string())),
        };
        if config.token.is_empty() {
            config.token = McpConfig::new_token().map_err(|e| ServiceError::Settings(e.to_string()))?;
            self.mcp = config;
            self.save_mcp_config()?;
        } else {
            self.mcp = config;
        }
        Ok(())
    }

    fn save_mcp_config(&self) -> Result<()> {
        let json = serde_json::to_vec_pretty(&self.mcp).map_err(|e| ServiceError::Settings(e.to_string()))?;
        self.write_private(MCP_FILE, &json)
    }

    pub fn mcp_config(&self) -> &McpConfig {
        &self.mcp
    }

    /// Ändert Einstellungen des KI-Zugriffs. Das Token bleibt unverändert (siehe [`Service::new_mcp_token`]).
    pub fn set_mcp_config(&mut self, enabled: bool, port: u16, write: bool, allow_personal: &[String]) -> Result<()> {
        if port != 0 && port < 1024 {
            return Err(ServiceError::Settings("Der Port muss mindestens 1024 sein".into()));
        }
        let mut allowed: Vec<String> = Vec::new();
        for kind in allow_personal {
            match schema::kind(kind) {
                Some(def) if def.personal => {
                    if !allowed.contains(kind) {
                        allowed.push(kind.clone());
                    }
                }
                _ => return Err(ServiceError::Settings(format!("„{kind}“ ist kein Typ mit Personenbezug"))),
            }
        }
        self.mcp = McpConfig { enabled, port, write, allow_personal: allowed, token: self.mcp.token.clone() };
        self.save_mcp_config()
    }

    /// Erzeugt ein neues Token. Clients mit dem alten Token verlieren den Zugriff.
    pub fn new_mcp_token(&mut self) -> Result<()> {
        self.mcp.token = McpConfig::new_token().map_err(|e| ServiceError::Settings(e.to_string()))?;
        self.save_mcp_config()
    }

    /// Hängt einen Eintrag an das Protokoll des KI-Zugriffs an. Es enthält Werkzeug, Typ, ID und Feldnamen,
    /// aber nie Feldinhalte.
    pub fn audit(&self, entry: &Value) {
        let path = self.dir.join(AUDIT_FILE);
        if fs::metadata(&path).is_ok_and(|m| m.len() > AUDIT_MAX_BYTES) {
            let _ = fs::rename(&path, self.dir.join(format!("{AUDIT_FILE}.1")));
        }
        let mut line = entry.to_string();
        line.push('\n');
        let opened = fs::OpenOptions::new().create(true).append(true).open(&path);
        if let Ok(mut file) = opened {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            }
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// Die letzten `n` Einträge des Protokolls, neueste zuletzt.
    pub fn audit_tail(&self, n: usize) -> Vec<Value> {
        let text = fs::read_to_string(self.dir.join(AUDIT_FILE)).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(n);
        lines[start..].iter().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    // ----- Zustand und Einstellungen ---------------------------------------------------------------------

    pub fn status(&self) -> Result<Status> {
        Ok(Status {
            configured: self.is_configured(),
            has_vault: self.store.keyring()?.is_some(),
            unlocked: self.session.is_some(),
        })
    }

    fn is_configured(&self) -> bool {
        !self.config.server_url.trim().is_empty() && !self.config.token.trim().is_empty()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) -> Result<()> {
        let config = Config {
            server_url: config.server_url.trim().trim_end_matches('/').to_string(),
            token: config.token.trim().to_string(),
            attachments_dir: config.attachments_dir.trim().to_string(),
        };
        config.validate()?;
        let path = self.dir.join(SETTINGS_FILE);
        let json = serde_json::to_vec_pretty(&config).map_err(|e| ServiceError::Settings(e.to_string()))?;
        fs::write(&path, json).map_err(|e| ServiceError::Settings(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        self.config = config;
        Ok(())
    }

    fn client(&self) -> Result<SyncClient> {
        if !self.is_configured() {
            return Err(ServiceError::NotConfigured);
        }
        Ok(SyncClient::new(&self.config.server_url, &self.config.token))
    }

    // ----- Tresor ----------------------------------------------------------------------------------------

    /// Legt einen neuen Tresor an und entsperrt ihn. Ist ein Server eingetragen, wird der Keyring dort
    /// abgelegt, sonst erst beim ersten Abgleich. Gibt den Recovery-Key zurück, der jetzt angezeigt werden
    /// muss. Er wird nirgends gespeichert.
    pub fn create_vault(&mut self, password: &str) -> Result<String> {
        if self.store.keyring()?.is_some() {
            return Err(ServiceError::VaultExists);
        }
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(ServiceError::WeakPassword);
        }
        let (vault, recovery) = if self.is_configured() {
            match sync::create_vault(&self.store, &self.client()?, password, self.kdf) {
                Err(SyncError::VaultExists) => return Err(ServiceError::VaultExists),
                other => other?,
            }
        } else {
            let (vault, keyring, recovery) = Vault::create(password, self.kdf)?;
            self.store.set_keyring(&keyring, 0)?;
            (vault, recovery)
        };
        let clock = self.store.load_clock(&vault)?;
        self.session = Some(Session { vault, clock });
        Ok(recovery.to_display())
    }

    /// Richtet diesen Rechner mit dem Tresor vom Server ein und entsperrt ihn.
    pub fn join_vault(&mut self, password: &str) -> Result<()> {
        if self.store.keyring()?.is_some() {
            return Err(ServiceError::VaultExists);
        }
        let vault = match sync::join_vault(&self.store, &self.client()?, password) {
            Err(SyncError::NoVault) => return Err(ServiceError::NoVault),
            // Bei falschem Passwort bleibt lokal nichts zurück, der Nutzer kann es erneut versuchen.
            Err(SyncError::Crypto(e)) => return Err(e.into()),
            other => other?,
        };
        let clock = self.store.load_clock(&vault)?;
        self.session = Some(Session { vault, clock });
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let (keyring, _) = self.store.keyring()?.ok_or(ServiceError::NoVault)?;
        let vault = Vault::unlock_with_password(&keyring, password)?;
        self.start_session(vault)
    }

    pub fn unlock_with_recovery(&mut self, recovery_key: &str) -> Result<()> {
        let (keyring, _) = self.store.keyring()?.ok_or(ServiceError::NoVault)?;
        let vault = Vault::unlock_with_recovery(&keyring, recovery_key).map_err(|e| match e {
            crypto::Error::InvalidRecoveryKey => ServiceError::Invalid("Der Recovery-Key hat ein ungültiges Format".into()),
            other => other.into(),
        })?;
        self.start_session(vault)
    }

    fn start_session(&mut self, vault: Vault) -> Result<()> {
        let clock = self.store.load_clock(&vault)?;
        self.session = Some(Session { vault, clock });
        Ok(())
    }

    pub fn lock(&mut self) {
        self.session = None;
    }

    /// Ändert das Master-Passwort. Der Recovery-Key bleibt gültig, die Daten bleiben unverändert.
    pub fn change_password(&mut self, old_password: &str, new_password: &str) -> Result<()> {
        if self.session.is_none() {
            return Err(ServiceError::Locked);
        }
        if new_password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(ServiceError::WeakPassword);
        }
        let (keyring, version) = self.store.keyring()?.ok_or(ServiceError::NoVault)?;
        let vault = Vault::unlock_with_password(&keyring, old_password)?;
        let new_keyring = vault.change_password(&keyring, new_password, self.kdf)?;
        let new_version = if version > 0 {
            self.client()?.put_keyring(version, &new_keyring)?
        } else {
            0
        };
        self.store.set_keyring(&new_keyring, new_version)?;
        Ok(())
    }

    // ----- Abgleich --------------------------------------------------------------------------------------

    /// Gleicht mit dem Server ab. Existiert der Keyring noch nicht auf dem Server (Tresor offline angelegt),
    /// wird er dabei hochgeladen.
    pub fn sync(&mut self) -> Result<SyncReport> {
        let client = self.client()?;
        let session = self.session.as_mut().ok_or(ServiceError::Locked)?;

        if let Some((keyring, 0)) = self.store.keyring()? {
            match client.get_keyring()? {
                None => {
                    let version = client.put_keyring(0, &keyring)?;
                    self.store.set_keyring(&keyring, version)?;
                }
                Some((server_keyring, version)) if server_keyring == keyring => {
                    self.store.set_keyring(&keyring, version)?;
                }
                Some(_) => return Err(ServiceError::OtherVaultOnServer),
            }
        }
        Ok(sync::sync(&mut self.store, &session.vault, &mut session.clock, &client)?)
    }

    // ----- Einträge --------------------------------------------------------------------------------------

    fn unlocked(&mut self) -> Result<(&Store, &Vault, &mut Clock)> {
        let session = self.session.as_mut().ok_or(ServiceError::Locked)?;
        Ok((&self.store, &session.vault, &mut session.clock))
    }

    pub fn kinds(&self) -> Vec<KindDef> {
        schema::kinds()
    }

    /// Alle nicht gelöschten Einträge eines Typs, nach Titel sortiert.
    pub fn list(&mut self, kind: &str) -> Result<Vec<RecordView>> {
        let def = schema::kind(kind).ok_or_else(|| ServiceError::UnknownKind(kind.to_string()))?;
        let (store, vault, _) = self.unlocked()?;
        let mut out: Vec<RecordView> = store
            .list(vault)?
            .into_iter()
            .filter(|(_, rec)| rec.kind() == kind && !rec.is_deleted())
            .map(|(id, rec)| view(id, &rec))
            .collect();
        sort_views(&def, &mut out);
        Ok(out)
    }

    /// Alle nicht gelöschten Einträge aller Typen (zum Beispiel für den Graphen und die Suche).
    pub fn list_all(&mut self) -> Result<Vec<RecordView>> {
        let (store, vault, _) = self.unlocked()?;
        Ok(store
            .list(vault)?
            .into_iter()
            .filter(|(_, rec)| rec.kind() != CONFLICT_KIND && !rec.is_deleted())
            .map(|(id, rec)| view(id, &rec))
            .collect())
    }

    pub fn get(&mut self, id: &str) -> Result<RecordView> {
        let (store, vault, _) = self.unlocked()?;
        match store.get_record(vault, id)? {
            Some(rec) if !rec.is_deleted() => Ok(view(id.to_string(), &rec)),
            _ => Err(ServiceError::NotFound),
        }
    }

    /// Legt einen Eintrag an (`id` leer) oder ändert ihn. Nur die übergebenen Felder werden geschrieben, und
    /// nur wenn sich der Wert ändert. Leere Texte und `null` leeren das Feld. Gibt die ID zurück.
    pub fn save(&mut self, kind: &str, id: Option<&str>, values: &Map<String, Value>) -> Result<String> {
        let def = schema::kind(kind).ok_or_else(|| ServiceError::UnknownKind(kind.to_string()))?;
        let (store, vault, clock) = self.unlocked()?;

        let existing = match id {
            Some(id) => match store.get_record(vault, id)? {
                Some(rec) if rec.kind() == kind && !rec.is_deleted() => Some(rec),
                _ => return Err(ServiceError::NotFound),
            },
            None => None,
        };

        // Prüfen und normalisieren, bevor etwas geschrieben wird.
        let mut normalized: Vec<(&'static str, Value)> = Vec::new();
        for name in values.keys() {
            if !def.fields.iter().any(|f| f.name == name) {
                return Err(ServiceError::Invalid(format!("Unbekanntes Feld „{name}“")));
            }
        }
        for field in &def.fields {
            let Some(value) = values.get(field.name) else { continue };
            let value = schema::normalize(field, value)?;
            if field.required && value.is_null() {
                return Err(ServiceError::Invalid(format!("{} ist ein Pflichtfeld", field.label)));
            }
            check_references(store, vault, &field.field_type, field.label, &value)?;
            normalized.push((field.name, value));
        }
        if existing.is_none() {
            for field in def.fields.iter().filter(|f| f.required) {
                if !normalized.iter().any(|(n, v)| *n == field.name && !v.is_null()) {
                    return Err(ServiceError::Invalid(format!("{} ist ein Pflichtfeld", field.label)));
                }
            }
        }

        let is_new = existing.is_none();
        let (id, mut rec) = match (id, existing) {
            (Some(id), Some(rec)) => (id.to_string(), rec),
            _ => (record::new_id()?, Record::new(kind)?),
        };
        let mut changed = false;
        for (name, value) in normalized {
            let current = rec.get(name).cloned().unwrap_or(Value::Null);
            if current != value {
                let stamp = clock.tick(record::now_ms());
                rec.set(name, value, stamp)?;
                changed = true;
            }
        }
        if changed || is_new {
            store.put_record(vault, &id, &rec)?;
        }
        Ok(id)
    }

    /// Markiert einen Eintrag (auch einen Konflikt) als gelöscht.
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let (store, vault, clock) = self.unlocked()?;
        let mut rec = match store.get_record(vault, id)? {
            Some(rec) if !rec.is_deleted() => rec,
            _ => return Err(ServiceError::NotFound),
        };
        rec.set_deleted(true, clock.tick(record::now_ms()));
        store.put_record(vault, id, &rec)?;
        Ok(())
    }

    /// Offene Konflikte aus dem Abgleich (siehe `docs/ARCHITEKTUR.md`).
    pub fn conflicts(&mut self) -> Result<Vec<RecordView>> {
        let (store, vault, _) = self.unlocked()?;
        Ok(store
            .list(vault)?
            .into_iter()
            .filter(|(_, rec)| rec.kind() == CONFLICT_KIND && !rec.is_deleted())
            .map(|(id, rec)| view(id, &rec))
            .collect())
    }
}

/// Sortiert nach dem Sortierfeld des Typs (bei Daten auch zeitlich richtig), bei Gleichstand nach Titel.
fn sort_views(def: &KindDef, views: &mut [RecordView]) {
    let text = |v: &RecordView, field: &str| v.fields.get(field).and_then(Value::as_str).unwrap_or("").to_lowercase();
    views.sort_by(|a, b| {
        let primary = text(a, def.sort_field).cmp(&text(b, def.sort_field));
        let primary = if def.sort_desc { primary.reverse() } else { primary };
        primary.then_with(|| text(a, def.title_field).cmp(&text(b, def.title_field)))
    });
}

fn view(id: String, rec: &Record) -> RecordView {
    RecordView {
        id,
        kind: rec.kind().to_string(),
        fields: rec.values().map(|(k, v)| (k.to_string(), v.clone())).collect(),
    }
}

/// Verweise müssen auf vorhandene, nicht gelöschte Einträge des richtigen Typs zeigen.
fn check_references(store: &Store, vault: &Vault, field_type: &FieldType, label: &str, value: &Value) -> Result<()> {
    let (target, ids): (&str, Vec<&str>) = match (field_type, value) {
        (FieldType::Ref { kind }, Value::String(id)) => (kind, vec![id.as_str()]),
        (FieldType::RefList { kind }, Value::Array(items)) => (kind, items.iter().filter_map(Value::as_str).collect()),
        _ => return Ok(()),
    };
    for id in ids {
        match store.get_record(vault, id)? {
            Some(rec) if rec.kind() == target && !rec.is_deleted() => {}
            _ => return Err(ServiceError::Invalid(format!("{label}: verweist auf einen Eintrag, den es nicht gibt"))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
    const PASSWORD: &str = "korrekt pferd batterie heftklammer";

    fn tempdir() -> PathBuf {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-service-test-{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn unlocked_service() -> (Service, String) {
        let mut s = Service::open(&tempdir(), FAST).unwrap();
        let recovery = s.create_vault(PASSWORD).unwrap();
        (s, recovery)
    }

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn fresh_service_has_no_vault() {
        let s = Service::open(&tempdir(), FAST).unwrap();
        assert_eq!(s.status().unwrap(), Status { configured: false, has_vault: false, unlocked: false });
    }

    #[test]
    fn create_lock_unlock_cycle() {
        let (mut s, _) = unlocked_service();
        assert_eq!(s.status().unwrap(), Status { configured: false, has_vault: true, unlocked: true });
        let id = s.save("schule", None, &map(json!({"name": "Grundschule"}))).unwrap();

        s.lock();
        assert!(!s.status().unwrap().unlocked);
        assert!(matches!(s.list("schule"), Err(ServiceError::Locked)));
        assert!(matches!(s.get(&id), Err(ServiceError::Locked)));
        assert!(matches!(s.sync(), Err(ServiceError::NotConfigured)));

        assert!(matches!(s.unlock("falsches passwort"), Err(ServiceError::WrongSecret)));
        s.unlock(PASSWORD).unwrap();
        assert_eq!(s.get(&id).unwrap().fields["name"], "Grundschule");
    }

    #[test]
    fn password_rules() {
        let mut s = Service::open(&tempdir(), FAST).unwrap();
        assert!(matches!(s.create_vault("kurz"), Err(ServiceError::WeakPassword)));
        s.create_vault(PASSWORD).unwrap();
        assert!(matches!(s.create_vault(PASSWORD), Err(ServiceError::VaultExists)));
    }

    #[test]
    fn recovery_key_unlocks() {
        let (mut s, recovery) = unlocked_service();
        let id = s.save("schule", None, &map(json!({"name": "A"}))).unwrap();
        s.lock();

        assert!(matches!(s.unlock_with_recovery("kein recovery key"), Err(ServiceError::Invalid(_))));
        let other = crypto::Vault::create(PASSWORD, FAST).unwrap().2.to_display();
        assert!(matches!(s.unlock_with_recovery(&other), Err(ServiceError::WrongSecret)));
        s.unlock_with_recovery(&recovery).unwrap();
        assert_eq!(s.get(&id).unwrap().fields["name"], "A");
    }

    #[test]
    fn data_and_vault_survive_restart() {
        let dir = tempdir();
        let mut s = Service::open(&dir, FAST).unwrap();
        s.create_vault(PASSWORD).unwrap();
        let id = s.save("schule", None, &map(json!({"name": "Dauerhaft"}))).unwrap();
        drop(s);

        let mut s = Service::open(&dir, FAST).unwrap();
        assert_eq!(s.status().unwrap(), Status { configured: false, has_vault: true, unlocked: false });
        s.unlock(PASSWORD).unwrap();
        assert_eq!(s.get(&id).unwrap().fields["name"], "Dauerhaft");
    }

    #[test]
    fn change_password_keeps_data_and_recovery() {
        let (mut s, recovery) = unlocked_service();
        let id = s.save("schule", None, &map(json!({"name": "A"}))).unwrap();

        assert!(matches!(s.change_password("falsch", "ein neues langes passwort"), Err(ServiceError::WrongSecret)));
        assert!(matches!(s.change_password(PASSWORD, "kurz"), Err(ServiceError::WeakPassword)));
        s.change_password(PASSWORD, "ein neues langes passwort").unwrap();

        s.lock();
        assert!(matches!(s.unlock(PASSWORD), Err(ServiceError::WrongSecret)));
        s.unlock("ein neues langes passwort").unwrap();
        assert_eq!(s.get(&id).unwrap().fields["name"], "A");
        s.lock();
        s.unlock_with_recovery(&recovery).unwrap();
    }

    #[test]
    fn settings_are_validated_and_persisted() {
        let dir = tempdir();
        let mut s = Service::open(&dir, FAST).unwrap();
        assert!(s.set_config(Config { server_url: "ftp://x".into(), token: "t".into(), ..Config::default() }).is_err());
        assert!(s.set_config(Config { server_url: "https://x.example".into(), token: "".into(), ..Config::default() }).is_err());

        s.set_config(Config {
            server_url: " https://butler.example/ ".into(),
            token: " geheim ".into(),
            attachments_dir: "/home/x/Nextcloud/BdB".into(),
        })
        .unwrap();
        assert_eq!(s.config().server_url, "https://butler.example");
        assert_eq!(s.config().token, "geheim");
        assert!(s.status().unwrap().configured);
        drop(s);

        let s = Service::open(&dir, FAST).unwrap();
        assert_eq!(s.config().attachments_dir, "/home/x/Nextcloud/BdB");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(SETTINGS_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "settings.json darf nur dem Nutzer gehören");
        }
    }

    #[test]
    fn save_validates_input() {
        let (mut s, _) = unlocked_service();
        assert!(matches!(s.save("gibt-es-nicht", None, &Map::new()), Err(ServiceError::UnknownKind(_))));
        assert!(matches!(s.save("schule", None, &map(json!({}))), Err(ServiceError::Invalid(_))), "Pflichtfeld fehlt");
        assert!(matches!(s.save("schule", None, &map(json!({"name": "  "}))), Err(ServiceError::Invalid(_))));
        assert!(matches!(s.save("schule", None, &map(json!({"name": "A", "unsinn": 1}))), Err(ServiceError::Invalid(_))));
        assert!(matches!(s.save("schule", None, &map(json!({"name": "A", "schulart": "Zirkus"}))), Err(ServiceError::Invalid(_))));
        assert!(s.list("schule").unwrap().is_empty(), "Fehlerhafte Eingaben dürfen nichts speichern");
    }

    #[test]
    fn update_changes_only_given_fields() {
        let (mut s, _) = unlocked_service();
        let id = s
            .save("schule", None, &map(json!({"name": "Alt", "ort": "Ulm", "schulart": "Gymnasium"})))
            .unwrap();

        s.save("schule", Some(&id), &map(json!({"name": "Neu"}))).unwrap();
        let v = s.get(&id).unwrap();
        assert_eq!(v.fields["name"], "Neu");
        assert_eq!(v.fields["ort"], "Ulm");

        // Leeren eines Feldes.
        s.save("schule", Some(&id), &map(json!({"ort": ""}))).unwrap();
        assert!(!s.get(&id).unwrap().fields.contains_key("ort"));
        // Pflichtfelder lassen sich nicht leeren.
        assert!(matches!(s.save("schule", Some(&id), &map(json!({"name": ""}))), Err(ServiceError::Invalid(_))));
        // Falscher Typ oder unbekannte ID.
        assert!(matches!(s.save("ansprechpartner", Some(&id), &map(json!({"name": "X"}))), Err(ServiceError::NotFound)));
        assert!(matches!(s.save("schule", Some("gibt-es-nicht"), &map(json!({"name": "X"}))), Err(ServiceError::NotFound)));
    }

    #[test]
    fn unchanged_save_writes_nothing() {
        let (mut s, _) = unlocked_service();
        let id = s.save("schule", None, &map(json!({"name": "A"}))).unwrap();
        let blob_before = s.store.pending().unwrap().remove(0).blob;
        s.save("schule", Some(&id), &map(json!({"name": "A"}))).unwrap();
        assert_eq!(s.store.pending().unwrap().remove(0).blob, blob_before);
    }

    #[test]
    fn list_sorts_by_title_and_hides_deleted() {
        let (mut s, _) = unlocked_service();
        let b = s.save("schule", None, &map(json!({"name": "berg"}))).unwrap();
        s.save("schule", None, &map(json!({"name": "Zeppelin"}))).unwrap();
        s.save("schule", None, &map(json!({"name": "Anna"}))).unwrap();

        let names: Vec<_> = s.list("schule").unwrap().iter().map(|v| v.fields["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["Anna", "berg", "Zeppelin"]);

        s.delete(&b).unwrap();
        assert_eq!(s.list("schule").unwrap().len(), 2);
        assert!(matches!(s.get(&b), Err(ServiceError::NotFound)));
        assert!(matches!(s.delete(&b), Err(ServiceError::NotFound)));
        assert!(matches!(s.save("schule", Some(&b), &map(json!({"name": "x"}))), Err(ServiceError::NotFound)));
    }

    #[test]
    fn lists_are_sorted_by_the_sort_field_of_the_kind() {
        let (mut s, _) = unlocked_service();
        // Beratungen: neueste zuerst, bei gleichem Datum nach Thema.
        for (thema, datum) in [("Beta", "2026-03-01"), ("Alpha", "2026-03-01"), ("Älteste", "2025-12-24"), ("Neueste", "2026-10-31")] {
            s.save("beratung", None, &map(json!({"thema": thema, "datum": datum}))).unwrap();
        }
        let themen: Vec<_> = s.list("beratung").unwrap().iter().map(|v| v.fields["thema"].as_str().unwrap().to_string()).collect();
        assert_eq!(themen, ["Neueste", "Alpha", "Beta", "Älteste"]);
    }

    #[test]
    fn markdown_texts_and_new_kinds_are_saved() {
        let (mut s, _) = unlocked_service();
        let schule = s.save("schule", None, &map(json!({"name": "Grundschule am See"}))).unwrap();
        let person = s.save("beratene_person", None, &map(json!({"name": "Herr Müller", "schule": schule}))).unwrap();
        let beratung = s
            .save("beratung", None, &map(json!({"thema": "Tablet-Einsatz", "datum": "2026-10-30", "art": "Vor Ort", "dauer": 90, "schule": schule, "personen": [person], "inhalt": "# Ergebnis\n- [ ] Konzept schicken"})))
            .unwrap();
        let protokoll = s
            .save("protokoll", None, &map(json!({"titel": "Beratung Tablets", "art": "Beratung", "datum": "2026-10-30", "beratung": beratung, "text": "Siehe [[Grundschule am See]]"})))
            .unwrap();
        assert_eq!(s.get(&protokoll).unwrap().fields["text"], "Siehe [[Grundschule am See]]");
        assert_eq!(s.get(&beratung).unwrap().fields["dauer"], 90);
        // Pflichtfelder und Auswahl der neuen Typen.
        assert!(matches!(s.save("protokoll", None, &map(json!({"titel": "X", "datum": "2026-10-30"}))), Err(ServiceError::Invalid(_))), "Art ist Pflicht");
        assert!(matches!(s.save("beratung", None, &map(json!({"thema": "X", "datum": "30.10.2026"}))), Err(ServiceError::Invalid(_))), "ungültiges Datum");
        assert!(matches!(s.save("beratung", None, &map(json!({"thema": "X", "datum": "2026-10-30", "personen": [schule]}))), Err(ServiceError::Invalid(_))), "falscher Typ im Verweis");
    }

    #[test]
    fn references_must_point_to_existing_entries_of_the_right_kind() {
        let (mut s, _) = unlocked_service();
        let schule = s.save("schule", None, &map(json!({"name": "Schule"}))).unwrap();
        let person = s
            .save("ansprechpartner", None, &map(json!({"name": "Frau Weber", "schule": schule})))
            .unwrap();
        assert_eq!(s.get(&person).unwrap().fields["schule"], json!(schule));

        // Verweis auf nicht vorhandenen Eintrag.
        assert!(matches!(
            s.save("ansprechpartner", None, &map(json!({"name": "X", "schule": "gibt-es-nicht"}))),
            Err(ServiceError::Invalid(_))
        ));
        // Verweis auf Eintrag des falschen Typs.
        assert!(matches!(
            s.save("ansprechpartner", None, &map(json!({"name": "X", "schule": person}))),
            Err(ServiceError::Invalid(_))
        ));
        // Listen von Verweisen.
        let kollege = s.save("ansprechpartner", None, &map(json!({"name": "Herr Bauer", "verknuepfungen": [person]}))).unwrap();
        assert_eq!(s.get(&kollege).unwrap().fields["verknuepfungen"], json!([person]));
        // Verweis auf gelöschten Eintrag.
        s.delete(&person).unwrap();
        assert!(matches!(
            s.save("ansprechpartner", None, &map(json!({"name": "Y", "verknuepfungen": [person]}))),
            Err(ServiceError::Invalid(_))
        ));
    }

    #[test]
    fn list_all_excludes_deleted_and_conflicts() {
        let (mut s, _) = unlocked_service();
        let a = s.save("schule", None, &map(json!({"name": "A"}))).unwrap();
        s.save("schule", None, &map(json!({"name": "B"}))).unwrap();
        s.delete(&a).unwrap();
        assert_eq!(s.list_all().unwrap().len(), 1);
        assert!(s.conflicts().unwrap().is_empty());
    }
}
