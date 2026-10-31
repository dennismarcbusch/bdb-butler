//! Lokale Datenbank. Sie speichert wie der Server nur Chiffretext (dieselben Blobs, die synchronisiert
//! werden), dazu pro Eintrag den zuletzt synchronisierten Stand (`base`) für den Dreiwegevergleich und ein
//! Kennzeichen, ob lokale Änderungen noch zum Server müssen.
//!
//! Beim Einspielen fremder Änderungen führt [`Store::apply_remote`] lokale und fremde Stände zusammen.
//! Echte Konflikte (dasselbe Feld wurde auf beiden Seiten verschieden geändert) werden als Einträge vom Typ
//! [`CONFLICT_KIND`] festgehalten, damit nichts stillschweigend verloren geht.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use crate::crypto::{self, Keyring, Vault};
use crate::record::{self, Clock, Record, RecordError};

/// Typ der Einträge, die einen Konflikt festhalten.
pub const CONFLICT_KIND: &str = "konflikt";

const META_NODE_ID: &str = "node_id";
const META_CURSOR: &str = "server_cursor";
const META_KEYRING: &str = "keyring";
const META_KEYRING_VERSION: &str = "keyring_version";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Datenbank: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Verschlüsselung: {0}")]
    Crypto(#[from] crypto::Error),
    #[error("Eintrag: {0}")]
    Record(#[from] RecordError),
    #[error("Beschädigte lokale Daten: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Ein Eintrag, wie ihn der Server liefert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub id: String,
    pub seq: i64,
    pub blob: Vec<u8>,
}

/// Eine lokale Änderung, die noch zum Server muss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    /// Folgenummer des Standes auf dem Server, auf dem die Änderung beruht (0 für neue Einträge).
    pub base_seq: i64,
    pub blob: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Neuer Eintrag.
    Inserted,
    /// Lokal unverändert, durch den Serverstand ersetzt.
    Replaced,
    /// Lokale Änderungen und Serverstand wurden zusammengeführt.
    Merged { conflicts: usize, needs_push: bool },
    /// Der Stand war schon bekannt.
    Skipped,
}

struct Row {
    blob: Vec<u8>,
    seq: i64,
    base: Option<Vec<u8>>,
    dirty: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS meta (
                 key   TEXT PRIMARY KEY,
                 value BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS entries (
                 id    TEXT PRIMARY KEY,
                 blob  BLOB NOT NULL,
                 seq   INTEGER NOT NULL DEFAULT 0,
                 base  BLOB,
                 dirty INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        Ok(Self { conn })
    }

    // ----- Metadaten -------------------------------------------------------------------------------------

    fn meta_get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    fn meta_set(&self, key: &str, value: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Höchste Folgenummer, die vom Server geholt wurde.
    pub fn cursor(&self) -> Result<i64> {
        match self.meta_get(META_CURSOR)? {
            None => Ok(0),
            Some(bytes) => std::str::from_utf8(&bytes)
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| StoreError::Corrupt("Cursor".into())),
        }
    }

    pub fn set_cursor(&self, seq: i64) -> Result<()> {
        self.meta_set(META_CURSOR, seq.to_string().as_bytes())
    }

    /// Lokale Kopie des Keyrings samt Versionsnummer vom Server (0, wenn der Keyring nur lokal existiert).
    pub fn keyring(&self) -> Result<Option<(Keyring, i64)>> {
        let Some(bytes) = self.meta_get(META_KEYRING)? else { return Ok(None) };
        let json = String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("Keyring".into()))?;
        let keyring = Keyring::from_json(&json)?;
        let version = match self.meta_get(META_KEYRING_VERSION)? {
            None => 0,
            Some(b) => std::str::from_utf8(&b)
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| StoreError::Corrupt("Keyring-Version".into()))?,
        };
        Ok(Some((keyring, version)))
    }

    pub fn set_keyring(&self, keyring: &Keyring, version: i64) -> Result<()> {
        self.meta_set(META_KEYRING, keyring.to_json().as_bytes())?;
        self.meta_set(META_KEYRING_VERSION, version.to_string().as_bytes())
    }

    // ----- Geheimnisse dieses Rechners --------------------------------------------------------------------

    /// Geheimnisse dieses Rechners (Sync-Token, MCP-Token, SMTP-Passwort) liegen mit dem Datenschlüssel
    /// verschlüsselt in den Metadaten. Sie werden nicht synchronisiert. Die ID für die zusätzlichen
    /// authentifizierten Daten enthält `/`, das in Eintrags-IDs des Servers nicht vorkommen kann, damit sich
    /// Geheimnis und Eintrag nie vertauschen lassen.
    fn secret_key(name: &str) -> (String, String) {
        (format!("secret:{name}"), format!("local/{name}"))
    }

    pub fn secret_exists(&self, name: &str) -> Result<bool> {
        Ok(self.meta_get(&Self::secret_key(name).0)?.is_some())
    }

    pub fn secret_get(&self, vault: &Vault, name: &str) -> Result<Option<String>> {
        let (key, aad_id) = Self::secret_key(name);
        let Some(blob) = self.meta_get(&key)? else { return Ok(None) };
        let plain = zeroize::Zeroizing::new(vault.decrypt_entry(&aad_id, &blob)?);
        let text = String::from_utf8(plain.to_vec()).map_err(|_| StoreError::Corrupt(format!("Geheimnis {name}")))?;
        Ok(Some(text))
    }

    /// Speichert ein Geheimnis verschlüsselt. Ein leerer Wert löscht es.
    pub fn secret_set(&self, vault: &Vault, name: &str, value: &str) -> Result<()> {
        let (key, aad_id) = Self::secret_key(name);
        if value.is_empty() {
            self.conn.execute("DELETE FROM meta WHERE key = ?1", [&key])?;
            return Ok(());
        }
        let blob = vault.encrypt_entry(&aad_id, value.as_bytes())?;
        self.meta_set(&key, &blob)
    }

    /// Uhr dieses Rechners. Die Knoten-ID wird beim ersten Aufruf erzeugt. Die Uhr übernimmt alle
    /// vorhandenen Stempel, damit neue Änderungen immer jünger sind.
    pub fn load_clock(&self, vault: &Vault) -> Result<Clock> {
        let node = match self.meta_get(META_NODE_ID)? {
            Some(bytes) => String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("Knoten-ID".into()))?,
            None => {
                let node = Clock::random_node_id()?;
                self.meta_set(META_NODE_ID, node.as_bytes())?;
                node
            }
        };
        let mut clock = Clock::new(&node)?;
        for (_, rec) in self.list(vault)? {
            clock.observe_record(&rec);
        }
        Ok(clock)
    }

    // ----- Einträge --------------------------------------------------------------------------------------

    fn row(&self, id: &str) -> Result<Option<Row>> {
        Ok(self
            .conn
            .query_row("SELECT blob, seq, base, dirty FROM entries WHERE id = ?1", [id], |r| {
                Ok(Row { blob: r.get(0)?, seq: r.get(1)?, base: r.get(2)?, dirty: r.get::<_, i64>(3)? != 0 })
            })
            .optional()?)
    }

    /// Speichert den neuen lokalen Stand eines Eintrags und merkt ihn zum Senden vor.
    pub fn put_record(&self, vault: &Vault, id: &str, record: &Record) -> Result<()> {
        let blob = vault.encrypt_entry(id, &record.to_bytes())?;
        self.conn.execute(
            "INSERT INTO entries (id, blob, seq, base, dirty) VALUES (?1, ?2, 0, NULL, 1)
             ON CONFLICT(id) DO UPDATE SET blob = excluded.blob, dirty = 1",
            params![id, blob],
        )?;
        Ok(())
    }

    pub fn get_record(&self, vault: &Vault, id: &str) -> Result<Option<Record>> {
        match self.row(id)? {
            None => Ok(None),
            Some(row) => Ok(Some(decrypt(vault, id, &row.blob)?)),
        }
    }

    /// Alle Einträge, entschlüsselt, nach ID sortiert. Gelöschte Einträge sind enthalten
    /// (siehe [`Record::is_deleted`]).
    pub fn list(&self, vault: &Vault) -> Result<Vec<(String, Record)>> {
        let mut stmt = self.conn.prepare("SELECT id, blob FROM entries ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, blob) = row?;
            let rec = decrypt(vault, &id, &blob)?;
            out.push((id, rec));
        }
        Ok(out)
    }

    /// Lokale Änderungen, die noch zum Server müssen.
    pub fn pending(&self) -> Result<Vec<Pending>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, seq, blob FROM entries WHERE dirty = 1 ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Pending { id: r.get(0)?, base_seq: r.get(1)?, blob: r.get(2)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Der Server hat `blob` unter der Folgenummer `seq` angenommen. Hat sich der Eintrag lokal inzwischen
    /// geändert, bleibt er zum Senden vorgemerkt.
    pub fn mark_pushed(&self, id: &str, seq: i64, blob: &[u8]) -> Result<()> {
        self.conn.execute(
            "UPDATE entries SET seq = ?2, base = ?3, dirty = (blob != ?3) WHERE id = ?1",
            params![id, seq, blob],
        )?;
        Ok(())
    }

    /// Spielt einen Eintrag vom Server ein.
    pub fn apply_remote(&mut self, vault: &Vault, clock: &mut Clock, remote: &RemoteEntry) -> Result<ApplyOutcome> {
        let remote_rec = decrypt(vault, &remote.id, &remote.blob)?;
        clock.observe_record(&remote_rec);

        let tx = self.conn.transaction()?;
        let existing = tx
            .query_row("SELECT blob, seq, base, dirty FROM entries WHERE id = ?1", [&remote.id], |r| {
                Ok(Row { blob: r.get(0)?, seq: r.get(1)?, base: r.get(2)?, dirty: r.get::<_, i64>(3)? != 0 })
            })
            .optional()?;

        let outcome = match existing {
            None => {
                tx.execute(
                    "INSERT INTO entries (id, blob, seq, base, dirty) VALUES (?1, ?2, ?3, ?2, 0)",
                    params![remote.id, remote.blob, remote.seq],
                )?;
                ApplyOutcome::Inserted
            }
            Some(row) if remote.seq <= row.seq => ApplyOutcome::Skipped,
            Some(row) if !row.dirty => {
                tx.execute(
                    "UPDATE entries SET blob = ?2, seq = ?3, base = ?2, dirty = 0 WHERE id = ?1",
                    params![remote.id, remote.blob, remote.seq],
                )?;
                ApplyOutcome::Replaced
            }
            Some(row) => {
                let local_rec = decrypt(vault, &remote.id, &row.blob)?;
                let base_rec = row
                    .base
                    .as_deref()
                    .map(|b| decrypt(vault, &remote.id, b))
                    .transpose()?;
                let merged = local_rec.merge(&remote_rec)?;
                let found = record::conflicts(base_rec.as_ref(), &local_rec, &remote_rec);
                let needs_push = merged != remote_rec;
                let merged_blob = vault.encrypt_entry(&remote.id, &merged.to_bytes())?;
                tx.execute(
                    "UPDATE entries SET blob = ?2, seq = ?3, base = ?4, dirty = ?5 WHERE id = ?1",
                    params![remote.id, merged_blob, remote.seq, remote.blob, needs_push as i64],
                )?;
                for conflict in &found {
                    let mut rec = Record::new(CONFLICT_KIND)?;
                    let stamp = clock.tick(record::now_ms());
                    let fields = [
                        ("eintrag", json!(remote.id)),
                        ("eintragstyp", json!(merged.kind())),
                        ("feld", json!(conflict.field)),
                        ("lokal", conflict.local.clone()),
                        ("fremd", conflict.remote.clone()),
                    ];
                    for (name, value) in fields {
                        rec.set(name, value, stamp.clone())?;
                    }
                    let id = record::new_id()?;
                    let blob = vault.encrypt_entry(&id, &rec.to_bytes())?;
                    tx.execute(
                        "INSERT INTO entries (id, blob, seq, base, dirty) VALUES (?1, ?2, 0, NULL, 1)",
                        params![id, blob],
                    )?;
                }
                ApplyOutcome::Merged { conflicts: found.len(), needs_push }
            }
        };
        tx.commit()?;
        Ok(outcome)
    }
}

fn decrypt(vault: &Vault, id: &str, blob: &[u8]) -> Result<Record> {
    let plain = vault.decrypt_entry(id, blob)?;
    Ok(Record::from_bytes(&plain)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };

    fn vault() -> Vault {
        Vault::create("passwort", FAST).unwrap().0
    }

    fn school(clock: &mut Clock, name: &str) -> Record {
        let mut r = Record::new("schule").unwrap();
        r.set("name", json!(name), clock.tick(1000)).unwrap();
        r
    }

    /// Zwei Rechner mit demselben Tresor (so wie nach dem Entsperren auf zwei Rechnern).
    struct Device {
        store: Store,
        clock: Clock,
    }

    fn device(vault: &Vault, node: &str) -> Device {
        let store = Store::open_in_memory().unwrap();
        store.meta_set(META_NODE_ID, node.as_bytes()).unwrap();
        let clock = store.load_clock(vault).unwrap();
        Device { store, clock }
    }

    fn remote_from(d: &Device, id: &str, seq: i64) -> RemoteEntry {
        RemoteEntry { id: id.into(), seq, blob: d.store.row(id).unwrap().unwrap().blob }
    }

    #[test]
    fn put_and_get_roundtrip() {
        let v = vault();
        let mut d = device(&v, "aa");
        let rec = school(&mut d.clock, "Grundschule");
        d.store.put_record(&v, "s1", &rec).unwrap();

        assert_eq!(d.store.get_record(&v, "s1").unwrap().unwrap(), rec);
        assert_eq!(d.store.get_record(&v, "fehlt").unwrap(), None);
        assert_eq!(d.store.list(&v).unwrap().len(), 1);
    }

    #[test]
    fn database_holds_only_ciphertext() {
        let v = vault();
        let mut d = device(&v, "aa");
        d.store
            .put_record(&v, "s1", &school(&mut d.clock, "Geheimschule"))
            .unwrap();
        let blob = d.store.row("s1").unwrap().unwrap().blob;
        assert!(!blob.windows(12).any(|w| w == b"Geheimschule"));
    }

    #[test]
    fn wrong_vault_cannot_read() {
        let v = vault();
        let other = vault();
        let mut d = device(&v, "aa");
        d.store.put_record(&v, "s1", &school(&mut d.clock, "A")).unwrap();
        assert!(d.store.get_record(&other, "s1").is_err());
    }

    #[test]
    fn new_entries_are_pending_until_pushed() {
        let v = vault();
        let mut d = device(&v, "aa");
        d.store.put_record(&v, "s1", &school(&mut d.clock, "A")).unwrap();

        let pending = d.store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!((pending[0].id.as_str(), pending[0].base_seq), ("s1", 0));

        d.store.mark_pushed("s1", 7, &pending[0].blob).unwrap();
        assert!(d.store.pending().unwrap().is_empty());
        assert_eq!(d.store.row("s1").unwrap().unwrap().seq, 7);
    }

    #[test]
    fn change_during_push_stays_pending() {
        let v = vault();
        let mut d = device(&v, "aa");
        d.store.put_record(&v, "s1", &school(&mut d.clock, "A")).unwrap();
        let pushed = d.store.pending().unwrap().remove(0);

        // Während des Sendens ändert der Nutzer den Eintrag erneut.
        let mut changed = d.store.get_record(&v, "s1").unwrap().unwrap();
        changed.set("name", json!("B"), d.clock.tick(2000)).unwrap();
        d.store.put_record(&v, "s1", &changed).unwrap();

        d.store.mark_pushed("s1", 1, &pushed.blob).unwrap();
        let pending = d.store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].base_seq, 1);
    }

    #[test]
    fn secrets_are_encrypted_and_bound_to_their_name() {
        let v = vault();
        let s = Store::open_in_memory().unwrap();
        assert!(!s.secret_exists("token").unwrap());
        s.secret_set(&v, "token", "sehr-geheim").unwrap();
        assert!(s.secret_exists("token").unwrap());
        assert_eq!(s.secret_get(&v, "token").unwrap().as_deref(), Some("sehr-geheim"));
        let raw = s.meta_get("secret:token").unwrap().unwrap();
        assert!(!raw.windows(11).any(|w| w == b"sehr-geheim"));
        // Unter anderem Namen abgelegt, lässt sich der Blob nicht lesen.
        s.meta_set("secret:anderes", &raw).unwrap();
        assert!(s.secret_get(&v, "anderes").is_err());
        assert!(s.secret_get(&vault(), "token").is_err());
        s.secret_set(&v, "token", "").unwrap();
        assert_eq!(s.secret_get(&v, "token").unwrap(), None);
    }

    #[test]
    fn cursor_and_keyring_persist() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.cursor().unwrap(), 0);
        s.set_cursor(42).unwrap();
        assert_eq!(s.cursor().unwrap(), 42);

        assert!(s.keyring().unwrap().is_none());
        let (_, keyring, _) = Vault::create("passwort", FAST).unwrap();
        s.set_keyring(&keyring, 3).unwrap();
        assert_eq!(s.keyring().unwrap().unwrap(), (keyring, 3));
    }

    #[test]
    fn node_id_is_stable_and_clock_resumes() {
        let dir = tempdir();
        let path = dir.join("db.sqlite");
        let v = vault();

        let store = Store::open(&path).unwrap();
        let mut clock = store.load_clock(&v).unwrap();
        let node = clock.node().to_string();
        let mut rec = Record::new("schule").unwrap();
        let last = clock.tick(record::now_ms() + 10_000_000);
        rec.set("name", json!("A"), last.clone()).unwrap();
        store.put_record(&v, "s1", &rec).unwrap();
        drop(store);

        // Neustart: gleiche Knoten-ID, und neue Stempel sind jünger als alle gespeicherten.
        let store = Store::open(&path).unwrap();
        let mut clock = store.load_clock(&v).unwrap();
        assert_eq!(clock.node(), node);
        assert!(clock.tick(0) > last);
    }

    #[test]
    fn remote_insert_and_replace() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");

        let mut rec = school(&mut a.clock, "A");
        a.store.put_record(&v, "s1", &rec).unwrap();
        let remote = remote_from(&a, "s1", 1);
        assert_eq!(b.store.apply_remote(&v, &mut b.clock, &remote).unwrap(), ApplyOutcome::Inserted);
        assert!(b.store.pending().unwrap().is_empty());
        assert_eq!(b.store.get_record(&v, "s1").unwrap().unwrap(), rec);

        // Eine neuere Fassung ersetzt die lokale, solange lokal nichts geändert wurde.
        rec.set("name", json!("A2"), a.clock.tick(2000)).unwrap();
        a.store.put_record(&v, "s1", &rec).unwrap();
        let remote = remote_from(&a, "s1", 2);
        assert_eq!(b.store.apply_remote(&v, &mut b.clock, &remote).unwrap(), ApplyOutcome::Replaced);
        assert_eq!(b.store.get_record(&v, "s1").unwrap().unwrap().get_str("name"), Some("A2"));
    }

    #[test]
    fn known_or_older_remote_is_skipped() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");
        a.store.put_record(&v, "s1", &school(&mut a.clock, "A")).unwrap();
        let remote = remote_from(&a, "s1", 5);
        b.store.apply_remote(&v, &mut b.clock, &remote).unwrap();

        assert_eq!(b.store.apply_remote(&v, &mut b.clock, &remote).unwrap(), ApplyOutcome::Skipped);
        let older = RemoteEntry { seq: 4, ..remote };
        assert_eq!(b.store.apply_remote(&v, &mut b.clock, &older).unwrap(), ApplyOutcome::Skipped);
    }

    #[test]
    fn concurrent_changes_to_different_fields_merge_without_conflict() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");

        // Gemeinsamer Ausgangsstand auf beiden Rechnern.
        let mut base = school(&mut a.clock, "Alt");
        base.set("ort", json!("Ulm"), a.clock.tick(1001)).unwrap();
        a.store.put_record(&v, "s1", &base).unwrap();
        let blob = a.store.pending().unwrap().remove(0).blob;
        a.store.mark_pushed("s1", 1, &blob).unwrap();
        b.store
            .apply_remote(&v, &mut b.clock, &RemoteEntry { id: "s1".into(), seq: 1, blob })
            .unwrap();

        // A ändert den Namen und sendet, B ändert den Ort lokal.
        let mut rec_a = base.clone();
        rec_a.set("name", json!("Neu"), a.clock.tick(2000)).unwrap();
        a.store.put_record(&v, "s1", &rec_a).unwrap();
        let remote = remote_from(&a, "s1", 2);

        let mut rec_b = base.clone();
        rec_b.set("ort", json!("Bonn"), b.clock.tick(2001)).unwrap();
        b.store.put_record(&v, "s1", &rec_b).unwrap();

        let outcome = b.store.apply_remote(&v, &mut b.clock, &remote).unwrap();
        assert_eq!(outcome, ApplyOutcome::Merged { conflicts: 0, needs_push: true });
        let merged = b.store.get_record(&v, "s1").unwrap().unwrap();
        assert_eq!((merged.get_str("name"), merged.get_str("ort")), (Some("Neu"), Some("Bonn")));
        // Nur der Eintrag selbst muss zum Server, es gibt keine Konfliktkopie.
        let pending = b.store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].base_seq, 2);
    }

    #[test]
    fn concurrent_change_to_same_field_creates_conflict_entry() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");

        let base = school(&mut a.clock, "Alt");
        a.store.put_record(&v, "s1", &base).unwrap();
        let blob = a.store.pending().unwrap().remove(0).blob;
        a.store.mark_pushed("s1", 1, &blob).unwrap();
        b.store
            .apply_remote(&v, &mut b.clock, &RemoteEntry { id: "s1".into(), seq: 1, blob })
            .unwrap();

        let mut rec_a = base.clone();
        rec_a.set("name", json!("Von A"), a.clock.tick(2000)).unwrap();
        a.store.put_record(&v, "s1", &rec_a).unwrap();
        let remote = remote_from(&a, "s1", 2);

        let mut rec_b = base.clone();
        rec_b.set("name", json!("Von B"), b.clock.tick(3000)).unwrap();
        b.store.put_record(&v, "s1", &rec_b).unwrap();

        let outcome = b.store.apply_remote(&v, &mut b.clock, &remote).unwrap();
        assert_eq!(outcome, ApplyOutcome::Merged { conflicts: 1, needs_push: true });

        // Der Jüngere gewinnt ...
        let merged = b.store.get_record(&v, "s1").unwrap().unwrap();
        assert_eq!(merged.get_str("name"), Some("Von B"));

        // ... und der Konflikt ist als eigener Eintrag festgehalten, der ebenfalls synchronisiert wird.
        let conflicts: Vec<_> = b
            .store
            .list(&v)
            .unwrap()
            .into_iter()
            .filter(|(_, r)| r.kind() == CONFLICT_KIND)
            .collect();
        assert_eq!(conflicts.len(), 1);
        let (_, c) = &conflicts[0];
        assert_eq!(c.get_str("eintrag"), Some("s1"));
        assert_eq!(c.get_str("eintragstyp"), Some("schule"));
        assert_eq!(c.get_str("feld"), Some("name"));
        assert_eq!(c.get_str("lokal"), Some("Von B"));
        assert_eq!(c.get_str("fremd"), Some("Von A"));
        assert_eq!(b.store.pending().unwrap().len(), 2);
    }

    #[test]
    fn merge_that_equals_remote_needs_no_push() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");

        let base = school(&mut a.clock, "Alt");
        a.store.put_record(&v, "s1", &base).unwrap();
        let blob = a.store.pending().unwrap().remove(0).blob;
        a.store.mark_pushed("s1", 1, &blob).unwrap();
        b.store
            .apply_remote(&v, &mut b.clock, &RemoteEntry { id: "s1".into(), seq: 1, blob })
            .unwrap();

        // B ändert lokal, aber A hat danach etwas Jüngeres für dasselbe Feld geschrieben.
        let mut rec_b = base.clone();
        rec_b.set("name", json!("Von B"), b.clock.tick(2000)).unwrap();
        b.store.put_record(&v, "s1", &rec_b).unwrap();

        let mut rec_a = base.clone();
        rec_a.set("name", json!("Von A"), a.clock.tick(9000)).unwrap();
        a.store.put_record(&v, "s1", &rec_a).unwrap();
        let remote = remote_from(&a, "s1", 2);

        let outcome = b.store.apply_remote(&v, &mut b.clock, &remote).unwrap();
        // Der Konflikt wird festgehalten, aber der Eintrag selbst entspricht dem Serverstand.
        assert_eq!(outcome, ApplyOutcome::Merged { conflicts: 1, needs_push: false });
        assert_eq!(b.store.get_record(&v, "s1").unwrap().unwrap().get_str("name"), Some("Von A"));
        assert!(b.store.pending().unwrap().iter().all(|p| p.id != "s1"));
    }

    #[test]
    fn remote_entry_of_other_id_cannot_be_swapped_in() {
        let v = vault();
        let mut a = device(&v, "aa");
        let mut b = device(&v, "bb");
        a.store.put_record(&v, "s1", &school(&mut a.clock, "A")).unwrap();
        let blob = a.store.row("s1").unwrap().unwrap().blob;
        // Ein böswilliger Server schiebt den Blob von s1 unter der ID s2 unter.
        let result = b.store.apply_remote(&v, &mut b.clock, &RemoteEntry { id: "s2".into(), seq: 1, blob });
        assert!(result.is_err());
        assert!(b.store.list(&v).unwrap().is_empty());
    }

    fn tempdir() -> std::path::PathBuf {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-core-test-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
