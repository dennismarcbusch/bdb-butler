//! Geheimnisse dieses Rechners, die nicht synchronisiert werden: Sync-Token, MCP-Token und SMTP-Passwort.
//!
//! Sie liegen mit dem Datenschlüssel verschlüsselt in der lokalen Datenbank (siehe
//! [`crate::store::Store::secret_set`]) und sind damit nur bei entsperrter App lesbar. Frühere Fassungen legten das
//! SMTP-Passwort im Klartext in `secrets.json` und die Token in `settings.json` und `mcp.json` ab. Diese Werte
//! übernimmt der Dienst beim ersten Entsperren und entfernt sie aus den Dateien (siehe
//! [`crate::service::Service`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Name des Sync-Tokens.
pub const SYNC_TOKEN: &str = "sync_token";
/// Name des Tokens für den KI-Zugriff.
pub const MCP_TOKEN: &str = "mcp_token";
/// Name des SMTP-Passworts.
pub const SMTP_PASSWORD: &str = "smtp_passwort";

const LEGACY_FILE: &str = "secrets.json";

/// Liest die alte Klartextdatei `secrets.json` (leer, wenn es sie nicht gibt oder sie unlesbar ist).
pub fn read_legacy(dir: &Path) -> BTreeMap<String, String> {
    fs::read(dir.join(LEGACY_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Entfernt die alte Klartextdatei und eine liegen gebliebene Nebendatei.
pub fn remove_legacy(dir: &Path) {
    let _ = fs::remove_file(dir.join(LEGACY_FILE));
    let _ = fs::remove_file(dir.join("secrets.json.tmp"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_file_is_read_and_removed() {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-secrets-test-{name}"));
        fs::create_dir_all(&dir).unwrap();
        assert!(read_legacy(&dir).is_empty());

        fs::write(dir.join(LEGACY_FILE), r#"{"smtp_passwort":"geheim"}"#).unwrap();
        assert_eq!(read_legacy(&dir).get(SMTP_PASSWORD).map(String::as_str), Some("geheim"));
        remove_legacy(&dir);
        assert!(!dir.join(LEGACY_FILE).exists());
    }
}
