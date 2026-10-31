//! Einmaliger Import der Daten des alten Dienstreisebusters (Django, SQLite) in die App.
//!
//! Übernommen werden die Stammdaten (samt Unterschrift und SMTP-Passwort), alle Reisen mit ihren Dateien (Antrag,
//! Abrechnung, Genehmigung, Formular des Veranstalters) und die Anhänge. Die Dateien werden in den Ordner für PDFs
//! und Anhänge kopiert, die alten Dateien bleiben unverändert.
//!
//! - **Wiederholbar:** Jeder importierte Eintrag bekommt eine feste ID (`import-reise-<Nr>`, `import-anhang-<Nr>`).
//!   Ein erneuter Import überspringt vorhandene Einträge und überschreibt nichts, was inzwischen in der App geändert
//!   wurde.
//! - **Zeiten:** Django speichert Zeitpunkte in UTC. Sie werden in die Ortszeit Europe/Berlin umgerechnet, wie es
//!   das alte Tool angezeigt hat.
//! - **Fehler** bei einzelnen Einträgen oder Dateien brechen den Import nicht ab, sie stehen im Bericht.

use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{NaiveDateTime, TimeZone};
use chrono_tz::Europe::Berlin;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::service::{Result, Service, ServiceError};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    /// `importiert`, `übersprungen` (es gibt schon Stammdaten) oder `keine` (die alte Datenbank hat keine).
    pub stammdaten: String,
    pub trips_imported: usize,
    pub trips_skipped: usize,
    pub attachments_imported: usize,
    pub attachments_skipped: usize,
    pub files_copied: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

type Row = Map<String, Value>;

fn invalid(msg: impl Into<String>) -> ServiceError {
    ServiceError::Invalid(msg.into())
}

/// Alle Zeilen einer Tabelle als Felder. Leere Texte, `NULL` und leere Dateifelder entfallen.
fn read_table(conn: &Connection, table: &str) -> Result<Vec<Row>> {
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY id"))
        .map_err(|_| invalid(format!("Das ist keine Datenbank des alten Dienstreisebusters (Tabelle {table} fehlt)")))?;
    let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut map = Row::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get::<_, SqlValue>(i)? {
                    SqlValue::Null => Value::Null,
                    SqlValue::Integer(n) => json!(n),
                    SqlValue::Real(r) => json!(r),
                    SqlValue::Text(t) => json!(t),
                    SqlValue::Blob(_) => Value::Null,
                };
                map.insert(name.clone(), value);
            }
            Ok(map)
        })
        .map_err(|e| invalid(format!("Alte Datenbank: {e}")))?;
    rows.collect::<std::result::Result<_, _>>().map_err(|e| invalid(format!("Alte Datenbank: {e}")))
}

fn text<'a>(row: &'a Row, name: &str) -> &'a str {
    row.get(name).and_then(Value::as_str).unwrap_or("").trim()
}

fn truthy(row: &Row, name: &str) -> bool {
    match row.get(name) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "1" | "true" | "True"),
        _ => false,
    }
}

fn number(row: &Row, name: &str) -> Option<f64> {
    match row.get(name)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

/// Django-Zeitpunkt (UTC, `JJJJ-MM-TT HH:MM:SS[.ffffff]`) als Ortszeit `JJJJ-MM-TTTHH:MM`.
pub fn utc_to_local(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('Z').replace('T', " ");
    let naive = NaiveDateTime::parse_from_str(&trimmed, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(&trimmed, "%Y-%m-%d %H:%M"))
        .ok()?;
    Some(chrono::Utc.from_utc_datetime(&naive).with_timezone(&Berlin).format("%Y-%m-%dT%H:%M").to_string())
}

fn map_choice(value: &str, table: &[(&str, &str)]) -> Option<String> {
    table.iter().find(|(old, _)| *old == value).map(|(_, new)| new.to_string())
}

const STATUS: &[(&str, &str)] = &[
    ("entwurf", "Entwurf"),
    ("beantragt", "Beantragt"),
    ("genehmigt", "Genehmigt"),
    ("abgerechnet", "Abgerechnet"),
    ("bezahlt", "Bezahlt"),
];
const ART: &[(&str, &str)] = &[("dienstreise", "Dienstreise"), ("fortbildung", "Fortbildungsreise")];
const ORT: &[(&str, &str)] = &[("wohnort", "Wohnort"), ("dienststelle", "Dienststelle"), ("anderer", "Anderer Ort")];
const MAHLZEIT: &[(&str, &str)] = &[("fruehstueck", "Frühstück"), ("mittagessen", "Mittagessen"), ("abendessen", "Abendessen")];
const ANHANG: &[(&str, &str)] = &[
    ("fahrkarte", "Fahrkarte"),
    ("teilnahme", "Teilnahmebescheinigung"),
    ("hotel", "Hotelrechnung"),
    ("sonstiges", "Sonstiges"),
];
const SICHERHEIT: &[(&str, &str)] = &[("starttls", "STARTTLS"), ("ssl", "SSL/TLS"), ("keine", "keine")];

/// Datei aus dem Medienordner der alten App lesen. Der Pfad darf den Ordner nicht verlassen.
fn read_media(media: &Path, relative: &str) -> std::result::Result<(String, Vec<u8>), String> {
    let rel = Path::new(relative);
    if relative.is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("ungültiger Dateipfad „{relative}“"));
    }
    let full: PathBuf = media.join(rel);
    if let (Ok(root), Ok(real)) = (media.canonicalize(), full.canonicalize()) {
        if !real.starts_with(root) {
            return Err(format!("Datei „{relative}“ liegt außerhalb des Medienordners"));
        }
    }
    let bytes = fs::read(&full).map_err(|_| format!("Datei „{relative}“ fehlt im Medienordner"))?;
    let name = full.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok((name, bytes))
}

fn put(fields: &mut Map<String, Value>, name: &str, value: Value) {
    match &value {
        Value::Null => {}
        Value::String(s) if s.trim().is_empty() => {}
        _ => {
            fields.insert(name.to_string(), value);
        }
    }
}

impl Service {
    /// Importiert die Daten des alten Dienstreisebusters. `db` ist die Datei `db.sqlite3`, `media` der Ordner `media`
    /// neben ihr. Der Dienst muss entsperrt sein.
    pub fn import_django(&mut self, db: &Path, media: &Path) -> Result<ImportReport> {
        let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| invalid(format!("Die Datenbank lässt sich nicht öffnen: {e}")))?;
        let settings = read_table(&conn, "reisen_einstellungen")?;
        let trips = read_table(&conn, "reisen_reise")?;
        let attachments = read_table(&conn, "reisen_anhang")?;

        let needs_files = trips.iter().any(|t| ["veranstalter_formular", "antrag_pdf", "genehmigung_pdf", "abrechnung_pdf"].iter().any(|f| !text(t, f).is_empty()))
            || !attachments.is_empty();
        if needs_files {
            // Vorab prüfen, damit der Import nicht halb läuft.
            self.files_root()?;
        }
        // Gesperrt: sofort abbrechen, statt für jeden Eintrag einen Fehler zu melden.
        self.stammdaten()?;
        let mut report = ImportReport { stammdaten: "keine".into(), ..ImportReport::default() };

        if let Some(row) = settings.first() {
            report.stammdaten = self.import_stammdaten(row, media, &mut report)?;
        }

        for row in &trips {
            let id = format!("import-reise-{}", row.get("id").and_then(Value::as_i64).unwrap_or(0));
            if self.get(&id).is_ok() {
                report.trips_skipped += 1;
                continue;
            }
            match self.import_trip(&id, row, media, &mut report) {
                Ok(()) => report.trips_imported += 1,
                Err(e) => report.errors.push(format!("Reise „{}“ ({}): {e}", text(row, "zweck"), id)),
            }
        }

        for row in &attachments {
            let nr = row.get("id").and_then(Value::as_i64).unwrap_or(0);
            let id = format!("import-anhang-{nr}");
            if self.get(&id).is_ok() {
                report.attachments_skipped += 1;
                continue;
            }
            let trip_id = format!("import-reise-{}", row.get("reise_id").and_then(Value::as_i64).unwrap_or(0));
            match self.import_attachment(&id, &trip_id, row, media, &mut report) {
                Ok(()) => report.attachments_imported += 1,
                Err(e) => report.errors.push(format!("Anhang {nr}: {e}")),
            }
        }
        Ok(report)
    }

    fn import_stammdaten(&mut self, row: &Row, media: &Path, report: &mut ImportReport) -> Result<String> {
        if !text(&self.stammdaten()?.fields, "name").is_empty() {
            report.warnings.push("Die Stammdaten in der App sind schon ausgefüllt und bleiben unverändert.".into());
            return Ok("übersprungen".into());
        }
        let mut fields = Map::new();
        for name in [
            "name", "vorname", "plz", "ort", "strasse", "telefon", "email", "dienststelle", "dienststelle_kurz", "abteilung",
            "besoldungsgruppe", "org_nr", "personal_nr", "iban", "bic", "kreditinstitut", "unterschrift_ort", "empfaenger_antrag",
            "empfaenger_abrechnung", "absender", "smtp_host", "smtp_user",
        ] {
            put(&mut fields, name, json!(text(row, name)));
        }
        if let Some(port) = number(row, "smtp_port") {
            put(&mut fields, "smtp_port", json!(port));
        }
        put(&mut fields, "smtp_sicherheit", map_choice(text(row, "smtp_sicherheit"), SICHERHEIT).map_or(Value::Null, Value::from));
        let svg_path = text(row, "unterschrift_svg");
        if !svg_path.is_empty() {
            match read_media(media, svg_path).and_then(|(_, bytes)| String::from_utf8(bytes).map_err(|_| "Unterschrift ist kein Text (SVG)".to_string())) {
                Ok(svg) => put(&mut fields, "unterschrift_svg", json!(svg)),
                Err(e) => report.warnings.push(format!("Unterschrift nicht übernommen: {e}")),
            }
        }
        self.save_stammdaten(&fields)?;
        let password = text(row, "smtp_passwort");
        if !password.is_empty() {
            self.set_smtp_password(password)?;
        }
        Ok("importiert".into())
    }

    fn import_trip(&mut self, id: &str, row: &Row, media: &Path, report: &mut ImportReport) -> Result<()> {
        let mut fields = Map::new();
        for name in ["zweck", "geschaeftsort", "abfahrt_von_text", "rueckkehr_nach_text", "uebernachtungskosten", "sonstige_kosten", "kostentraeger_dritte", "hinweise", "aktenzeichen"] {
            put(&mut fields, name, json!(text(row, name)));
        }
        for name in ["taegliche_rueckkehr", "oepnv", "bahn", "bahncard", "verpflegung_gestellt", "hotel_selbst_bezahlt"] {
            fields.insert(name.to_string(), json!(truthy(row, name)));
        }
        for name in ["reise_beginn", "dienstgeschaeft_beginn", "dienstgeschaeft_ende", "reise_ende", "antrag_versendet_am", "abrechnung_versendet_am"] {
            let raw = text(row, name);
            if raw.is_empty() {
                continue;
            }
            match utc_to_local(raw) {
                Some(local) => {
                    fields.insert(name.to_string(), json!(local));
                }
                None => return Err(invalid(format!("Zeitpunkt „{raw}“ im Feld {name} ist ungültig"))),
            }
        }
        for name in ["verpflegung_von", "verpflegung_bis", "bezahlt_am"] {
            put(&mut fields, name, json!(text(row, name).get(..10).unwrap_or("")));
        }
        for (name, table) in [("status", STATUS), ("art", ART), ("abfahrt_von", ORT), ("rueckkehr_nach", ORT), ("verpflegung_von_mahlzeit", MAHLZEIT), ("verpflegung_bis_mahlzeit", MAHLZEIT)] {
            put(&mut fields, name, map_choice(text(row, name), table).map_or(Value::Null, Value::from));
        }
        if let Some(price) = number(row, "fahrkartenpreis") {
            fields.insert("fahrkartenpreis".into(), json!(price));
        }
        self.save_with_id("reise", id, &fields)?;

        // Dateien in den Ordner der Reise kopieren und merken.
        let trip = self.get(id)?;
        let mut paths = Map::new();
        for (field, prefix) in [("veranstalter_formular", "veranstalter-"), ("antrag_pdf", ""), ("genehmigung_pdf", "genehmigung-"), ("abrechnung_pdf", "")] {
            let source = text(row, field);
            if source.is_empty() {
                continue;
            }
            match read_media(media, source) {
                Ok((name, bytes)) => {
                    let stored = self.write_trip_file(&trip, &format!("{prefix}{name}"), &bytes)?;
                    paths.insert(field.to_string(), json!(stored));
                    report.files_copied += 1;
                }
                Err(e) => report.warnings.push(format!("Reise „{}“: {e}", text(row, "zweck"))),
            }
        }
        if !paths.is_empty() {
            self.save("reise", Some(id), &paths)?;
        }
        Ok(())
    }

    fn import_attachment(&mut self, id: &str, trip_id: &str, row: &Row, media: &Path, report: &mut ImportReport) -> Result<()> {
        let trip = self.get(trip_id).map_err(|_| invalid("die zugehörige Reise wurde nicht importiert"))?;
        let (name, bytes) = read_media(media, text(row, "datei")).map_err(invalid)?;
        let stored = self.write_trip_file(&trip, &format!("anhang-{name}"), &bytes)?;
        let mut fields = Map::new();
        fields.insert("reise".into(), json!(trip_id));
        put(&mut fields, "art", map_choice(text(row, "art"), ANHANG).map_or(Value::Null, Value::from));
        fields.insert("name".into(), json!(name));
        fields.insert("datei".into(), json!(stored));
        fields.insert("beilegen".into(), json!(truthy(row, "beilegen")));
        if let Err(e) = self.save_with_id("anhang", id, &fields) {
            // Keine verwaiste Kopie zurücklassen.
            if let Ok(path) = self.file_path(&stored) {
                let _ = fs::remove_file(path);
            }
            return Err(e);
        }
        report.files_copied += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;
    use crate::service::Config;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/django")
    }

    fn temp(prefix: &str) -> PathBuf {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("{prefix}-{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn service() -> (Service, PathBuf) {
        let mut s = Service::open(&temp("bdb-import"), FAST).unwrap();
        s.create_vault("korrekt pferd batterie heftklammer").unwrap();
        let files = temp("bdb-import-dateien");
        s.set_config(Config { attachments_dir: files.to_string_lossy().to_string(), ..Config::default() }).unwrap();
        (s, files)
    }

    #[test]
    fn converts_utc_to_berlin_local_time() {
        assert_eq!(utc_to_local("2026-11-16 05:45:00").as_deref(), Some("2026-11-16T06:45"), "Winterzeit +1");
        assert_eq!(utc_to_local("2026-07-14 06:00:00.123456").as_deref(), Some("2026-07-14T08:00"), "Sommerzeit +2");
        assert_eq!(utc_to_local("2026-10-25 00:30:00").as_deref(), Some("2026-10-25T02:30"), "Tag der Zeitumstellung, noch Sommerzeit");
        assert_eq!(utc_to_local("2026-12-31 23:30:00").as_deref(), Some("2027-01-01T00:30"), "über den Jahreswechsel");
        assert_eq!(utc_to_local("2026-11-16T05:45:00Z").as_deref(), Some("2026-11-16T06:45"));
        assert_eq!(utc_to_local("kaputt"), None);
        assert_eq!(utc_to_local(""), None);
    }

    #[test]
    fn imports_settings_trips_files_and_attachments() {
        let (mut s, files) = service();
        let report = s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap();
        assert_eq!(report.stammdaten, "importiert");
        assert_eq!((report.trips_imported, report.trips_skipped, report.attachments_imported), (2, 0, 2));
        assert_eq!(report.files_copied, 4 + 2, "vier Dateien der ersten Reise und zwei Anhänge");
        assert!(report.errors.is_empty() && report.warnings.is_empty(), "{report:?}");

        // Stammdaten mit Unterschrift und Passwort.
        let st = s.stammdaten().unwrap().fields;
        assert_eq!((st["name"].as_str(), st["iban"].as_str(), st["smtp_sicherheit"].as_str()), (Some("Mustermann"), Some("DE89 3704 0044 0532 0130 00"), Some("SSL/TLS")));
        assert_eq!(st["smtp_port"], 465.0);
        assert!(st["unterschrift_svg"].as_str().unwrap().contains("<svg"));
        assert!(s.smtp_password_is_set());

        // Reisen: Zuordnung der Werte und Ortszeit.
        let winter = s.get("import-reise-1").unwrap().fields;
        assert_eq!(winter["zweck"], "Fortbildung Künstliche Intelligenz");
        assert_eq!((winter["status"].as_str(), winter["art"].as_str()), (Some("Bezahlt"), Some("Fortbildungsreise")));
        assert_eq!(winter["reise_beginn"], "2026-11-16T06:45");
        assert_eq!(winter["reise_ende"], "2026-11-17T18:20");
        assert_eq!(winter["abfahrt_von"], "Dienststelle");
        assert_eq!((winter["rueckkehr_nach"].as_str(), winter["rueckkehr_nach_text"].as_str()), (Some("Anderer Ort"), Some("Augsburg, Hauptbahnhof")));
        assert_eq!((winter["bahn"].clone(), winter["bahncard"].clone(), winter["oepnv"].clone()), (json!(true), json!(true), json!(false)));
        assert_eq!(winter["fahrkartenpreis"], 123.45);
        assert_eq!((winter["verpflegung_von_mahlzeit"].as_str(), winter["verpflegung_bis_mahlzeit"].as_str()), (Some("Mittagessen"), Some("Frühstück")));
        assert_eq!((winter["verpflegung_von"].as_str(), winter["bezahlt_am"].as_str()), (Some("2026-11-16"), Some("2026-12-01")));
        assert_eq!(winter["antrag_versendet_am"], "2026-10-01T09:30", "Sommerzeit am 1. Oktober");
        assert_eq!(winter["hinweise"], "Zeile eins\nZeile zwei");
        let summer = s.get("import-reise-2").unwrap().fields;
        assert_eq!(summer["reise_beginn"], "2026-07-14T08:00");
        assert_eq!((summer["status"].as_str(), summer["art"].as_str()), (Some("Entwurf"), Some("Dienstreise")));
        assert!(!summer.contains_key("antrag_pdf") && !summer.contains_key("fahrkartenpreis"));

        // Dateien: kopiert, im Ordner der Reise, mit Inhalt der alten Dateien.
        let antrag = winter["antrag_pdf"].as_str().unwrap();
        assert!(antrag.starts_with("Dienstreisen/2026-11-16-fortbildung-kuenstliche-intelligenz-") && antrag.ends_with("/antrag-2026-11-16.pdf"), "{antrag}");
        assert_eq!(s.file_read(antrag).unwrap().1, b"%PDF-1.4 antrag");
        assert!(winter["genehmigung_pdf"].as_str().unwrap().ends_with("/genehmigung-genehmigung.pdf"));
        assert_eq!(s.file_read(winter["abrechnung_pdf"].as_str().unwrap()).unwrap().1, b"%PDF-1.4 abrechnung");
        let form = s.file_read(winter["veranstalter_formular"].as_str().unwrap()).unwrap().1;
        assert_eq!(form, fs::read(fixture().join("../fibs_form.pdf")).unwrap());
        // Die alten Dateien bleiben unberührt.
        assert!(fixture().join("media/antraege/antrag-2026-11-16.pdf").exists());

        // Anhänge.
        let mut list = s.list("anhang").unwrap();
        list.sort_by_key(|a| a.fields["name"].as_str().unwrap().to_string());
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].fields["art"].as_str(), list[0].fields["name"].as_str(), list[0].fields["beilegen"].clone()), (Some("Hotelrechnung"), Some("hotel_rechnung.pdf"), json!(false)));
        assert_eq!((list[1].fields["art"].as_str(), list[1].fields["beilegen"].clone()), (Some("Fahrkarte"), json!(true)));
        assert_eq!(list[1].fields["reise"], "import-reise-1");
        assert_eq!(s.file_read(list[1].fields["datei"].as_str().unwrap()).unwrap().1, b"%PDF-1.4 ticket");
        assert!(files.join("Dienstreisen").is_dir());
    }

    #[test]
    fn imported_data_is_usable_in_the_app() {
        let (mut s, _) = service();
        s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap();
        // Antrag mit den importierten Stammdaten und der importierten Unterschrift erzeugen.
        let path = s.reise_antrag_erzeugen("import-reise-2").unwrap();
        let (_, bytes) = s.file_read(&path).unwrap();
        let values = crate::pdf::read_values(&bytes).unwrap();
        assert_eq!(values["S1 Name"], crate::pdf::FieldValue::text("Mustermann"));
        assert_eq!(values["S1 Hin Antritt"], crate::pdf::FieldValue::text("14.07.2026"));
        assert_eq!(values["S1 Hin Uhrzeit"], crate::pdf::FieldValue::text("08:00"));
        // Abrechnung aus dem importierten Formular des Veranstalters.
        let abrechnung = s.reise_abrechnung_erzeugen("import-reise-1").unwrap();
        let values = crate::pdf::read_values(&s.file_read(&abrechnung).unwrap().1).unwrap();
        assert_eq!(values["Aktenzeichen"], crate::pdf::FieldValue::text("AZ-2026/1234"));
        assert_eq!(values["Fahrkartenpreis"], crate::pdf::FieldValue::text("123,45"));
        // Der Mailentwurf nennt Empfänger und Anhänge.
        let draft = s.mail_entwurf("import-reise-1", crate::reisen::MailKind::Abrechnung).unwrap();
        assert_eq!(draft.to, "lff@example.org");
        assert_eq!(draft.attachments.iter().filter(|a| !a.selected).count(), 1, "die Hotelrechnung ist vorab abgewählt");
    }

    #[test]
    fn importing_again_changes_nothing() {
        let (mut s, _) = service();
        s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap();
        // Eine Änderung in der App darf ein erneuter Import nicht zurücksetzen.
        let edit: Map<String, Value> = json!({"zweck": "In der App geändert"}).as_object().unwrap().clone();
        s.save("reise", Some("import-reise-2"), &edit).unwrap();

        let again = s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap();
        assert_eq!((again.trips_imported, again.trips_skipped, again.attachments_imported, again.attachments_skipped), (0, 2, 0, 2));
        assert_eq!(again.stammdaten, "übersprungen");
        assert_eq!(s.get("import-reise-2").unwrap().fields["zweck"], "In der App geändert");
        assert_eq!(s.list("reise").unwrap().len(), 2);
        assert_eq!(s.list("anhang").unwrap().len(), 2);
    }

    #[test]
    fn existing_personal_data_are_not_overwritten() {
        let (mut s, _) = service();
        let mine: Map<String, Value> = json!({"name": "Schon da", "vorname": "Eigene"}).as_object().unwrap().clone();
        s.save_stammdaten(&mine).unwrap();
        let report = s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap();
        assert_eq!(report.stammdaten, "übersprungen");
        assert_eq!(s.stammdaten().unwrap().fields["name"], "Schon da");
        assert!(!s.smtp_password_is_set(), "auch das Passwort bleibt unberührt");
    }

    #[test]
    fn broken_input_is_reported() {
        let (mut s, _) = service();
        let nothing = temp("bdb-import-leer");
        assert!(matches!(s.import_django(&nothing.join("gibt-es-nicht.sqlite3"), &nothing), Err(ServiceError::Invalid(_))));
        // Eine SQLite-Datei ohne die Tabellen des alten Tools.
        let other = nothing.join("fremd.sqlite3");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE x (id INTEGER)").unwrap();
        let err = s.import_django(&other, &nothing).unwrap_err().to_string();
        assert!(err.contains("keine Datenbank des alten Dienstreisebusters"), "{err}");
        // Keine Datei, die keine Datenbank ist.
        let junk = nothing.join("text.sqlite3");
        fs::write(&junk, b"das ist keine datenbank, nur text").unwrap();
        assert!(s.import_django(&junk, &nothing).is_err());
        assert!(s.list("reise").unwrap().is_empty(), "ein gescheiterter Import legt nichts an");
    }

    #[test]
    fn missing_or_unsafe_files_become_warnings_and_errors_not_a_crash() {
        let (mut s, _) = service();
        let work = temp("bdb-import-kopie");
        fs::copy(fixture().join("db.sqlite3"), work.join("db.sqlite3")).unwrap();
        let conn = Connection::open(work.join("db.sqlite3")).unwrap();
        conn.execute("UPDATE reisen_reise SET antrag_pdf = '../../etc/passwd' WHERE id = 1", []).unwrap();
        conn.execute("UPDATE reisen_reise SET abrechnung_pdf = 'gibt/es/nicht.pdf' WHERE id = 1", []).unwrap();
        conn.execute("UPDATE reisen_anhang SET datei = '/etc/passwd' WHERE id = 1", []).unwrap();
        conn.execute("UPDATE reisen_reise SET reise_ende = 'kaputt' WHERE id = 2", []).unwrap();
        drop(conn);

        let report = s.import_django(&work.join("db.sqlite3"), &fixture().join("media")).unwrap();
        assert_eq!(report.trips_imported, 1, "die Reise mit dem kaputten Zeitpunkt scheitert, die andere kommt an");
        assert_eq!(report.attachments_imported, 1);
        assert!(report.warnings.iter().any(|w| w.contains("ungültiger Dateipfad")), "{report:?}");
        assert!(report.warnings.iter().any(|w| w.contains("fehlt im Medienordner")), "{report:?}");
        assert!(report.errors.iter().any(|e| e.contains("Beratung vor Ort") && e.contains("ungültig")), "{report:?}");
        assert!(report.errors.iter().any(|e| e.starts_with("Anhang 1")), "{report:?}");
        // Die Reise ist trotz fehlender Dateien da, nur ohne diese.
        let winter = s.get("import-reise-1").unwrap().fields;
        assert!(!winter.contains_key("antrag_pdf") && !winter.contains_key("abrechnung_pdf"));
        assert!(winter.contains_key("genehmigung_pdf"));
        assert!(matches!(s.get("import-reise-2"), Err(ServiceError::NotFound)));
    }

    #[test]
    fn locked_app_refuses_the_import() {
        let (mut s, _) = service();
        s.lock();
        assert!(matches!(s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")), Err(ServiceError::Locked)));
    }

    #[test]
    fn files_folder_is_needed_when_there_are_files() {
        let mut s = Service::open(&temp("bdb-import-ohne-ordner"), FAST).unwrap();
        s.create_vault("korrekt pferd batterie heftklammer").unwrap();
        let err = s.import_django(&fixture().join("db.sqlite3"), &fixture().join("media")).unwrap_err().to_string();
        assert!(err.contains("Ordner für PDFs und Anhänge"), "{err}");
        assert!(s.list("reise").unwrap().is_empty(), "vorab geprüft: nichts wurde halb importiert");
        assert!(s.stammdaten().unwrap().fields.is_empty());
    }
}
