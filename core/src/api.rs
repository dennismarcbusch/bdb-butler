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

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::crypto;
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
struct SaveArgs {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    fields: Map<String, Value>,
}

/// Führt einen Befehl aus. Der Dienst ist durch ein Mutex geschützt, es läuft immer nur ein Befehl.
pub fn call(service: &Mutex<Service>, command: &str, args: &Value) -> ApiResult {
    let mut service = service
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
        other => Err(ApiError::new("unknown_command", format!("Unbekannter Befehl „{other}“"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
    const PASSWORD: &str = "korrekt pferd batterie heftklammer";

    fn service() -> Mutex<Service> {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-api-test-{name}"));
        Mutex::new(Service::open(&dir, FAST).unwrap())
    }

    fn ok(s: &Mutex<Service>, cmd: &str, args: Value) -> Value {
        call(s, cmd, &args).unwrap_or_else(|e| panic!("{cmd}: {e:?}"))
    }

    fn err(s: &Mutex<Service>, cmd: &str, args: Value) -> ApiError {
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
}
