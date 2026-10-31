//! Klartextformat der Einträge und ihre Zusammenführung.
//!
//! Ein [`Record`] ist ein Dokument mit Typ (`kind`) und benannten Feldern. Jedes Feld trägt einen
//! Zeitstempel ([`Stamp`], hybride logische Uhr). Beim Zusammenführen gewinnt je Feld der jüngere Stempel.
//! Das ist kommutativ, assoziativ und idempotent: Alle Rechner kommen unabhängig von der Reihenfolge der
//! Synchronisation zum selben Ergebnis.
//!
//! Löschen ist ein Register (`deleted`) mit eigenem Stempel. Ein Eintrag lässt sich also wiederherstellen.
//!
//! Echte Konflikte (dasselbe Feld wurde auf zwei Rechnern verschieden geändert) erkennt [`conflicts`] mit
//! einem Dreiwegevergleich gegen den zuletzt synchronisierten Stand. Den Verlierer verwirft die
//! Zusammenführung nicht stillschweigend, die Anwendung kann daraus zum Beispiel eine Konfliktkopie machen.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Aktuelle Version des Klartextformats.
pub const FORMAT_VERSION: u32 = 1;

const COUNTER_MAX: u32 = 0xffff;

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("Die Einträge haben verschiedene Typen ({0} und {1})")]
    KindMismatch(String, String),
    #[error("Nicht unterstütztes Eintragsformat (Version {0})")]
    UnsupportedFormat(u32),
    #[error("Ungültiger Eintrag: {0}")]
    Invalid(String),
    #[error("Ungültiger Zeitstempel: {0}")]
    InvalidStamp(String),
    #[error("Zufallsquelle nicht verfügbar")]
    Random,
}

/// Zeitstempel der hybriden logischen Uhr. Die Zeichenkettenform ist so aufgebaut, dass die
/// alphabetische Reihenfolge der zeitlichen Reihenfolge entspricht:
/// `<Millisekunden, 13 Stellen>-<Zähler, 4 Hexstellen>-<Knoten-ID>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Stamp(String);

impl Stamp {
    pub fn new(wall_ms: u64, counter: u32, node: &str) -> Result<Self, RecordError> {
        if wall_ms > 9_999_999_999_999 {
            return Err(RecordError::InvalidStamp("Zeit zu groß".into()));
        }
        if counter > COUNTER_MAX {
            return Err(RecordError::InvalidStamp("Zähler zu groß".into()));
        }
        validate_node(node)?;
        Ok(Stamp(format!("{wall_ms:013}-{counter:04x}-{node}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Zeit in Millisekunden, Zähler und Knoten-ID.
    pub fn parts(&self) -> (u64, u32, &str) {
        let mut it = self.0.splitn(3, '-');
        let wall = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let counter = it.next().and_then(|s| u32::from_str_radix(s, 16).ok()).unwrap_or(0);
        let node = it.next().unwrap_or("");
        (wall, counter, node)
    }

    fn parse(text: &str) -> Result<Self, RecordError> {
        let bad = || RecordError::InvalidStamp(text.to_string());
        let mut it = text.splitn(3, '-');
        let wall = it.next().ok_or_else(bad)?;
        let counter = it.next().ok_or_else(bad)?;
        let node = it.next().ok_or_else(bad)?;
        if wall.len() != 13 || !wall.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        if counter.len() != 4 || !counter.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(bad());
        }
        validate_node(node).map_err(|_| bad())?;
        Ok(Stamp(text.to_string()))
    }
}

fn validate_node(node: &str) -> Result<(), RecordError> {
    let valid = !node.is_empty()
        && node.len() <= 32
        && node.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if valid {
        Ok(())
    } else {
        Err(RecordError::InvalidStamp(format!("Knoten-ID „{node}“")))
    }
}

impl PartialOrd for Stamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Stamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl fmt::Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Stamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Stamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Stamp::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Uhr eines Rechners. Sie liefert streng aufsteigende Stempel, auch wenn die Systemzeit stehen bleibt oder
/// zurückspringt, und übernimmt Stempel anderer Rechner, damit eigene Änderungen nach dem Empfangen fremder
/// immer jünger sind.
#[derive(Clone, Debug)]
pub struct Clock {
    node: String,
    wall: u64,
    counter: u32,
}

impl Clock {
    pub fn new(node: &str) -> Result<Self, RecordError> {
        validate_node(node)?;
        Ok(Self { node: node.to_string(), wall: 0, counter: 0 })
    }

    /// Zufällige Knoten-ID für einen neuen Rechner (8 Hexstellen).
    pub fn random_node_id() -> Result<String, RecordError> {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).map_err(|_| RecordError::Random)?;
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    pub fn node(&self) -> &str {
        &self.node
    }

    /// Nächster eigener Stempel. `now_ms` ist die Systemzeit in Millisekunden seit 1970.
    pub fn tick(&mut self, now_ms: u64) -> Stamp {
        if now_ms > self.wall {
            self.wall = now_ms;
            self.counter = 0;
        } else if self.counter >= COUNTER_MAX {
            self.wall += 1;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
        Stamp::new(self.wall, self.counter, &self.node).expect("Uhr erzeugt nur gültige Stempel")
    }

    /// Übernimmt einen fremden Stempel, damit der nächste eigene Stempel jünger ist.
    pub fn observe(&mut self, remote: &Stamp) {
        let (wall, counter, _) = remote.parts();
        if (wall, counter) > (self.wall, self.counter) {
            self.wall = wall;
            self.counter = counter;
        }
    }

    /// Übernimmt alle Stempel eines Eintrags.
    pub fn observe_record(&mut self, record: &Record) {
        for stamp in record.stamps() {
            self.observe(stamp);
        }
    }
}

/// Aktuelle Systemzeit in Millisekunden seit 1970.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Neue zufällige Eintrags-ID (UUID Version 4).
pub fn new_id() -> Result<String, RecordError> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|_| RecordError::Random)?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub v: Value,
    pub t: Stamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Deletion {
    v: bool,
    t: Stamp,
}

/// Ein Feld, das auf beiden Seiten verschieden geändert wurde.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub field: String,
    pub local: Value,
    pub remote: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    format: u32,
    kind: String,
    fields: BTreeMap<String, Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deleted: Option<Deletion>,
}

impl Record {
    pub fn new(kind: &str) -> Result<Self, RecordError> {
        if kind.is_empty() {
            return Err(RecordError::Invalid("Typ darf nicht leer sein".into()));
        }
        Ok(Self {
            format: FORMAT_VERSION,
            kind: kind.to_string(),
            fields: BTreeMap::new(),
            deleted: None,
        })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Setzt ein Feld, wenn der Stempel jünger als der vorhandene ist. Gibt zurück, ob das Feld geändert
    /// wurde. `Value::Null` löscht den Inhalt des Feldes (das Feld bleibt als Stempel erhalten).
    pub fn set(&mut self, field: &str, value: Value, stamp: Stamp) -> Result<bool, RecordError> {
        if field.is_empty() {
            return Err(RecordError::Invalid("Feldname darf nicht leer sein".into()));
        }
        match self.fields.get(field) {
            Some(existing) if existing.t >= stamp => Ok(false),
            _ => {
                self.fields.insert(field.to_string(), Field { v: value, t: stamp });
                Ok(true)
            }
        }
    }

    /// Wert eines Feldes. Fehlende Felder und `null` liefern `None`.
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field).map(|f| &f.v).filter(|v| !v.is_null())
    }

    pub fn get_str(&self, field: &str) -> Option<&str> {
        self.get(field).and_then(Value::as_str)
    }

    /// Alle Felder mit Inhalt.
    pub fn values(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.fields
            .iter()
            .filter(|(_, f)| !f.v.is_null())
            .map(|(name, f)| (name.as_str(), &f.v))
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted.as_ref().is_some_and(|d| d.v)
    }

    /// Markiert den Eintrag als gelöscht (oder stellt ihn wieder her), wenn der Stempel jünger ist.
    pub fn set_deleted(&mut self, deleted: bool, stamp: Stamp) -> bool {
        match &self.deleted {
            Some(existing) if existing.t >= stamp => false,
            _ => {
                self.deleted = Some(Deletion { v: deleted, t: stamp });
                true
            }
        }
    }

    /// Alle Stempel des Eintrags.
    pub fn stamps(&self) -> impl Iterator<Item = &Stamp> {
        self.fields
            .values()
            .map(|f| &f.t)
            .chain(self.deleted.iter().map(|d| &d.t))
    }

    /// Führt zwei Stände zusammen. Das Ergebnis hängt nicht von der Reihenfolge ab.
    pub fn merge(&self, other: &Record) -> Result<Record, RecordError> {
        if self.kind != other.kind {
            return Err(RecordError::KindMismatch(self.kind.clone(), other.kind.clone()));
        }
        let mut fields = self.fields.clone();
        for (name, theirs) in &other.fields {
            match fields.get(name) {
                Some(mine) if !field_wins(theirs, mine) => {}
                _ => {
                    fields.insert(name.clone(), theirs.clone());
                }
            }
        }
        let deleted = match (&self.deleted, &other.deleted) {
            (Some(a), Some(b)) => Some(if deletion_wins(b, a) { b.clone() } else { a.clone() }),
            (Some(a), None) => Some(a.clone()),
            (None, Some(b)) => Some(b.clone()),
            (None, None) => None,
        };
        Ok(Record { format: FORMAT_VERSION, kind: self.kind.clone(), fields, deleted })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Eintrag lässt sich immer serialisieren")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RecordError> {
        let record: Record =
            serde_json::from_slice(bytes).map_err(|e| RecordError::Invalid(e.to_string()))?;
        if record.format != FORMAT_VERSION {
            return Err(RecordError::UnsupportedFormat(record.format));
        }
        if record.kind.is_empty() || record.fields.keys().any(String::is_empty) {
            return Err(RecordError::Invalid("leerer Typ oder Feldname".into()));
        }
        Ok(record)
    }
}

/// `new` löst `old` ab: jüngerer Stempel, bei gleichem Stempel entscheidet der Inhalt (deterministisch).
/// `Value` hat keine Ordnung, deshalb vergleicht der Gleichstand die JSON-Zeichenketten.
#[allow(clippy::cmp_owned)]
fn field_wins(new: &Field, old: &Field) -> bool {
    match new.t.cmp(&old.t) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => new.v.to_string() > old.v.to_string(),
    }
}

fn deletion_wins(new: &Deletion, old: &Deletion) -> bool {
    match new.t.cmp(&old.t) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => new.v && !old.v,
    }
}

/// Felder, die seit `base` auf beiden Seiten verschieden geändert wurden. `base` ist der zuletzt
/// synchronisierte Stand (`None` für einen Eintrag, den beide Seiten unabhängig angelegt haben).
pub fn conflicts(base: Option<&Record>, local: &Record, remote: &Record) -> Vec<Conflict> {
    let value = |r: Option<&Record>, name: &str| r.and_then(|r| r.get(name)).cloned().unwrap_or(Value::Null);

    let mut names: Vec<&String> = local.fields.keys().chain(remote.fields.keys()).collect();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .filter_map(|name| {
            let l = value(Some(local), name);
            let r = value(Some(remote), name);
            let b = value(base, name);
            (l != r && l != b && r != b).then(|| Conflict { field: name.clone(), local: l, remote: r })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stamp(wall: u64, counter: u32, node: &str) -> Stamp {
        Stamp::new(wall, counter, node).unwrap()
    }

    fn rec_with(fields: &[(&str, Value, Stamp)]) -> Record {
        let mut r = Record::new("schule").unwrap();
        for (name, value, s) in fields {
            r.set(name, value.clone(), s.clone()).unwrap();
        }
        r
    }

    #[test]
    fn stamp_order_follows_time_counter_node() {
        assert!(stamp(1, 0, "aa") < stamp(2, 0, "aa"));
        assert!(stamp(1, 1, "aa") > stamp(1, 0, "ff"));
        assert!(stamp(1, 0, "ab") > stamp(1, 0, "aa"));
        assert!(stamp(1, 0x10, "aa") > stamp(1, 0x9, "aa"));
        assert!(stamp(1000, 0, "aa") > stamp(999, 0xffff, "aa"));
    }

    #[test]
    fn stamp_roundtrip_and_validation() {
        let s = stamp(1_700_000_000_123, 17, "a1b2c3d4");
        assert_eq!(s.as_str(), "1700000000123-0011-a1b2c3d4");
        assert_eq!(s.parts(), (1_700_000_000_123, 17, "a1b2c3d4"));
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Stamp>(&json).unwrap(), s);

        for bad in ["", "1-0-aa", "1700000000123-0011", "1700000000123-001-aa", "1700000000123-0011-XY",
                    "17000000001234-0011-aa", "1700000000123-00G1-aa", "1700000000123-0011-"] {
            assert!(serde_json::from_str::<Stamp>(&format!("\"{bad}\"")).is_err(), "{bad}");
        }
        assert!(Stamp::new(0, 0x1_0000, "aa").is_err());
        assert!(Stamp::new(0, 0, "").is_err());
        assert!(Stamp::new(0, 0, "zz").is_err());
        assert!(Stamp::new(10_000_000_000_000, 0, "aa").is_err());
    }

    #[test]
    fn clock_is_strictly_increasing() {
        let mut clock = Clock::new("aa").unwrap();
        let mut last = clock.tick(1000);
        // Zeit steht still.
        for _ in 0..5 {
            let next = clock.tick(1000);
            assert!(next > last);
            last = next;
        }
        // Zeit springt zurück.
        let next = clock.tick(500);
        assert!(next > last);
        last = next;
        // Zeit läuft wieder vor.
        let next = clock.tick(2000);
        assert!(next > last);
        assert_eq!(next.parts().1, 0);
    }

    #[test]
    fn clock_counter_overflow_advances_time() {
        let mut clock = Clock::new("aa").unwrap();
        let mut last = clock.tick(1000);
        for _ in 0..(COUNTER_MAX as usize + 5) {
            let next = clock.tick(1000);
            assert!(next > last);
            last = next;
        }
    }

    #[test]
    fn clock_observes_remote_stamps() {
        let mut clock = Clock::new("aa").unwrap();
        clock.observe(&stamp(5000, 3, "bb"));
        // Eigene Systemzeit ist hinter der fremden: der nächste Stempel ist trotzdem jünger.
        assert!(clock.tick(1000) > stamp(5000, 3, "bb"));
        // Ältere fremde Stempel ändern nichts.
        let before = clock.tick(1000);
        clock.observe(&stamp(10, 0, "bb"));
        assert!(clock.tick(1000) > before);
    }

    #[test]
    fn ids_are_uuid_v4_and_unique() {
        let a = new_id().unwrap();
        let b = new_id().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(matches!(a.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(a.bytes().all(|c| c == b'-' || c.is_ascii_hexdigit()));
    }

    #[test]
    fn set_get_and_null() {
        let mut r = Record::new("schule").unwrap();
        assert!(r.set("name", json!("Grundschule"), stamp(1, 0, "aa")).unwrap());
        assert_eq!(r.get_str("name"), Some("Grundschule"));
        assert_eq!(r.get("fehlt"), None);

        // Älterer oder gleicher Stempel ändert nichts.
        assert!(!r.set("name", json!("alt"), stamp(1, 0, "aa")).unwrap());
        assert!(!r.set("name", json!("alt"), stamp(0, 9, "aa")).unwrap());
        assert_eq!(r.get_str("name"), Some("Grundschule"));

        // null leert das Feld.
        assert!(r.set("name", Value::Null, stamp(2, 0, "aa")).unwrap());
        assert_eq!(r.get("name"), None);
        assert_eq!(r.values().count(), 0);

        assert!(r.set("", json!(1), stamp(3, 0, "aa")).is_err());
        assert!(Record::new("").is_err());
    }

    #[test]
    fn newer_stamp_wins_per_field() {
        let a = rec_with(&[("name", json!("A"), stamp(1, 0, "aa")), ("ort", json!("Ulm"), stamp(5, 0, "aa"))]);
        let b = rec_with(&[("name", json!("B"), stamp(2, 0, "bb")), ("ort", json!("Bonn"), stamp(3, 0, "bb"))]);
        let m = a.merge(&b).unwrap();
        assert_eq!(m.get_str("name"), Some("B"));
        assert_eq!(m.get_str("ort"), Some("Ulm"));
    }

    #[test]
    fn merge_unions_fields() {
        let a = rec_with(&[("name", json!("A"), stamp(1, 0, "aa"))]);
        let b = rec_with(&[("ort", json!("Bonn"), stamp(1, 0, "bb"))]);
        let m = a.merge(&b).unwrap();
        assert_eq!(m.get_str("name"), Some("A"));
        assert_eq!(m.get_str("ort"), Some("Bonn"));
    }

    #[test]
    fn merge_rejects_different_kinds() {
        let a = Record::new("schule").unwrap();
        let b = Record::new("person").unwrap();
        assert!(matches!(a.merge(&b), Err(RecordError::KindMismatch(_, _))));
    }

    #[test]
    fn deletion_is_a_register_and_can_be_undone() {
        let mut a = rec_with(&[("name", json!("A"), stamp(1, 0, "aa"))]);
        let mut b = a.clone();
        assert!(!a.is_deleted());

        assert!(a.set_deleted(true, stamp(2, 0, "aa")));
        assert!(a.is_deleted());
        // Älterer Stempel ändert nichts.
        assert!(!a.set_deleted(false, stamp(1, 5, "aa")));

        // Eine Änderung auf b ohne Löschung ändert das Löschen nicht.
        b.set("ort", json!("Ulm"), stamp(3, 0, "bb")).unwrap();
        let m = a.merge(&b).unwrap();
        assert!(m.is_deleted());
        assert_eq!(m.get_str("ort"), Some("Ulm"));

        // Wiederherstellen mit jüngerem Stempel.
        b.set_deleted(false, stamp(4, 0, "bb"));
        assert!(!a.merge(&b).unwrap().is_deleted());
        assert!(!b.merge(&a).unwrap().is_deleted());
    }

    #[test]
    fn equal_stamps_resolve_deterministically() {
        let a = rec_with(&[("name", json!("A"), stamp(1, 0, "aa"))]);
        let b = rec_with(&[("name", json!("B"), stamp(1, 0, "aa"))]);
        assert_eq!(a.merge(&b).unwrap(), b.merge(&a).unwrap());
    }

    #[test]
    fn serialization_roundtrip_is_stable() {
        let mut r = rec_with(&[("name", json!("Äpfel"), stamp(1, 0, "aa")), ("zahl", json!(3), stamp(2, 0, "aa"))]);
        r.set_deleted(true, stamp(3, 0, "aa"));
        let bytes = r.to_bytes();
        assert_eq!(Record::from_bytes(&bytes).unwrap(), r);
        assert_eq!(Record::from_bytes(&bytes).unwrap().to_bytes(), bytes);
    }

    #[test]
    fn from_bytes_rejects_garbage_and_unknown_format() {
        assert!(Record::from_bytes(b"kein json").is_err());
        assert!(Record::from_bytes(br#"{"format":1,"kind":"","fields":{}}"#).is_err());
        assert!(Record::from_bytes(br#"{"format":1,"kind":"x","fields":{"":{"v":1,"t":"0000000000001-0000-aa"}}}"#).is_err());
        assert!(matches!(
            Record::from_bytes(br#"{"format":2,"kind":"x","fields":{}}"#),
            Err(RecordError::UnsupportedFormat(2))
        ));
        assert!(Record::from_bytes(br#"{"format":1,"kind":"x","fields":{"a":{"v":1,"t":"kaputt"}}}"#).is_err());
    }

    #[test]
    fn conflicts_need_changes_on_both_sides() {
        let base = rec_with(&[("name", json!("Alt"), stamp(1, 0, "aa")), ("ort", json!("Ulm"), stamp(1, 0, "aa"))]);
        let mut local = base.clone();
        let mut remote = base.clone();

        // Nur lokal geändert: kein Konflikt.
        local.set("name", json!("Lokal"), stamp(2, 0, "aa")).unwrap();
        assert!(conflicts(Some(&base), &local, &remote).is_empty());

        // Auf beiden Seiten gleich geändert: kein Konflikt.
        remote.set("name", json!("Lokal"), stamp(3, 0, "bb")).unwrap();
        assert!(conflicts(Some(&base), &local, &remote).is_empty());

        // Auf beiden Seiten verschieden geändert: Konflikt.
        remote.set("name", json!("Remote"), stamp(4, 0, "bb")).unwrap();
        let found = conflicts(Some(&base), &local, &remote);
        assert_eq!(
            found,
            vec![Conflict { field: "name".into(), local: json!("Lokal"), remote: json!("Remote") }]
        );

        // Verschiedene Felder geändert: kein Konflikt, beide Änderungen bleiben erhalten.
        let mut local = base.clone();
        let mut remote = base.clone();
        local.set("name", json!("Lokal"), stamp(2, 0, "aa")).unwrap();
        remote.set("ort", json!("Bonn"), stamp(2, 0, "bb")).unwrap();
        assert!(conflicts(Some(&base), &local, &remote).is_empty());
        let m = local.merge(&remote).unwrap();
        assert_eq!((m.get_str("name"), m.get_str("ort")), (Some("Lokal"), Some("Bonn")));
    }

    #[test]
    fn conflicts_without_base_compare_both_sides() {
        let local = rec_with(&[("name", json!("A"), stamp(1, 0, "aa"))]);
        let remote = rec_with(&[("name", json!("B"), stamp(1, 0, "bb"))]);
        assert_eq!(conflicts(None, &local, &remote).len(), 1);
        assert!(conflicts(None, &local, &local).is_empty());
    }

    /// Einfacher Zufallsgenerator, damit die Eigenschaftstests ohne weitere Abhängigkeit reproduzierbar sind.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn random_record(rng: &mut Lcg, node: &str) -> Record {
        let mut clock = Clock::new(node).unwrap();
        let mut r = Record::new("schule").unwrap();
        for _ in 0..rng.below(6) {
            let field = format!("f{}", rng.below(4));
            let value = match rng.below(3) {
                0 => Value::Null,
                1 => json!(rng.below(5)),
                _ => json!(format!("text{}", rng.below(5))),
            };
            let s = clock.tick(1 + rng.below(8));
            r.set(&field, value, s).unwrap();
        }
        if rng.below(3) == 0 {
            let s = clock.tick(1 + rng.below(8));
            r.set_deleted(rng.below(2) == 0, s);
        }
        r
    }

    #[test]
    fn merge_is_commutative_associative_and_idempotent() {
        let mut rng = Lcg(42);
        for round in 0..500 {
            // Verschiedene Knoten-IDs, aber bewusst viele gleiche Zeiten, damit auch Gleichstände vorkommen.
            let a = random_record(&mut rng, "aa");
            let b = random_record(&mut rng, "bb");
            let c = random_record(&mut rng, "aa");

            assert_eq!(a.merge(&b).unwrap(), b.merge(&a).unwrap(), "kommutativ, Runde {round}");
            assert_eq!(
                a.merge(&b).unwrap().merge(&c).unwrap(),
                a.merge(&b.merge(&c).unwrap()).unwrap(),
                "assoziativ, Runde {round}"
            );
            assert_eq!(a.merge(&a).unwrap(), a, "idempotent, Runde {round}");
            let m = a.merge(&b).unwrap();
            assert_eq!(m.merge(&a).unwrap(), m, "Absorption, Runde {round}");
        }
    }
}
