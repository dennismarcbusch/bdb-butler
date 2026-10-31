//! Integrationstest: Der Rust-Client synchronisiert gegen den echten Go-Server aus `../server`.
//!
//! Der Test baut den Server mit `go build` und startet ihn auf einem freien Port. Ist `go` nicht
//! installiert, werden die Tests mit einem Hinweis übersprungen.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bdb_core::crypto::KdfParams;
use bdb_core::record::{self, Clock, Record};
use bdb_core::store::{Store, CONFLICT_KIND};
use bdb_core::sync::{self, SyncClient, SyncError};
use bdb_core::Vault;
use serde_json::json;

const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
const TOKEN: &str = "0123456789abcdef0123456789abcdef-integration";
const PASSWORD: &str = "korrekt pferd batterie heftklammer";

fn random_name() -> String {
    let mut bytes = [0u8; 6];
    getrandom::getrandom(&mut bytes).unwrap();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Baut die Server-Binärdatei einmal pro Testlauf. `None`, wenn Go fehlt.
fn server_binary() -> Option<&'static PathBuf> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let out = std::env::temp_dir().join(format!("bdbb-server-{}", random_name()));
            let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../server");
            let status = Command::new("go")
                .args(["build", "-o"])
                .arg(&out)
                .arg(".")
                .current_dir(server_dir)
                .status();
            match status {
                Ok(s) if s.success() => Some(out),
                Ok(s) => panic!("go build ist fehlgeschlagen: {s}"),
                Err(_) => None,
            }
        })
        .as_ref()
}

struct TestServer {
    child: Child,
    url: String,
    dir: PathBuf,
}

impl TestServer {
    fn start() -> Option<Self> {
        let Some(binary) = server_binary() else {
            eprintln!("Go nicht gefunden, Integrationstest übersprungen");
            return None;
        };
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let dir = std::env::temp_dir().join(format!("bdbb-it-{}", random_name()));
        std::fs::create_dir_all(&dir).unwrap();
        let child = Command::new(binary)
            .env("BDBB_TOKEN", TOKEN)
            .env("BDBB_ADDR", format!("127.0.0.1:{port}"))
            .env("BDBB_DB", dir.join("server.db"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Server starten");
        let url = format!("http://127.0.0.1:{port}");
        let server = TestServer { child, url, dir };
        server.wait_until_ready();
        Some(server)
    }

    fn wait_until_ready(&self) {
        let agent = ureq::agent();
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if agent.get(&format!("{}/healthz", self.url)).call().is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("Server wurde nicht bereit");
    }

    fn client(&self) -> SyncClient {
        SyncClient::new(&self.url, TOKEN)
    }

    /// Rohe Bytes aller Datenbankdateien des Servers (auch Write-Ahead-Log).
    fn raw_database_bytes(&self) -> Vec<u8> {
        let mut all = Vec::new();
        for entry in std::fs::read_dir(&self.dir).unwrap() {
            all.extend(std::fs::read(entry.unwrap().path()).unwrap());
        }
        all
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Ein Rechner: lokale Datenbank, entsperrter Tresor, Uhr und Verbindung zum Server.
struct Device {
    store: Store,
    vault: Vault,
    clock: Clock,
    client: SyncClient,
}

impl Device {
    fn sync(&mut self) -> sync::SyncReport {
        sync::sync(&mut self.store, &self.vault, &mut self.clock, &self.client).expect("sync")
    }

    fn new_record(&mut self, kind: &str, fields: &[(&str, serde_json::Value)]) -> String {
        let id = record::new_id().unwrap();
        let mut rec = Record::new(kind).unwrap();
        let stamp = self.clock.tick(record::now_ms());
        for (name, value) in fields {
            rec.set(name, value.clone(), stamp.clone()).unwrap();
        }
        self.store.put_record(&self.vault, &id, &rec).unwrap();
        id
    }

    fn edit(&mut self, id: &str, field: &str, value: serde_json::Value) {
        let mut rec = self.store.get_record(&self.vault, id).unwrap().unwrap();
        let stamp = self.clock.tick(record::now_ms());
        rec.set(field, value, stamp).unwrap();
        self.store.put_record(&self.vault, id, &rec).unwrap();
    }

    fn get(&self, id: &str) -> Option<Record> {
        self.store.get_record(&self.vault, id).unwrap()
    }
}

/// Gerät A legt den Tresor an, Gerät B tritt bei.
fn two_devices(server: &TestServer) -> (Device, Device) {
    let store_a = Store::open_in_memory().unwrap();
    let (vault_a, _recovery) = sync::create_vault(&store_a, &server.client(), PASSWORD, FAST).unwrap();
    let clock_a = store_a.load_clock(&vault_a).unwrap();
    let a = Device { store: store_a, vault: vault_a, clock: clock_a, client: server.client() };

    let store_b = Store::open_in_memory().unwrap();
    let vault_b = sync::join_vault(&store_b, &server.client(), PASSWORD).unwrap();
    let clock_b = store_b.load_clock(&vault_b).unwrap();
    let b = Device { store: store_b, vault: vault_b, clock: clock_b, client: server.client() };
    (a, b)
}

#[test]
fn entries_travel_between_devices() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, mut b) = two_devices(&server);

    let id = a.new_record("schule", &[("name", json!("Grundschule am See"))]);
    let report = a.sync();
    assert_eq!((report.pushed, report.pulled), (1, 0));

    let report = b.sync();
    assert_eq!(report.pulled, 1);
    assert_eq!(b.get(&id).unwrap().get_str("name"), Some("Grundschule am See"));

    // B ändert, A holt die Änderung.
    b.edit(&id, "name", json!("Grundschule am Fluss"));
    b.sync();
    a.sync();
    assert_eq!(a.get(&id).unwrap().get_str("name"), Some("Grundschule am Fluss"));

    // Ein weiterer Abgleich ohne Änderungen tut nichts.
    assert_eq!(a.sync(), sync::SyncReport::default());
    assert_eq!(b.sync(), sync::SyncReport::default());
}

#[test]
fn server_stores_only_ciphertext() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, _) = two_devices(&server);
    a.new_record("schule", &[("name", json!("Geheimschule Nordwind")), ("notiz", json!("vertraulich"))]);
    a.sync();

    let raw = server.raw_database_bytes();
    assert!(!raw.is_empty());
    for secret in ["Geheimschule", "Nordwind", "vertraulich", "schule", PASSWORD] {
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "Klartext „{secret}“ in der Server-Datenbank gefunden"
        );
    }
}

#[test]
fn concurrent_edits_to_different_fields_are_merged() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, mut b) = two_devices(&server);

    let id = a.new_record("schule", &[("name", json!("Alt")), ("ort", json!("Ulm"))]);
    a.sync();
    b.sync();

    // Beide ändern offline unterschiedliche Felder.
    a.edit(&id, "name", json!("Neu"));
    b.edit(&id, "ort", json!("Bonn"));
    a.sync();
    let report = b.sync();
    assert_eq!(report.conflicts, 0);
    a.sync();

    for device in [&a, &b] {
        let rec = device.get(&id).unwrap();
        assert_eq!((rec.get_str("name"), rec.get_str("ort")), (Some("Neu"), Some("Bonn")));
    }
}

#[test]
fn concurrent_edits_to_same_field_create_a_synced_conflict_entry() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, mut b) = two_devices(&server);

    let id = a.new_record("protokoll", &[("text", json!("Ausgangstext"))]);
    a.sync();
    b.sync();

    a.edit(&id, "text", json!("Fassung von A"));
    b.edit(&id, "text", json!("Fassung von B"));
    a.sync();
    let report = b.sync();
    assert_eq!(report.conflicts, 1);
    a.sync();

    // Beide Rechner haben denselben Eintrag und sehen den Konflikt.
    assert_eq!(a.get(&id).unwrap().to_bytes(), b.get(&id).unwrap().to_bytes());
    for device in [&a, &b] {
        let conflicts: Vec<_> = device
            .store
            .list(&device.vault)
            .unwrap()
            .into_iter()
            .filter(|(_, r)| r.kind() == CONFLICT_KIND)
            .collect();
        assert_eq!(conflicts.len(), 1, "jeder Rechner sieht genau einen Konflikt");
        let (_, c) = &conflicts[0];
        assert_eq!(c.get_str("eintrag"), Some(id.as_str()));
        assert_eq!(c.get_str("feld"), Some("text"));
        let texts = [c.get_str("lokal").unwrap(), c.get_str("fremd").unwrap()];
        assert!(texts.contains(&"Fassung von A") && texts.contains(&"Fassung von B"));
    }
}

#[test]
fn deletion_propagates_and_can_be_undone() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, mut b) = two_devices(&server);

    let id = a.new_record("auftrag", &[("titel", json!("Bericht"))]);
    a.sync();
    b.sync();

    let mut rec = a.get(&id).unwrap();
    rec.set_deleted(true, a.clock.tick(record::now_ms()));
    a.store.put_record(&a.vault, &id, &rec).unwrap();
    a.sync();
    b.sync();
    assert!(b.get(&id).unwrap().is_deleted());

    let mut rec = b.get(&id).unwrap();
    rec.set_deleted(false, b.clock.tick(record::now_ms()));
    b.store.put_record(&b.vault, &id, &rec).unwrap();
    b.sync();
    a.sync();
    assert!(!a.get(&id).unwrap().is_deleted());
}

#[test]
fn many_entries_are_paged() {
    let Some(server) = TestServer::start() else { return };
    let (mut a, mut b) = two_devices(&server);

    for i in 0..1100 {
        a.new_record("person", &[("nr", json!(i))]);
    }
    let report = a.sync();
    assert_eq!(report.pushed, 1100);

    let report = b.sync();
    assert_eq!(report.pulled, 1100);
    assert_eq!(b.store.list(&b.vault).unwrap().len(), 1100);
}

#[test]
fn vault_setup_and_join_rules() {
    let Some(server) = TestServer::start() else { return };

    // Beitreten ohne Tresor.
    let store = Store::open_in_memory().unwrap();
    assert!(matches!(
        sync::join_vault(&store, &server.client(), PASSWORD),
        Err(SyncError::NoVault)
    ));

    // Anlegen, dann nicht noch einmal.
    let (_, recovery) = sync::create_vault(&store, &server.client(), PASSWORD, FAST).unwrap();
    assert!(matches!(
        sync::create_vault(&Store::open_in_memory().unwrap(), &server.client(), "anderes", FAST),
        Err(SyncError::VaultExists)
    ));

    // Falsches Passwort.
    let other = Store::open_in_memory().unwrap();
    assert!(matches!(
        sync::join_vault(&other, &server.client(), "falsch"),
        Err(SyncError::Crypto(_))
    ));

    // Mit dem Recovery-Key lässt sich der lokal abgelegte Keyring entsperren.
    let (keyring, version) = other.keyring().unwrap().expect("Keyring bleibt lokal gespeichert");
    assert_eq!(version, 1);
    assert!(Vault::unlock_with_recovery(&keyring, &recovery.to_display()).is_ok());
}

#[test]
fn wrong_token_is_rejected() {
    let Some(server) = TestServer::start() else { return };
    let bad = SyncClient::new(&server.url, "ein-falsches-token-ein-falsches-token-x");
    assert!(matches!(bad.get_keyring(), Err(SyncError::Unauthorized)));
    assert!(matches!(bad.pull(0, 10), Err(SyncError::Unauthorized)));
    assert!(matches!(bad.push("abc", 0, b"x"), Err(SyncError::Unauthorized)));
}

#[test]
fn unreachable_server_is_a_network_error() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let client = SyncClient::new(&format!("http://127.0.0.1:{port}"), TOKEN);
    assert!(matches!(client.pull(0, 10), Err(SyncError::Network(_))));
}
