//! JSON-Schnittstelle des Dienstes. Tauri-Befehle, MCP-Server und Browser-Tests rufen alle [`call`] auf.
//!
//! Befehle und Argumente:
//!
//! | Befehl | Argumente | Ergebnis |
//! |---|---|---|
//! | `status` | | `{configured, has_vault, unlocked}` |
//! | `config_get` | | `{server_url, token, attachments_dir}` |
//! | `config_set` | `{server_url, token, attachments_dir}` | |
//! | `create_vault` | `{password}` | `{recovery_key}` |
//! | `join_vault` | `{password}` | |
//! | `unlock` | `{password}` | |
//! | `unlock_recovery` | `{recovery_key}` | |
//! | `lock` | | |
//! | `change_password` | `{old_password, new_password}` | |
//! | `sync` | | `{pulled, pushed, merged, conflicts}` |
//! | `kinds` | | Typdefinitionen |
//! | `list` | `{kind}` | Einträge |
//! | `list_all` | | Einträge |
//! | `get` | `{id}` | Eintrag |
//! | `save` | `{kind, id?, fields}` | `{id}` |
//! | `delete` | `{id}` | |
//! | `conflicts` | | Konflikte |
//! | `stammdaten_get` | | `{id, fields, smtp_password_set}` |
//! | `stammdaten_save` | `{fields}` | |
//! | `smtp_password_set` | `{password}` (leer löscht) | |
//! | `reise_antrag` | `{id}` | `{path}` |
//! | `reise_abrechnung` | `{id}` | `{path}` |
//! | `reise_formular` | `{id, name, data}` (Base64) | `{path}` |
//! | `reise_genehmigung` | `{id, name, data}` (Base64) | `{path}` |
//! | `anhang_add` | `{reise, art, name, data, beilegen?}` | `{id}` |
//! | `anhang_delete` | `{id}` | |
//! | `file_path` | `{path}` | `{path}` (absolut) |
//! | `file_read` | `{path}` | `{name, data}` (Base64) |
//! | `mail_draft` | `{id, kind}` (`antrag` oder `abrechnung`) | Entwurf |
//! | `mail_send` | `{id, kind, subject, body, attachments}` | |
//! | `mcp_get` | | Einstellungen des KI-Zugriffs und ob der Server läuft |
//! | `mcp_set` | `{enabled, port, write, allow_personal}` | wie `mcp_get` |
//! | `mcp_new_token` | | wie `mcp_get` |
//! | `mcp_audit` | `{limit?}` | letzte Einträge des Protokolls |

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::{json, Map, Value};

use crate::crypto;
use crate::mcp::McpServer;
use crate::reisen::MailKind;
use crate::service::{Config, Service, ServiceError};
use crate::sync::SyncError;

/// Fehler für die Oberfläche: ein stabiler Code und ein lesbarer Text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl From<ServiceError> for ApiError {
    fn from(e: ServiceError) -> Self {
        let code = match &e {
            ServiceError::Locked => "locked",
            ServiceError::NoVault => "no_vault",
            ServiceError::VaultExists => "vault_exists",
            ServiceError::WrongSecret => "wrong_secret",
            ServiceError::WeakPassword => "weak_password",
            ServiceError::NotConfigured => "not_configured",
            ServiceError::UnknownKind(_) => "unknown_kind",
            ServiceError::NotFound => "not_found",
            ServiceError::Invalid(_) => "invalid",
            ServiceError::OtherVaultOnServer => "other_vault",
            ServiceError::Settings(_) => "settings",
            ServiceError::Sync(SyncError::Unauthorized) => "unauthorized",
            ServiceError::Sync(SyncError::Network(_)) => "network",
            ServiceError::Sync(SyncError::Crypto(crypto::Error::WrongSecret)) => "wrong_secret",
            ServiceError::Sync(_) => "sync",
            ServiceError::Store(_) => "store",
            ServiceError::Crypto(_) => "crypto",
        };
        let message = match &e {
            // Das Passwort kann auch beim Beitreten falsch sein: gleiche Meldung.
            ServiceError::Sync(SyncError::Crypto(crypto::Error::WrongSecret)) => {
                ServiceError::WrongSecret.to_string()
            }
            other => other.to_string(),
        };
        Self { code, message }
    }
}

type ApiResult = Result<Value, ApiError>;

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, ApiError> {
    serde_json::from_value(args.clone()).map_err(|e| ApiError::new("bad_request", format!("Ungültige Argumente: {e}")))
}

fn to_value<T: Serialize>(v: T) -> ApiResult {
    serde_json::to_value(v).map_err(|e| ApiError::new("internal", e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Password {
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryArgs {
    recovery_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangePassword {
    old_password: String,
    new_password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindArgs {
    kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldsArgs {
    fields: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileArgs {
    id: String,
    name: String,
    /// Dateiinhalt als Base64.
    data: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentArgs {
    reise: String,
    art: String,
    name: String,
    data: String,
    #[serde(default)]
    beilegen: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MailKindArgs {
    id: String,
    kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MailSendArgs {
    id: String,
    kind: String,
    subject: String,
    body: String,
    #[serde(default)]
    attachments: Vec<String>,
}

fn decode_base64(data: &str) -> Result<Vec<u8>, ApiError> {
    B64.decode(data.trim()).map_err(|_| ApiError::new("bad_request", "Die Datei ist nicht richtig kodiert"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveArgs {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    fields: Map<String, Value>,
}

/// Die Anwendung: der Dienst und, wenn eingeschaltet, der laufende MCP-Server.
pub struct App {
    service: Arc<Mutex<Service>>,
    /// Laufender Server und der Port, mit dem er angefordert wurde (0 = beliebig).
    mcp: Mutex<Option<(McpServer, u16)>>,
}

impl App {
    pub fn new(service: Service) -> Self {
        Self { service: Arc::new(Mutex::new(service)), mcp: Mutex::new(None) }
    }

    pub fn service(&self) -> &Arc<Mutex<Service>> {
        &self.service
    }

    /// Bringt den MCP-Server in den Zustand der gespeicherten Einstellungen: startet, hält an oder startet
    /// bei geändertem Port neu. Token, Schreibstufe und Freigaben gelten ohne Neustart. Gibt die Adresse
    /// zurück, wenn er läuft.
    pub fn apply_mcp_config(&self) -> Result<Option<String>, ServiceError> {
        let config = self
            .service
            .lock()
            .map_err(|_| ServiceError::Settings("Interner Fehler: Sperre vergiftet".into()))?
            .mcp_config()
            .clone();
        let mut running = self
            .mcp
            .lock()
            .map_err(|_| ServiceError::Settings("Interner Fehler: Sperre vergiftet".into()))?;
        if !config.enabled {
            *running = None;
            return Ok(None);
        }
        if let Some((server, requested)) = running.as_ref() {
            if *requested == config.port {
                return Ok(Some(server.url()));
            }
        }
        // Den alten Server zuerst anhalten, damit der Port frei wird.
        *running = None;
        let mut last_error = None;
        // Der Port wird erst frei, wenn der alte Server-Thread beendet ist: kurz erneut versuchen.
        for _ in 0..40 {
            match McpServer::spawn(Arc::clone(&self.service), config.port) {
                Ok(server) => {
                    let url = server.url();
                    *running = Some((server, config.port));
                    return Ok(Some(url));
                }
                Err(e) => {
                    last_error = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        Err(ServiceError::Settings(format!(
            "Der KI-Zugriff konnte nicht gestartet werden (Port {}): {}",
            config.port,
            last_error.map(|e| e.to_string()).unwrap_or_default()
        )))
    }

    /// Adresse des laufenden MCP-Servers.
    pub fn mcp_url(&self) -> Option<String> {
        self.mcp.lock().ok().and_then(|m| m.as_ref().map(|(server, _)| server.url()))
    }

    fn mcp_state(&self) -> ApiResult {
        let config = self
            .service
            .lock()
            .map_err(|_| ApiError::new("internal", "Interner Fehler: Sperre vergiftet"))?
            .mcp_config()
            .clone();
        let url = self.mcp_url();
        Ok(json!({
            "enabled": config.enabled,
            "port": config.port,
            "token": config.token,
            "write": config.write,
            "allow_personal": config.allow_personal,
            "running": url.is_some(),
            "url": url,
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct McpSetArgs {
    enabled: bool,
    port: u16,
    write: bool,
    #[serde(default)]
    allow_personal: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditArgs {
    #[serde(default)]
    limit: Option<usize>,
}

/// Befehle für den KI-Zugriff. `None`, wenn der Befehl nicht dazugehört.
fn call_mcp(app: &App, command: &str, args: &Value) -> Option<ApiResult> {
    let lock = || {
        app.service
            .lock()
            .map_err(|_| ApiError::new("internal", "Interner Fehler: Sperre vergiftet"))
    };
    Some((|| match command {
        "mcp_get" => app.mcp_state(),
        "mcp_set" => {
            let a: McpSetArgs = parse(args)?;
            lock()?.set_mcp_config(a.enabled, a.port, a.write, &a.allow_personal)?;
            app.apply_mcp_config()?;
            app.mcp_state()
        }
        "mcp_new_token" => {
            lock()?.new_mcp_token()?;
            app.apply_mcp_config()?;
            app.mcp_state()
        }
        "mcp_audit" => {
            let a: AuditArgs = parse(args)?;
            Ok(Value::Array(lock()?.audit_tail(a.limit.unwrap_or(50).min(200))))
        }
        _ => Err(ApiError::new("__not_mcp", "")),
    })())
    .filter(|r| !matches!(r, Err(e) if e.code == "__not_mcp"))
}

/// Führt einen Befehl aus. Der Dienst ist durch ein Mutex geschützt, es läuft immer nur ein Befehl.
pub fn call(app: &App, command: &str, args: &Value) -> ApiResult {
    if let Some(result) = call_mcp(app, command, args) {
        return result;
    }
    let mut service = app
        .service
        .lock()
        .map_err(|_| ApiError::new("internal", "Interner Fehler: Sperre vergiftet"))?;
    let none = || -> Result<(), ApiError> {
        if args.is_null() || args.as_object().is_some_and(Map::is_empty) {
            Ok(())
        } else {
            Err(ApiError::new("bad_request", "Dieser Befehl hat keine Argumente"))
        }
    };

    match command {
        "status" => {
            none()?;
            to_value(service.status()?)
        }
        "config_get" => {
            none()?;
            to_value(service.config())
        }
        "config_set" => {
            let config: Config = parse(args)?;
            service.set_config(config)?;
            Ok(Value::Null)
        }
        "create_vault" => {
            let a: Password = parse(args)?;
            let recovery_key = service.create_vault(&a.password)?;
            Ok(json!({ "recovery_key": recovery_key }))
        }
        "join_vault" => {
            let a: Password = parse(args)?;
            service.join_vault(&a.password)?;
            Ok(Value::Null)
        }
        "unlock" => {
            let a: Password = parse(args)?;
            service.unlock(&a.password)?;
            Ok(Value::Null)
        }
        "unlock_recovery" => {
            let a: RecoveryArgs = parse(args)?;
            service.unlock_with_recovery(&a.recovery_key)?;
            Ok(Value::Null)
        }
        "lock" => {
            none()?;
            service.lock();
            Ok(Value::Null)
        }
        "change_password" => {
            let a: ChangePassword = parse(args)?;
            service.change_password(&a.old_password, &a.new_password)?;
            Ok(Value::Null)
        }
        "sync" => {
            none()?;
            to_value(service.sync()?)
        }
        "kinds" => {
            none()?;
            to_value(service.kinds())
        }
        "list" => {
            let a: KindArgs = parse(args)?;
            to_value(service.list(&a.kind)?)
        }
        "list_all" => {
            none()?;
            to_value(service.list_all()?)
        }
        "get" => {
            let a: IdArgs = parse(args)?;
            to_value(service.get(&a.id)?)
        }
        "save" => {
            let a: SaveArgs = parse(args)?;
            let id = service.save(&a.kind, a.id.as_deref(), &a.fields)?;
            Ok(json!({ "id": id }))
        }
        "delete" => {
            let a: IdArgs = parse(args)?;
            service.delete(&a.id)?;
            Ok(Value::Null)
        }
        "conflicts" => {
            none()?;
            to_value(service.conflicts()?)
        }
        "stammdaten_get" => {
            none()?;
            let view = service.stammdaten()?;
            Ok(json!({"id": view.id, "fields": view.fields, "smtp_password_set": service.smtp_password_is_set()}))
        }
        "stammdaten_save" => {
            let a: FieldsArgs = parse(args)?;
            service.save_stammdaten(&a.fields)?;
            Ok(Value::Null)
        }
        "smtp_password_set" => {
            let a: Password = parse(args)?;
            service.set_smtp_password(&a.password)?;
            Ok(Value::Null)
        }
        "reise_antrag" => {
            let a: IdArgs = parse(args)?;
            Ok(json!({"path": service.reise_antrag_erzeugen(&a.id)?}))
        }
        "reise_abrechnung" => {
            let a: IdArgs = parse(args)?;
            Ok(json!({"path": service.reise_abrechnung_erzeugen(&a.id)?}))
        }
        "reise_formular" => {
            let a: FileArgs = parse(args)?;
            Ok(json!({"path": service.reise_formular_setzen(&a.id, &a.name, &decode_base64(&a.data)?)?}))
        }
        "reise_genehmigung" => {
            let a: FileArgs = parse(args)?;
            Ok(json!({"path": service.reise_genehmigung_setzen(&a.id, &a.name, &decode_base64(&a.data)?)?}))
        }
        "anhang_add" => {
            let a: AttachmentArgs = parse(args)?;
            let id = service.anhang_hinzufuegen(&a.reise, &a.art, &a.name, &decode_base64(&a.data)?, a.beilegen)?;
            Ok(json!({"id": id}))
        }
        "anhang_delete" => {
            let a: IdArgs = parse(args)?;
            service.anhang_loeschen(&a.id)?;
            Ok(Value::Null)
        }
        "file_path" => {
            let a: PathArgs = parse(args)?;
            Ok(json!({"path": service.file_path(&a.path)?}))
        }
        "file_read" => {
            let a: PathArgs = parse(args)?;
            let (name, bytes) = service.file_read(&a.path)?;
            Ok(json!({"name": name, "data": B64.encode(bytes)}))
        }
        "mail_draft" => {
            let a: MailKindArgs = parse(args)?;
            to_value(service.mail_entwurf(&a.id, MailKind::parse(&a.kind)?)?)
        }
        "mail_send" => {
            let a: MailSendArgs = parse(args)?;
            service.mail_senden(&a.id, MailKind::parse(&a.kind)?, &a.subject, &a.body, &a.attachments)?;
            Ok(Value::Null)
        }
        other => Err(ApiError::new("unknown_command", format!("Unbekannter Befehl „{other}“"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
    const PASSWORD: &str = "korrekt pferd batterie heftklammer";

    fn service() -> App {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-api-test-{name}"));
        App::new(Service::open(&dir, FAST).unwrap())
    }

    fn ok(s: &App, cmd: &str, args: Value) -> Value {
        call(s, cmd, &args).unwrap_or_else(|e| panic!("{cmd}: {e:?}"))
    }

    fn err(s: &App, cmd: &str, args: Value) -> ApiError {
        call(s, cmd, &args).expect_err(cmd)
    }

    #[test]
    fn full_flow_through_json() {
        let s = service();
        assert_eq!(ok(&s, "status", Value::Null), json!({"configured": false, "has_vault": false, "unlocked": false}));

        let created = ok(&s, "create_vault", json!({"password": PASSWORD}));
        assert!(created["recovery_key"].as_str().unwrap().len() > 50);
        assert_eq!(ok(&s, "status", Value::Null)["unlocked"], true);

        let saved = ok(&s, "save", json!({"kind": "schule", "fields": {"name": "Grundschule", "ort": "Ulm"}}));
        let id = saved["id"].as_str().unwrap().to_string();
        let listed = ok(&s, "list", json!({"kind": "schule"}));
        assert_eq!(listed[0]["id"], id);
        assert_eq!(listed[0]["fields"]["ort"], "Ulm");

        ok(&s, "save", json!({"kind": "schule", "id": id, "fields": {"ort": "Bonn"}}));
        assert_eq!(ok(&s, "get", json!({"id": id}))["fields"]["ort"], "Bonn");

        ok(&s, "lock", Value::Null);
        assert_eq!(err(&s, "list", json!({"kind": "schule"})).code, "locked");
        ok(&s, "unlock", json!({"password": PASSWORD}));

        ok(&s, "delete", json!({"id": id}));
        assert_eq!(err(&s, "get", json!({"id": id})).code, "not_found");
        assert_eq!(ok(&s, "list_all", Value::Null), json!([]));
        assert_eq!(ok(&s, "conflicts", Value::Null), json!([]));
    }

    #[test]
    fn schema_is_exposed() {
        let s = service();
        let kinds = ok(&s, "kinds", Value::Null);
        assert_eq!(kinds[0]["kind"], "schule");
        assert_eq!(kinds[1]["personal"], true);
    }

    #[test]
    fn error_codes_are_stable() {
        let s = service();
        assert_eq!(err(&s, "unlock", json!({"password": PASSWORD})).code, "no_vault");
        assert_eq!(err(&s, "sync", Value::Null).code, "not_configured");
        assert_eq!(err(&s, "create_vault", json!({"password": "kurz"})).code, "weak_password");
        ok(&s, "create_vault", json!({"password": PASSWORD}));
        assert_eq!(err(&s, "create_vault", json!({"password": PASSWORD})).code, "vault_exists");
        ok(&s, "lock", Value::Null);
        assert_eq!(err(&s, "unlock", json!({"password": "falsches passwort"})).code, "wrong_secret");
        ok(&s, "unlock", json!({"password": PASSWORD}));
        assert_eq!(err(&s, "save", json!({"kind": "schule", "fields": {}})).code, "invalid");
        assert_eq!(err(&s, "list", json!({"kind": "x"})).code, "unknown_kind");
        assert_eq!(err(&s, "config_set", json!({"server_url": "ftp://x", "token": "t", "attachments_dir": ""})).code, "settings");
    }

    #[test]
    fn bad_requests_are_rejected() {
        let s = service();
        assert_eq!(err(&s, "gibt-es-nicht", Value::Null).code, "unknown_command");
        assert_eq!(err(&s, "unlock", json!({})).code, "bad_request");
        assert_eq!(err(&s, "unlock", json!({"password": "x", "extra": 1})).code, "bad_request");
        assert_eq!(err(&s, "unlock", json!("text")).code, "bad_request");
        assert_eq!(err(&s, "status", json!({"x": 1})).code, "bad_request");
        assert_eq!(err(&s, "save", json!({"kind": "schule"})).code, "bad_request");
    }

    #[test]
    fn config_roundtrip() {
        let s = service();
        ok(&s, "config_set", json!({"server_url": "https://butler.example/", "token": "abc", "attachments_dir": "/x"}));
        assert_eq!(
            ok(&s, "config_get", Value::Null),
            json!({"server_url": "https://butler.example", "token": "abc", "attachments_dir": "/x"})
        );
        assert_eq!(ok(&s, "status", Value::Null)["configured"], true);
    }

    fn mcp_post(url: &str, token: &str, body: &str) -> (u16, String) {
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
        match agent.post(url).header("Authorization", format!("Bearer {token}")).send(body) {
            Ok(mut r) => (r.status().as_u16(), r.body_mut().read_to_string().unwrap_or_default()),
            Err(_) => (0, String::new()),
        }
    }

    #[test]
    fn mcp_settings_are_validated_and_start_and_stop_the_server() {
        let s = service();
        let initial = ok(&s, "mcp_get", Value::Null);
        assert_eq!((initial["enabled"].clone(), initial["running"].clone(), initial["write"].clone()), (json!(false), json!(false), json!(false)));
        let token = initial["token"].as_str().unwrap().to_string();
        assert_eq!(token.len(), 64);
        assert_eq!(initial["allow_personal"], json!([]));

        // Ungültige Einstellungen werden abgelehnt und ändern nichts.
        assert_eq!(err(&s, "mcp_set", json!({"enabled": true, "port": 80, "write": false})).code, "settings");
        assert_eq!(err(&s, "mcp_set", json!({"enabled": true, "port": 0, "write": false, "allow_personal": ["schule"]})).code, "settings");
        assert_eq!(err(&s, "mcp_set", json!({"enabled": true, "port": 0, "write": false, "allow_personal": ["gibt-es-nicht"]})).code, "settings");
        assert_eq!(err(&s, "mcp_set", json!({"enabled": true, "port": 0})).code, "bad_request");
        assert_eq!(ok(&s, "mcp_get", Value::Null)["running"], false);

        // Einschalten: Der Server läuft und antwortet mit dem Token.
        let on = ok(&s, "mcp_set", json!({"enabled": true, "port": 0, "write": true, "allow_personal": ["ansprechpartner", "ansprechpartner"]}));
        assert_eq!(on["running"], true);
        assert_eq!(on["allow_personal"], json!(["ansprechpartner"]), "Doppelte Angaben entfallen");
        assert_eq!(on["token"], json!(token), "Das Token bleibt beim Ändern der Einstellungen");
        let url = on["url"].as_str().unwrap().to_string();
        let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        assert_eq!(mcp_post(&url, &token, ping).0, 200);
        assert_eq!(mcp_post(&url, "falsch", ping).0, 401);

        // Aufrufe landen im Protokoll, ohne Inhalte.
        ok(&s, "create_vault", json!({"password": PASSWORD}));
        let call = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"create_record","arguments":{"kind":"schule","fields":{"name":"Vertrauliche Schule"}}}}"#;
        let (status, body) = mcp_post(&url, &token, call);
        assert_eq!(status, 200);
        assert!(body.contains("\"isError\":false"), "{body}");
        let audit = ok(&s, "mcp_audit", json!({"limit": 10}));
        assert_eq!(audit[0]["tool"], "create_record");
        assert!(!audit.to_string().contains("Vertrauliche"));
        assert_eq!(ok(&s, "list", json!({"kind": "schule"}))[0]["fields"]["name"], "Vertrauliche Schule");

        // Neues Token: das alte verliert den Zugriff, der Server läuft weiter.
        let renewed = ok(&s, "mcp_new_token", Value::Null);
        let new_token = renewed["token"].as_str().unwrap().to_string();
        assert_ne!(new_token, token);
        assert_eq!(renewed["running"], true);
        let new_url = renewed["url"].as_str().unwrap().to_string();
        assert_eq!(mcp_post(&new_url, &token, ping).0, 401);
        assert_eq!(mcp_post(&new_url, &new_token, ping).0, 200);

        // Ausschalten.
        let off = ok(&s, "mcp_set", json!({"enabled": false, "port": 0, "write": false, "allow_personal": []}));
        assert_eq!((off["running"].clone(), off["url"].clone()), (json!(false), Value::Null));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(mcp_post(&new_url, &new_token, ping).0, 0, "ausgeschalteter Server antwortet nicht");
    }

    #[test]
    fn mcp_settings_survive_a_restart() {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-api-mcp-{name}"));
        let first = App::new(Service::open(&dir, FAST).unwrap());
        ok(&first, "mcp_set", json!({"enabled": false, "port": 9123, "write": true, "allow_personal": ["ansprechpartner"]}));
        let token = ok(&first, "mcp_get", Value::Null)["token"].clone();
        drop(first);

        let second = App::new(Service::open(&dir, FAST).unwrap());
        let state = ok(&second, "mcp_get", Value::Null);
        assert_eq!(state["port"], 9123);
        assert_eq!(state["write"], true);
        assert_eq!(state["allow_personal"], json!(["ansprechpartner"]));
        assert_eq!(state["token"], token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("mcp.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "mcp.json enthält das Token und darf nur dem Nutzer gehören");
        }
    }

    #[test]
    fn busy_port_is_reported() {
        let s = service();
        let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = blocker.local_addr().unwrap().port();
        if port < 1024 {
            return;
        }
        let e = err(&s, "mcp_set", json!({"enabled": true, "port": port, "write": false}));
        assert_eq!(e.code, "settings");
        assert!(e.message.contains(&port.to_string()));
        assert_eq!(ok(&s, "mcp_get", Value::Null)["running"], false);
    }

    #[test]
    fn trip_commands_work_through_json() {
        let s = service();
        ok(&s, "create_vault", json!({"password": PASSWORD}));
        let files = std::env::temp_dir().join(format!("bdb-api-files-{}", std::process::id()));
        ok(&s, "config_set", json!({"server_url": "", "token": "", "attachments_dir": files.to_string_lossy()}));

        // Stammdaten: erst leer, dann gefüllt; das Passwort ist getrennt.
        let empty = ok(&s, "stammdaten_get", Value::Null);
        assert_eq!((empty["fields"].clone(), empty["smtp_password_set"].clone()), (json!({}), json!(false)));
        let trip = ok(&s, "save", json!({"kind": "reise", "fields": {"zweck": "Beratung", "geschaeftsort": "Ulm",
            "reise_beginn": "2026-11-16T08:00", "dienstgeschaeft_beginn": "2026-11-16T09:00",
            "dienstgeschaeft_ende": "2026-11-16T16:00", "reise_ende": "2026-11-16T17:00"}}))["id"].as_str().unwrap().to_string();
        assert_eq!(err(&s, "reise_antrag", json!({"id": trip})).code, "invalid", "ohne Stammdaten kein Antrag");

        ok(&s, "stammdaten_save", json!({"fields": {"name": "Mustermann", "vorname": "Max", "plz": "89073", "ort": "Ulm",
            "strasse": "Hauptstraße 1", "dienststelle_kurz": "Schule", "personal_nr": "1"}}));
        ok(&s, "smtp_password_set", json!({"password": "geheim"}));
        assert_eq!(ok(&s, "stammdaten_get", Value::Null)["smtp_password_set"], true);
        assert_eq!(err(&s, "stammdaten_save", json!({"fields": {"unsinn": 1}})).code, "invalid");

        let path = ok(&s, "reise_antrag", json!({"id": trip}))["path"].as_str().unwrap().to_string();
        assert!(path.ends_with(".pdf"));
        let read = ok(&s, "file_read", json!({"path": path}));
        let bytes = B64.decode(read["data"].as_str().unwrap()).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(ok(&s, "file_path", json!({"path": path}))["path"].as_str().unwrap().starts_with(&*files.to_string_lossy()));
        assert_eq!(err(&s, "file_read", json!({"path": "../x"})).code, "invalid");
        assert_eq!(err(&s, "file_read", json!({"path": "gibt/es/nicht"})).code, "not_found");

        // Dateien als Base64: falsche Kodierung, kein Formular, Anhang.
        assert_eq!(err(&s, "reise_formular", json!({"id": trip, "name": "x.pdf", "data": "***"})).code, "bad_request");
        assert_eq!(err(&s, "reise_formular", json!({"id": trip, "name": "x.pdf", "data": B64.encode(b"kein pdf")})).code, "invalid");
        let id = ok(&s, "anhang_add", json!({"reise": trip, "art": "Fahrkarte", "name": "ticket.pdf", "data": B64.encode(b"%PDF ticket")}))["id"].as_str().unwrap().to_string();
        ok(&s, "anhang_delete", json!({"id": id}));
        assert_eq!(err(&s, "anhang_delete", json!({"id": id})).code, "not_found");

        // Mail: Entwurf braucht das Dokument, Senden ohne Empfänger scheitert verständlich.
        let draft = ok(&s, "mail_draft", json!({"id": trip, "kind": "antrag"}));
        assert!(draft["subject"].as_str().unwrap().starts_with("Antrag auf Genehmigung einer Dienstreise am 16.11.2026"));
        assert_eq!(err(&s, "mail_draft", json!({"id": trip, "kind": "abrechnung"})).code, "invalid");
        assert_eq!(err(&s, "mail_draft", json!({"id": trip, "kind": "sonst"})).code, "invalid");
        let e = err(&s, "mail_send", json!({"id": trip, "kind": "antrag", "subject": "x", "body": "y"}));
        assert_eq!(e.code, "invalid");
        assert!(e.message.contains("fehlt"), "{}", e.message);
        std::fs::remove_dir_all(&files).ok();
    }
}
