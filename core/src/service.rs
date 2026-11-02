//! Dienstschicht: der gemeinsame Einstiegspunkt für Oberfläche, MCP-Server und Tests.
//!
//! Ein [`Service`] gehört zu einem Datenordner (lokale Datenbank und `settings.json`). Er verwaltet den
//! Zustand „eingerichtet“, „gesperrt“ und „entsperrt“ und bietet Einträge nach den Definitionen aus
//! [`crate::schema`] an. Der Datenschlüssel bleibt im Speicher des Dienstes und verlässt ihn nicht.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::crypto::{self, KdfParams, Vault};
use crate::mcp::McpConfig;
use crate::record::{self, Clock, Record};
use crate::schema::{self, FieldType, KindDef};
use crate::secrets::{self, MCP_TOKEN, SMTP_PASSWORD, SYNC_TOKEN};
use crate::store::{Store, StoreError, CONFLICT_KIND};
use crate::sync::{self, ResetReason, SyncClient, SyncError, SyncReport};

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
///
/// `settings.json` enthält nur Server-Adresse und Ordner. Das Token liegt verschlüsselt in der lokalen Datenbank
/// (siehe [`crate::secrets`]) und steht nur bei entsperrter App im Speicher. Ein Token aus einer älteren
/// `settings.json` wird beim ersten Entsperren übernommen und aus der Datei entfernt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_url: String,
    pub token: String,
    /// Ordner für PDFs und Anhänge (wird zum Beispiel von Nextcloud synchronisiert).
    pub attachments_dir: String,
}

/// Inhalt von `settings.json` (ohne Token).
#[derive(Serialize)]
struct SettingsFile<'a> {
    server_url: &'a str,
    attachments_dir: &'a str,
}

/// Schreibt eine Datei, die nur der Nutzer lesen darf. Sie entsteht gleich mit den Rechten 0600 (unter Unix)
/// als Nebendatei und ersetzt dann die alte, damit sie nie mit weiteren Rechten oder halb geschrieben vorliegt.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
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

/// Nächster Schritt beim Abgleich des Keyrings (siehe [`Service::keyring_plan`]).
enum KeyringStep {
    /// Nichts hochzuladen, gegebenenfalls mit Hinweisen.
    Done(Vec<String>),
    /// Diesen Keyring hochladen. `initial`: Es ist der erste Keyring auf dem Server (ein Fehler bricht ab).
    Upload { keyring: crypto::Keyring, base_version: i64, initial: bool },
}

/// Mit welchem Keyring es nach dem Zurücksetzen weitergeht (siehe [`Service::refill_plan`]).
enum RefillStep {
    /// Ein anderer Rechner hat den Server schon neu befüllt: dessen Keyring übernehmen.
    Adopt { keyring: crypto::Keyring, version: i64 },
    /// Dieser Rechner befüllt den Server: Keyring mit neuer Bestandskennung hochladen.
    Upload { keyring: crypto::Keyring, base_version: i64 },
}

/// Zugriff auf die Datenbank eines geteilten Dienstes in kurzen Abschnitten (siehe [`Service::sync_shared`]).
struct Shared<'a>(&'a Mutex<Service>);

impl sync::Local for Shared<'_> {
    fn with<R>(&mut self, f: impl FnOnce(&mut Store, &Vault, &mut Clock) -> sync::Result<R>) -> sync::Result<R> {
        let mut guard = self.0.lock().map_err(|_| SyncError::Protocol("Interner Fehler: Sperre vergiftet".into()))?;
        let service = &mut *guard;
        let session = service.session.as_mut().ok_or(SyncError::Locked)?;
        f(&mut service.store, &session.vault, &mut session.clock)
    }
}

pub(crate) struct Session {
    pub(crate) vault: Vault,
    pub(crate) clock: Clock,
}

pub struct Service {
    pub(crate) dir: PathBuf,
    pub(crate) store: Store,
    pub(crate) config: Config,
    mcp: McpConfig,
    kdf: KdfParams,
    pub(crate) session: Option<Session>,
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

    /// Liest `mcp.json`. Das Token steht dort nicht mehr (es liegt verschlüsselt in der Datenbank und wird beim
    /// Entsperren geladen). Ein Token aus einer älteren Datei bleibt bis zum Entsperren im Speicher und wird dann
    /// übernommen.
    fn load_mcp_config(&mut self) -> Result<()> {
        self.mcp = match fs::read(self.dir.join(MCP_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| ServiceError::Settings(format!("mcp.json ist ungültig: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => McpConfig::default(),
            Err(e) => return Err(ServiceError::Settings(e.to_string())),
        };
        Ok(())
    }

    fn save_mcp_config(&self) -> Result<()> {
        // `McpConfig` schreibt das Token nicht mit (siehe dort).
        let json = serde_json::to_vec_pretty(&self.mcp).map_err(|e| ServiceError::Settings(e.to_string()))?;
        write_private(&self.dir.join(MCP_FILE), &json).map_err(|e| ServiceError::Settings(e.to_string()))
    }

    fn write_settings(&self) -> Result<()> {
        let file = SettingsFile { server_url: &self.config.server_url, attachments_dir: &self.config.attachments_dir };
        let json = serde_json::to_vec_pretty(&file).map_err(|e| ServiceError::Settings(e.to_string()))?;
        write_private(&self.dir.join(SETTINGS_FILE), &json).map_err(|e| ServiceError::Settings(e.to_string()))
    }

    /// Nach dem Entsperren: Geheimnisse aus der verschlüsselten Ablage laden. Liegen noch Werte im Speicher (eben
    /// eingetragen, oder aus Dateien älterer Fassungen gelesen), werden sie verschlüsselt abgelegt und die Dateien
    /// ohne sie neu geschrieben.
    fn load_secrets(&mut self) -> Result<()> {
        let vault = &self.session.as_ref().ok_or(ServiceError::Locked)?.vault;
        let store = &self.store;

        let settings_had_token = fs::read_to_string(self.dir.join(SETTINGS_FILE)).is_ok_and(|t| t.contains("\"token\""));
        if self.config.token.is_empty() {
            self.config.token = store.secret_get(vault, SYNC_TOKEN)?.unwrap_or_default();
        } else {
            store.secret_set(vault, SYNC_TOKEN, &self.config.token)?;
        }

        let mcp_had_token = !self.mcp.token.is_empty();
        if mcp_had_token {
            store.secret_set(vault, MCP_TOKEN, &self.mcp.token)?;
        } else {
            self.mcp.token = match store.secret_get(vault, MCP_TOKEN)? {
                Some(token) => token,
                None => {
                    let token = McpConfig::new_token().map_err(|e| ServiceError::Settings(e.to_string()))?;
                    store.secret_set(vault, MCP_TOKEN, &token)?;
                    token
                }
            };
        }

        let legacy = secrets::read_legacy(&self.dir);
        if let Some(password) = legacy.get(SMTP_PASSWORD) {
            if !store.secret_exists(SMTP_PASSWORD)? {
                store.secret_set(vault, SMTP_PASSWORD, password)?;
            }
        }
        secrets::remove_legacy(&self.dir);

        if settings_had_token {
            self.write_settings()?;
        }
        if mcp_had_token || fs::read_to_string(self.dir.join(MCP_FILE)).is_ok_and(|t| t.contains("\"token\"")) {
            self.save_mcp_config()?;
        }
        Ok(())
    }

    pub fn mcp_config(&self) -> &McpConfig {
        &self.mcp
    }

    /// Ändert Einstellungen des KI-Zugriffs. Das Token bleibt unverändert (siehe [`Service::new_mcp_token`]). Nur
    /// bei entsperrter App, damit niemand am gesperrten Rechner den Zugriff auf Personendaten freigeben kann.
    pub fn set_mcp_config(&mut self, enabled: bool, port: u16, write: bool, allow_personal: &[String]) -> Result<()> {
        if self.session.is_none() {
            return Err(ServiceError::Locked);
        }
        if port != 0 && port < 1024 {
            return Err(ServiceError::Settings("Der Port muss mindestens 1024 sein".into()));
        }
        let mut allowed: Vec<String> = Vec::new();
        for kind in allow_personal {
            match schema::kind(kind) {
                Some(def) if def.personal && !def.hidden => {
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
        let (store, vault, _) = self.unlocked()?;
        let token = McpConfig::new_token().map_err(|e| ServiceError::Settings(e.to_string()))?;
        store.secret_set(vault, MCP_TOKEN, &token)?;
        self.mcp.token = token;
        Ok(())
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
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        if let Ok(mut file) = options.open(&path) {
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

    /// Server und Token sind eingetragen. Bei gesperrter App ist das Token nicht im Speicher, dann zählt, ob es
    /// verschlüsselt abgelegt ist.
    fn is_configured(&self) -> bool {
        !self.config.server_url.trim().is_empty()
            && (!self.config.token.trim().is_empty() || self.store.secret_exists(SYNC_TOKEN).unwrap_or(false))
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
        // Ohne Tresor (Einrichtung) bleibt das Token bis zum Anlegen oder Beitreten im Speicher.
        if let Some(session) = &self.session {
            self.store.secret_set(&session.vault, SYNC_TOKEN, &config.token)?;
        }
        self.config = config;
        self.write_settings()
    }

    fn client(&self) -> Result<SyncClient> {
        if self.config.server_url.trim().is_empty() || self.config.token.trim().is_empty() {
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
        self.start_session(vault)?;
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
        self.start_session(vault)
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
        self.load_secrets()
    }

    /// Sperrt die App. Datenschlüssel und Token verlassen den Speicher.
    pub fn lock(&mut self) {
        self.session = None;
        self.config.token.clear();
        self.mcp.token.clear();
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

    /// Gleicht mit dem Server ab. Vorher wird der Keyring abgeglichen (siehe [`Service::keyring_plan`]).
    pub fn sync(&mut self) -> Result<SyncReport> {
        let client = self.sync_client()?;
        let mut warnings = match self.keyring_plan(client.get_keyring()?)? {
            KeyringStep::Done(warnings) => warnings,
            KeyringStep::Upload { keyring, base_version, initial } => {
                let result = client.put_keyring(base_version, &keyring);
                self.keyring_uploaded(&keyring, initial, result)?;
                Vec::new()
            }
        };
        let session = self.session.as_mut().ok_or(ServiceError::Locked)?;
        let mut report = sync::sync(&mut self.store, &session.vault, &mut session.clock, &client)?;
        warnings.append(&mut report.warnings);
        report.warnings = warnings;
        Ok(report)
    }

    /// Wie [`Service::sync`] für einen geteilten Dienst (App, MCP-Server): Die Sperre gilt nur für die kurzen
    /// Zugriffe auf die Datenbank, nicht während der Anfragen an den Server. Oberfläche und KI-Zugriff müssen so
    /// nicht auf einen langsamen oder unerreichbaren Server warten. Wird die App währenddessen gesperrt, bricht der
    /// Abgleich ab.
    pub fn sync_shared(service: &Mutex<Service>) -> Result<SyncReport> {
        let lock = || service.lock().map_err(|_| ServiceError::Settings("Interner Fehler: Sperre vergiftet".into()));
        let client = lock()?.sync_client()?;
        let server_keyring = client.get_keyring()?;
        let step = lock()?.keyring_plan(server_keyring)?;
        let mut warnings = match step {
            KeyringStep::Done(warnings) => warnings,
            KeyringStep::Upload { keyring, base_version, initial } => {
                let result = client.put_keyring(base_version, &keyring);
                lock()?.keyring_uploaded(&keyring, initial, result)?;
                Vec::new()
            }
        };
        let mut report = sync::sync_with(&mut Shared(service), &client).map_err(|e| match e {
            SyncError::Locked => ServiceError::Locked,
            other => other.into(),
        })?;
        warnings.append(&mut report.warnings);
        report.warnings = warnings;
        Ok(report)
    }

    /// Prüft, ob abgeglichen werden kann, und liefert die Verbindung zum Server.
    fn sync_client(&self) -> Result<SyncClient> {
        if !self.is_configured() {
            return Err(ServiceError::NotConfigured);
        }
        if self.session.is_none() {
            return Err(ServiceError::Locked);
        }
        self.client()
    }

    /// Abgleich des Keyrings vor dem Abgleich der Einträge, ausgehend vom Keyring des Servers:
    ///
    /// - Existiert er noch nicht auf dem Server (Tresor offline angelegt), wird er hochgeladen.
    /// - Hat der Server einen neueren Keyring (zum Beispiel nach einem Passwortwechsel auf einem anderen Rechner),
    ///   übernimmt dieser Rechner ihn, wenn seine Prüfsumme zum Datenschlüssel passt. Danach gilt auch hier nur
    ///   noch das neue Passwort. Sonst gibt es einen Hinweis.
    /// - Ein Keyring ohne Prüfsumme (angelegt vor ihrer Einführung) bekommt sie und wird neu hochgeladen.
    /// - Hat der Server seinen Bestand verloren oder hat ein anderer Rechner ihn neu befüllt, bricht der Abgleich mit
    ///   [`SyncError::ServerReset`] ab (siehe [`reset_reason`] und [`Service::refill_server`]).
    ///
    /// Hochladen erledigt der Aufrufer (ohne Sperre) und meldet das Ergebnis an [`Service::keyring_uploaded`].
    fn keyring_plan(&mut self, server: Option<(crypto::Keyring, i64)>) -> Result<KeyringStep> {
        let vault = &self.session.as_ref().ok_or(ServiceError::Locked)?.vault;
        let (local, version) = self.store.keyring()?.ok_or(ServiceError::NoVault)?;
        let mut warnings = Vec::new();
        if let Some(reason) = reset_reason(vault, &local, version, server.as_ref()) {
            return Err(SyncError::ServerReset(reason).into());
        }
        match server {
            None if version == 0 => {
                return Ok(KeyringStep::Upload { keyring: vault.seal_keyring(&local), base_version: 0, initial: true });
            }
            // Ausgeschlossen durch `reset_reason` (der Server hat seinen Bestand verloren).
            None => {}
            Some((server, server_version)) if version == 0 => {
                if server == local || vault.verify_keyring(&server) {
                    self.store.set_keyring(&server, server_version)?;
                } else {
                    return Err(ServiceError::OtherVaultOnServer);
                }
            }
            Some((server, server_version)) if server_version > version => {
                if vault.verify_keyring(&server) {
                    self.store.set_keyring(&server, server_version)?;
                } else {
                    warnings.push(
                        "Auf dem Server liegt ein neuerer Keyring, dessen Prüfsumme nicht zu diesem Tresor passt. Er wurde \
                         nicht übernommen. Wurde das Master-Passwort auf einem Rechner mit älterer App geändert, dort \
                         bitte die App aktualisieren und das Passwort erneut setzen."
                            .to_string(),
                    );
                }
            }
            Some((server, server_version)) if server_version == version && server == local && !local.is_sealed() => {
                return Ok(KeyringStep::Upload { keyring: vault.seal_keyring(&local), base_version: version, initial: false });
            }
            Some(_) => {}
        }
        Ok(KeyringStep::Done(warnings))
    }

    fn keyring_uploaded(&mut self, keyring: &crypto::Keyring, initial: bool, result: sync::Result<i64>) -> Result<()> {
        match result {
            Ok(version) => Ok(self.store.set_keyring(keyring, version)?),
            // Beim Nachrüsten der Prüfsumme war ein anderer Rechner schneller. Der nächste Abgleich übernimmt dessen
            // Stand.
            Err(SyncError::KeyringChanged) if !initial => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // ----- Server neu befüllen ----------------------------------------------------------------------------

    /// Nimmt den Server nach einem Verlust wieder in Betrieb (leer oder aus einer älteren Sicherung) oder schließt
    /// diesen Rechner an einen Server an, den ein anderer Rechner neu befüllt hat. Danach läuft ein normaler Abgleich.
    ///
    /// Lokal gehen dabei keine Daten verloren: Alle Einträge werden zum Senden vorgemerkt und, wo der Server schon
    /// einen Stand hat, je Feld mit ihm zusammengeführt. Der jüngere Zeitstempel gewinnt, bei einer älteren Sicherung
    /// also der Stand der Rechner. Siehe `docs/ARCHITEKTUR.md`, „Server neu befüllen“.
    pub fn refill_server(&mut self) -> Result<SyncReport> {
        let client = self.sync_client()?;
        let (keyring, version) = match self.refill_plan(client.get_keyring()?)? {
            RefillStep::Adopt { keyring, version } => (keyring, version),
            RefillStep::Upload { keyring, base_version } => {
                let version = client.put_keyring(base_version, &keyring).map_err(refill_upload_error)?;
                (keyring, version)
            }
        };
        self.store.reset_server_state(&keyring, version)?;
        self.sync()
    }

    /// Wie [`Service::refill_server`] für einen geteilten Dienst (ohne Sperre während der Anfragen an den Server).
    pub fn refill_server_shared(service: &Mutex<Service>) -> Result<SyncReport> {
        let lock = || service.lock().map_err(|_| ServiceError::Settings("Interner Fehler: Sperre vergiftet".into()));
        let client = lock()?.sync_client()?;
        let server_keyring = client.get_keyring()?;
        let step = lock()?.refill_plan(server_keyring)?;
        let (keyring, version) = match step {
            RefillStep::Adopt { keyring, version } => (keyring, version),
            RefillStep::Upload { keyring, base_version } => {
                let version = client.put_keyring(base_version, &keyring).map_err(refill_upload_error)?;
                (keyring, version)
            }
        };
        lock()?.store.reset_server_state(&keyring, version)?;
        Self::sync_shared(service)
    }

    /// Entscheidet, mit welchem Keyring es nach dem Zurücksetzen weitergeht:
    ///
    /// - Trägt der Keyring des Servers eine andere Bestandskennung und passt seine Prüfsumme, hat ein anderer
    ///   Rechner den Server neu befüllt. Dieser Rechner übernimmt den Keyring (samt Version) und lädt nichts hoch.
    /// - Sonst befüllt dieser Rechner den Server: Sein Keyring bekommt eine neue Bestandskennung und ersetzt den des
    ///   Servers (oder wird der erste, wenn der Server leer ist). Die neue Kennung zeigt allen anderen Rechnern, dass
    ///   sie ebenfalls zurücksetzen müssen. Ein Keyring auf dem Server wird nur ersetzt, wenn er zu diesem Tresor
    ///   gehört (Prüfsumme passt oder er gleicht dem lokalen).
    fn refill_plan(&self, server: Option<(crypto::Keyring, i64)>) -> Result<RefillStep> {
        let vault = &self.session.as_ref().ok_or(ServiceError::Locked)?.vault;
        let (local, _) = self.store.keyring()?.ok_or(ServiceError::NoVault)?;
        let base_version = match server {
            None => 0,
            Some((server, version)) if server.epoch() != local.epoch() && vault.verify_keyring(&server) => {
                return Ok(RefillStep::Adopt { keyring: server, version });
            }
            Some((server, version)) if server == local || vault.verify_keyring(&server) => version,
            Some(_) => return Err(ServiceError::OtherVaultOnServer),
        };
        Ok(RefillStep::Upload { keyring: vault.seal_keyring(&local.with_new_epoch()?), base_version })
    }

    // ----- Einträge --------------------------------------------------------------------------------------

    pub(crate) fn unlocked(&mut self) -> Result<(&Store, &Vault, &mut Clock)> {
        let session = self.session.as_mut().ok_or(ServiceError::Locked)?;
        Ok((&self.store, &session.vault, &mut session.clock))
    }

    pub fn kinds(&self) -> Vec<KindDef> {
        schema::kinds()
    }

    /// Alle nicht gelöschten Einträge eines Typs, nach dem Sortierfeld des Typs sortiert.
    pub fn list(&mut self, kind: &str) -> Result<Vec<RecordView>> {
        self.list_many(&[kind])
    }

    /// Wie [`Service::list`] für mehrere Typen, in der Reihenfolge der Typen. Die Datenbank wird dafür nur einmal
    /// entschlüsselt (der Typ steht im Chiffretext, eine Abfrage nach Typ gibt es deshalb nicht).
    pub fn list_many(&mut self, kinds: &[&str]) -> Result<Vec<RecordView>> {
        let defs = kinds
            .iter()
            .map(|kind| schema::kind(kind).ok_or_else(|| ServiceError::UnknownKind(kind.to_string())))
            .collect::<Result<Vec<_>>>()?;
        let (store, vault, _) = self.unlocked()?;
        let mut by_kind: std::collections::HashMap<String, Vec<RecordView>> = std::collections::HashMap::new();
        for (id, rec) in store.list(vault)? {
            if !rec.is_deleted() && kinds.contains(&rec.kind()) {
                by_kind.entry(rec.kind().to_string()).or_default().push(view(id, &rec));
            }
        }
        let mut out = Vec::new();
        for def in defs {
            let mut views = by_kind.remove(def.kind).unwrap_or_default();
            sort_views(&def, &mut views);
            out.extend(views);
        }
        Ok(out)
    }

    /// Alle nicht gelöschten Einträge aller Typen (zum Beispiel für den Graphen und die Suche).
    pub fn list_all(&mut self) -> Result<Vec<RecordView>> {
        let (store, vault, _) = self.unlocked()?;
        Ok(store
            .list(vault)?
            .into_iter()
            .filter(|(_, rec)| {
                let hidden = schema::kind(rec.kind()).is_some_and(|d| d.hidden);
                rec.kind() != CONFLICT_KIND && !hidden && !rec.is_deleted()
            })
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
    ///
    /// Interne Felder (zum Beispiel Dateipfade, siehe [`schema::FieldDef::internal`]) lassen sich so nicht
    /// schreiben. Diesen Weg nehmen Oberfläche und KI. Die Anwendung setzt sie über [`Service::save_trusted`].
    pub fn save(&mut self, kind: &str, id: Option<&str>, values: &Map<String, Value>) -> Result<String> {
        self.save_inner(kind, id, values, false, false)
    }

    /// Wie [`Service::save`], legt den Eintrag mit dieser ID an, wenn es ihn noch nicht gibt (für Typen mit
    /// fester ID wie die Stammdaten).
    pub fn save_with_id(&mut self, kind: &str, id: &str, values: &Map<String, Value>) -> Result<String> {
        self.save_inner(kind, Some(id), values, true, false)
    }

    /// Speichern durch die Anwendung selbst: erlaubt auch interne Felder. Nie mit Eingaben von Oberfläche oder KI
    /// aufrufen, sonst ließen sich zum Beispiel Dateipfade auf fremde Dateien im Ordner umbiegen.
    pub(crate) fn save_trusted(&mut self, kind: &str, id: Option<&str>, values: &Map<String, Value>) -> Result<String> {
        self.save_inner(kind, id, values, false, true)
    }

    fn save_inner(
        &mut self,
        kind: &str,
        id: Option<&str>,
        values: &Map<String, Value>,
        upsert: bool,
        allow_internal: bool,
    ) -> Result<String> {
        let def = schema::kind(kind).ok_or_else(|| ServiceError::UnknownKind(kind.to_string()))?;
        let (store, vault, clock) = self.unlocked()?;

        let existing = match id {
            Some(id) => match store.get_record(vault, id)? {
                Some(rec) if rec.kind() == kind && !rec.is_deleted() => Some(rec),
                None if upsert => None,
                _ => return Err(ServiceError::NotFound),
            },
            None => None,
        };

        // Prüfen und normalisieren, bevor etwas geschrieben wird.
        let mut normalized: Vec<(&'static str, Value)> = Vec::new();
        for name in values.keys() {
            match def.fields.iter().find(|f| f.name == name) {
                None => return Err(ServiceError::Invalid(format!("Unbekanntes Feld „{name}“"))),
                Some(field) if field.internal && !allow_internal => {
                    return Err(ServiceError::Invalid(format!("Das Feld „{name}“ verwaltet die Anwendung selbst")))
                }
                Some(_) => {}
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
            // Vorgabewerte gelten nur, wenn das Feld beim Anlegen gar nicht angegeben wurde. Eine ausdrücklich
            // geleerte Pflichtangabe bleibt ein Fehler (oben geprüft).
            for field in &def.fields {
                if let Some(default) = field.default {
                    if !values.contains_key(field.name) {
                        let value = match field.field_type {
                            FieldType::Bool => Value::Bool(default == "true"),
                            _ => Value::String(default.to_string()),
                        };
                        normalized.push((field.name, value));
                    }
                }
            }
            for field in def.fields.iter().filter(|f| f.required) {
                if !normalized.iter().any(|(n, v)| *n == field.name && !v.is_null()) {
                    return Err(ServiceError::Invalid(format!("{} ist ein Pflichtfeld", field.label)));
                }
            }
        }

        let is_new = existing.is_none();
        let (id, mut rec) = match (id, existing) {
            (Some(id), Some(rec)) => (id.to_string(), rec),
            (Some(id), None) if upsert => (id.to_string(), Record::new(kind)?),
            _ => (record::new_id()?, Record::new(kind)?),
        };
        if kind == "reise" {
            // Prüft die Zeiten des Ergebnisses (bestehende Werte plus Änderungen), bevor etwas geschrieben wird.
            let merged = |name: &str| -> String {
                match normalized.iter().find(|(n, _)| *n == name) {
                    Some((_, v)) => v.as_str().unwrap_or("").to_string(),
                    None => rec.get(name).and_then(Value::as_str).unwrap_or("").to_string(),
                }
            };
            check_travel_times(&[
                ("Reiseantritt", merged("reise_beginn")),
                ("Beginn Dienstgeschäft", merged("dienstgeschaeft_beginn")),
                ("Ende Dienstgeschäft", merged("dienstgeschaeft_ende")),
                ("Beendigung der Reise", merged("reise_ende")),
            ])?;
        }
        if kind == "fortbildung" {
            // Phasenwechsel vorwärts nur mit vollständigen Angaben (siehe `fortbildung::blockers`).
            let mut merged: Map<String, Value> = rec.values().map(|(k, v)| (k.to_string(), v.clone())).collect();
            for (name, value) in &normalized {
                if value.is_null() {
                    merged.remove(*name);
                } else {
                    merged.insert((*name).to_string(), value.clone());
                }
            }
            let from = rec.get_str("phase").unwrap_or("Planung").to_string();
            let to = merged.get("phase").and_then(Value::as_str).unwrap_or("Planung").to_string();
            let reasons = crate::fortbildung::blockers(&merged, &from, &to);
            if !reasons.is_empty() {
                return Err(ServiceError::Invalid(format!("Die Fortbildung kann nicht in die Phase „{to}“ wechseln. {}.", reasons.join(". "))));
            }
        }
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
/// Passt der Server noch zum Stand dieses Rechners? Geprüft wird hier nur der Keyring. `latest` und die
/// Folgenummern der Einträge prüft der Sync-Client (siehe `sync::check_server`).
///
/// Ein Rechner mit Version 0 kennt den Server noch nicht (Tresor offline angelegt) oder hat gerade zurückgesetzt.
/// Für ihn gibt es nichts zu prüfen.
fn reset_reason(
    vault: &Vault,
    local: &crypto::Keyring,
    version: i64,
    server: Option<&(crypto::Keyring, i64)>,
) -> Option<ResetReason> {
    if version == 0 {
        return None;
    }
    match server {
        None => Some(ResetReason::Empty),
        // Nur mit passender Prüfsumme: Einen fremden Keyring meldet `keyring_plan` als Hinweis.
        Some((keyring, _)) if keyring.epoch() != local.epoch() && vault.verify_keyring(keyring) => {
            Some(ResetReason::Refilled)
        }
        Some((_, server_version)) if *server_version < version => Some(ResetReason::Behind),
        Some(_) => None,
    }
}

fn refill_upload_error(e: SyncError) -> ServiceError {
    match e {
        SyncError::KeyringChanged => ServiceError::Invalid(
            "Ein anderer Rechner hat den Server gerade neu befüllt. Bitte „Server neu befüllen“ noch einmal wählen."
                .into(),
        ),
        other => other.into(),
    }
}

fn sort_views(def: &KindDef, views: &mut [RecordView]) {
    let text = |v: &RecordView, field: &str| v.fields.get(field).and_then(Value::as_str).unwrap_or("").to_lowercase();
    views.sort_by(|a, b| {
        let (ka, kb) = (text(a, def.sort_field), text(b, def.sort_field));
        // Einträge ohne Sortierwert (zum Beispiel ohne Frist) stehen immer am Ende.
        let primary = match (ka.is_empty(), kb.is_empty()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => {
                let ord = ka.cmp(&kb);
                if def.sort_desc { ord.reverse() } else { ord }
            }
        };
        primary.then_with(|| text(a, def.title_field).cmp(&text(b, def.title_field)))
    });
}

/// Reiseantritt, Beginn und Ende des Dienstgeschäfts und Ende der Reise müssen in dieser Reihenfolge liegen.
fn check_travel_times(times: &[(&str, String)]) -> Result<()> {
    let present: Vec<&(&str, String)> = times.iter().filter(|(_, t)| !t.is_empty()).collect();
    for pair in present.windows(2) {
        if pair[0].1 > pair[1].1 {
            return Err(ServiceError::Invalid(format!("{} darf nicht nach {} liegen", pair[0].0, pair[1].0)));
        }
    }
    Ok(())
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
        assert!(!fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap().contains("geheim"), "Token nie im Klartext");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(SETTINGS_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "settings.json darf nur dem Nutzer gehören");
        }
    }

    #[test]
    fn tokens_and_passwords_are_stored_encrypted_and_only_available_when_unlocked() {
        let dir = tempdir();
        let mut s = Service::open(&dir, FAST).unwrap();
        s.create_vault(PASSWORD).unwrap();
        s.set_config(Config { server_url: "https://butler.example".into(), token: "sync-geheim".into(), ..Config::default() })
            .unwrap();
        s.set_smtp_password("smtp-geheim").unwrap();
        let mcp_token = s.mcp_config().token.clone();
        assert_eq!(mcp_token.len(), 64);
        s.set_mcp_config(true, 0, false, &[]).unwrap();

        let plain_in = |name: &str| fs::read(dir.join(name)).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
        for file in [SETTINGS_FILE, MCP_FILE, "secrets.json", DB_FILE] {
            let text = plain_in(file);
            assert!(!text.contains("sync-geheim") && !text.contains("smtp-geheim") && !text.contains(&mcp_token), "{file}");
        }

        // Gesperrt: nichts davon im Speicher, eingerichtet bleibt es trotzdem.
        s.lock();
        assert!(s.config().token.is_empty() && s.mcp_config().token.is_empty());
        assert!(s.status().unwrap().configured && s.smtp_password_is_set());
        assert!(matches!(s.set_mcp_config(true, 0, true, &[]), Err(ServiceError::Locked)));
        assert!(matches!(s.new_mcp_token(), Err(ServiceError::Locked)));
        drop(s);

        let mut s = Service::open(&dir, FAST).unwrap();
        s.unlock(PASSWORD).unwrap();
        assert_eq!(s.config().token, "sync-geheim");
        assert_eq!(s.mcp_config().token, mcp_token);
    }

    #[test]
    fn plaintext_secrets_of_older_versions_are_migrated() {
        let dir = tempdir();
        let mut s = Service::open(&dir, FAST).unwrap();
        s.create_vault(PASSWORD).unwrap();
        drop(s);
        // So legten ältere Fassungen die Geheimnisse ab.
        fs::write(dir.join(SETTINGS_FILE), r#"{"server_url":"https://butler.example","token":"alt-sync","attachments_dir":""}"#).unwrap();
        fs::write(dir.join(MCP_FILE), r#"{"enabled":false,"port":8765,"token":"alt-mcp","write":false,"allow_personal":[]}"#).unwrap();
        fs::write(dir.join("secrets.json"), r#"{"smtp_passwort":"alt-smtp"}"#).unwrap();

        let mut s = Service::open(&dir, FAST).unwrap();
        s.unlock(PASSWORD).unwrap();
        assert_eq!(s.config().token, "alt-sync");
        assert_eq!(s.mcp_config().token, "alt-mcp");
        assert!(s.smtp_password_is_set());
        assert!(!dir.join("secrets.json").exists());
        for file in [SETTINGS_FILE, MCP_FILE] {
            let text = fs::read_to_string(dir.join(file)).unwrap();
            assert!(!text.contains("alt-") && !text.contains("token"), "{file}: {text}");
        }
        assert_eq!(s.config().server_url, "https://butler.example");
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
    fn orders_get_default_status_and_sort_by_deadline_with_open_ones_last() {
        let (mut s, _) = unlocked_service();
        let spaet = s.save("auftrag", None, &map(json!({"titel": "Später", "frist": "2026-12-01"}))).unwrap();
        s.save("auftrag", None, &map(json!({"titel": "Ohne Frist"}))).unwrap();
        s.save("auftrag", None, &map(json!({"titel": "Zuerst", "frist": "2026-11-05", "status": "In Arbeit", "auftraggeber": "Ministerium"}))).unwrap();

        // Der Status fehlt bei zwei Aufträgen: Vorgabe „Offen“.
        assert_eq!(s.get(&spaet).unwrap().fields["status"], "Offen");
        let titel: Vec<_> = s.list("auftrag").unwrap().iter().map(|v| v.fields["titel"].as_str().unwrap().to_string()).collect();
        assert_eq!(titel, ["Zuerst", "Später", "Ohne Frist"], "ohne Frist steht am Ende");

        // Ein ausdrücklich geleerter Status ist ein Fehler, ein ungültiger auch.
        assert!(matches!(s.save("auftrag", None, &map(json!({"titel": "X", "status": ""}))), Err(ServiceError::Invalid(_))));
        assert!(matches!(s.save("auftrag", None, &map(json!({"titel": "X", "status": "Vielleicht"}))), Err(ServiceError::Invalid(_))));
        // Beim Ändern gilt keine Vorgabe: der Status bleibt, wie er ist.
        s.save("auftrag", Some(&spaet), &map(json!({"titel": "Später, geändert"}))).unwrap();
        assert_eq!(s.get(&spaet).unwrap().fields["status"], "Offen");
        s.save("auftrag", Some(&spaet), &map(json!({"status": "Erledigt", "erledigt_am": "2026-10-31"}))).unwrap();
        assert_eq!(s.get(&spaet).unwrap().fields["status"], "Erledigt");
    }

    #[test]
    fn training_moves_through_its_phases_only_with_complete_data() {
        let (mut s, _) = unlocked_service();
        let id = s.save("fortbildung", None, &map(json!({"titel": "KI im Unterricht"}))).unwrap();
        assert_eq!(s.get(&id).unwrap().fields["phase"], "Planung", "Vorgabe: Planung");

        // Ohne Planungsangaben keine Ausschreibung. Die Meldung nennt, was fehlt.
        let err = s.save("fortbildung", Some(&id), &map(json!({"phase": "Ausschreibung"}))).unwrap_err().to_string();
        assert!(err.contains("Ausschreibung") && err.contains("Referent") && err.contains("Ort"), "{err}");
        assert_eq!(s.get(&id).unwrap().fields["phase"], "Planung", "nichts wurde gespeichert");

        // Angaben und Phase in einem Schritt: erlaubt, weil das Ergebnis vollständig ist.
        s.save("fortbildung", Some(&id), &map(json!({
            "referent": "Dr. Beispiel", "inhalt": "Grundlagen", "beginn": "2027-02-10T09:00", "ort": "Ulm", "teilnehmer_geplant": 25, "phase": "Ausschreibung"
        }))).unwrap();
        assert_eq!(s.get(&id).unwrap().fields["phase"], "Ausschreibung");

        // Durchführung braucht die FIBS-Nummer.
        assert!(s.save("fortbildung", Some(&id), &map(json!({"phase": "Durchführung"}))).unwrap_err().to_string().contains("FIBS"));
        s.save("fortbildung", Some(&id), &map(json!({"fibs_nummer": "2027-1", "phase": "Durchführung"}))).unwrap();
        s.save("fortbildung", Some(&id), &map(json!({"phase": "Nachbereitung", "checkliste_nachbereitung": "- [ ] Bescheinigungen\n- [ ] Abrechnung"}))).unwrap();

        // Abschluss erst mit abgehakter Checkliste.
        let err = s.save("fortbildung", Some(&id), &map(json!({"phase": "Abgeschlossen"}))).unwrap_err().to_string();
        assert!(err.contains("2 offene Aufgaben"), "{err}");
        s.save("fortbildung", Some(&id), &map(json!({"checkliste_nachbereitung": "- [x] Bescheinigungen\n- [x] Abrechnung", "phase": "Abgeschlossen"}))).unwrap();

        // Zurück geht immer, auch wenn Angaben fehlen. Andere Felder lassen sich in jeder Phase ändern.
        s.save("fortbildung", Some(&id), &map(json!({"referent": "", "phase": "Planung"}))).unwrap();
        s.save("fortbildung", Some(&id), &map(json!({"notizen": "später ergänzt"}))).unwrap();
        // Überspringen prüft jede Phase auf dem Weg.
        let err = s.save("fortbildung", Some(&id), &map(json!({"phase": "Durchführung"}))).unwrap_err().to_string();
        assert!(err.contains("Referent"), "{err}");
        // Eine neue Fortbildung direkt in einer späteren Phase anzulegen, geht nur mit Angaben.
        assert!(s.save("fortbildung", None, &map(json!({"titel": "Direkt", "phase": "Durchführung"}))).is_err());
        assert!(matches!(s.save("fortbildung", None, &map(json!({"titel": "X", "phase": "Unsinn"}))), Err(ServiceError::Invalid(_))));
    }

    #[test]
    fn requests_can_be_collected_and_assigned() {
        let (mut s, _) = unlocked_service();
        let schule = s.save("schule", None, &map(json!({"name": "Schule"}))).unwrap();
        let kontakt = s.save("ansprechpartner", None, &map(json!({"name": "Frau Weber"}))).unwrap();
        let b1 = s.save("bedarf", None, &map(json!({"thema": "KI im Unterricht", "schule": schule, "kontakt": kontakt, "interessenten": 8, "eingegangen_am": "2026-10-01"}))).unwrap();
        let b2 = s.save("bedarf", None, &map(json!({"thema": "Tablets", "eingegangen_am": "2026-10-20"}))).unwrap();
        assert_eq!(s.get(&b1).unwrap().fields["status"], "Offen");
        // Neueste Anmeldung zuerst.
        let themen: Vec<_> = s.list("bedarf").unwrap().iter().map(|v| v.fields["thema"].as_str().unwrap().to_string()).collect();
        assert_eq!(themen, ["Tablets", "KI im Unterricht"]);

        let fb = s.save("fortbildung", None, &map(json!({"titel": "KI-Tag"}))).unwrap();
        s.save("bedarf", Some(&b1), &map(json!({"fortbildung": fb, "status": "Eingeplant"}))).unwrap();
        assert_eq!(s.get(&b1).unwrap().fields["fortbildung"], json!(fb));
        // Verweise müssen auf eine Fortbildung zeigen.
        assert!(matches!(s.save("bedarf", Some(&b2), &map(json!({"fortbildung": schule}))), Err(ServiceError::Invalid(_))));
        assert!(matches!(s.save("bedarf", None, &map(json!({"thema": "X", "status": "Vielleicht"}))), Err(ServiceError::Invalid(_))));
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
