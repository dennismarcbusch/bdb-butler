//! Geheimnisse dieses Rechners, die nicht synchronisiert werden (zurzeit das SMTP-Passwort).
//!
//! Sie liegen in `secrets.json` im Datenordner der App mit Zugriffsrechten nur für den Nutzer (Linux und macOS).
//! Der Ordner ist durch die Laufwerksverschlüsselung des Rechners geschützt. Eine Ablage im Schlüsselbund des
//! Betriebssystems ist noch nicht umgesetzt. Der Zugriff läuft über diese Schnittstelle, damit sich das später
//! austauschen lässt, ohne den Rest der Anwendung zu ändern.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "secrets.json";

pub struct Secrets {
    path: PathBuf,
}

impl Secrets {
    pub fn new(dir: &Path) -> Self {
        Self { path: dir.join(FILE) }
    }

    fn load(&self) -> BTreeMap<String, String> {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn store(&self, map: &BTreeMap<String, String>) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(map).map_err(io::Error::other)?;
        // Erst in eine Nebendatei schreiben und dann umbenennen, damit nie eine halbe Datei zurückbleibt.
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&tmp, &self.path)
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.load().remove(key)
    }

    pub fn is_set(&self, key: &str) -> bool {
        self.load().contains_key(key)
    }

    /// Setzt ein Geheimnis. Ein leerer Wert löscht es.
    pub fn set(&self, key: &str, value: &str) -> io::Result<()> {
        let mut map = self.load();
        if value.is_empty() {
            map.remove(key);
        } else {
            map.insert(key.to_string(), value.to_string());
        }
        self.store(&map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-secrets-test-{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stores_reads_and_deletes() {
        let secrets = Secrets::new(&dir());
        assert_eq!(secrets.get("smtp"), None);
        assert!(!secrets.is_set("smtp"));
        secrets.set("smtp", "geheim mit \"Anführungszeichen\" und äöü").unwrap();
        assert_eq!(secrets.get("smtp").as_deref(), Some("geheim mit \"Anführungszeichen\" und äöü"));
        assert!(secrets.is_set("smtp"));
        secrets.set("andere", "x").unwrap();
        secrets.set("smtp", "").unwrap();
        assert!(!secrets.is_set("smtp"));
        assert_eq!(secrets.get("andere").as_deref(), Some("x"), "andere Geheimnisse bleiben");
    }

    #[test]
    fn survives_a_restart_and_survives_a_damaged_file() {
        let d = dir();
        Secrets::new(&d).set("smtp", "wert").unwrap();
        assert_eq!(Secrets::new(&d).get("smtp").as_deref(), Some("wert"));
        fs::write(d.join(FILE), b"kaputt").unwrap();
        assert_eq!(Secrets::new(&d).get("smtp"), None, "eine beschädigte Datei ergibt keinen Absturz");
        Secrets::new(&d).set("smtp", "neu").unwrap();
        assert_eq!(Secrets::new(&d).get("smtp").as_deref(), Some("neu"));
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir();
        Secrets::new(&d).set("smtp", "wert").unwrap();
        let mode = fs::metadata(d.join(FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
        assert!(!d.join("secrets.json.tmp").exists());
    }
}
