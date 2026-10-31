//! Dienstreisemodul: Stammdaten, Dateien im Ordner für PDFs und Anhänge, Antrag und Abrechnung erzeugen,
//! Mail vorbereiten und senden.
//!
//! Portierung der Abläufe aus `dienstreisebuster/reisen/views.py`.
//!
//! Dateien liegen nicht in der verschlüsselten Datenbank, sondern in einem Ordner, den Nextcloud (oder ein
//! anderes Werkzeug) synchronisiert. Die Einträge speichern nur den Pfad relativ zu diesem Ordner. Jeder Zugriff
//! prüft, dass der Pfad im Ordner bleibt.

use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Local;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::dienstreise::{self, Fields};
use crate::mail::{self, Mail, MailAttachment, MailError, Security, SmtpSettings, MAX_ATTACHMENT_BYTES};
use crate::pdf::{self, PdfError};
use crate::schema::{self, STAMMDATEN_ID};
use crate::secrets::Secrets;
use crate::service::{RecordView, Result, Service, ServiceError};

/// Name des Schlüssels für das SMTP-Passwort.
const SMTP_PASSWORD_KEY: &str = "smtp_passwort";

/// Ohne diese Angaben lassen sich Antrag und Abrechnung nicht sinnvoll ausfüllen.
const REQUIRED_PERSONAL_DATA: &[&str] = &["name", "vorname", "plz", "ort", "strasse", "dienststelle_kurz", "personal_nr"];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DraftAttachment {
    pub key: String,
    pub name: String,
    pub size: usize,
    pub selected: bool,
    /// Der Antrag beziehungsweise die Abrechnung selbst ist immer dabei.
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MailDraft {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<DraftAttachment>,
    /// Wann dieses Dokument schon einmal gesendet wurde (`JJJJ-MM-TTTHH:MM`).
    pub already_sent: Option<String>,
}

/// Welches Dokument gesendet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailKind {
    /// Dienstreiseantrag an die Verwaltung.
    Antrag,
    /// Abrechnung an das LfF.
    Abrechnung,
}

impl MailKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "antrag" => Ok(MailKind::Antrag),
            "abrechnung" => Ok(MailKind::Abrechnung),
            other => Err(ServiceError::Invalid(format!("Unbekannte Mailart „{other}“"))),
        }
    }
}

fn mail_err(e: MailError) -> ServiceError {
    ServiceError::Invalid(e.to_string())
}

fn pdf_err(e: PdfError) -> ServiceError {
    ServiceError::Invalid(format!("Das Formular konnte nicht ausgefüllt werden: {e}"))
}

fn io_err(e: std::io::Error) -> ServiceError {
    ServiceError::Settings(format!("Datei: {e}"))
}

fn today_iso() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn now_local() -> String {
    Local::now().format("%Y-%m-%dT%H:%M").to_string()
}

fn date_de(iso: &str) -> String {
    match (iso.get(0..4), iso.get(5..7), iso.get(8..10)) {
        (Some(y), Some(m), Some(d)) => format!("{d}.{m}.{y}"),
        _ => iso.to_string(),
    }
}

/// Dateiname ohne Pfadanteile und Steuerzeichen. Leere Namen werden zu `datei`.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim().to_string();
    let cleaned: String = cleaned.chars().take(120).collect();
    if cleaned.is_empty() {
        "datei".to_string()
    } else {
        cleaned
    }
}

/// Teil eines Ordnernamens aus einem Titel: Kleinbuchstaben, Umlaute umgeschrieben, Rest durch Bindestriche.
fn slug(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let mapped: &str = match c {
            'ä' | 'Ä' => "ae",
            'ö' | 'Ö' => "oe",
            'ü' | 'Ü' => "ue",
            'ß' => "ss",
            _ => "",
        };
        if !mapped.is_empty() {
            out.push_str(mapped);
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed: String = out.trim_matches('-').chars().take(max).collect();
    trimmed.trim_matches('-').to_string()
}

fn text<'a>(fields: &'a Map<String, Value>, name: &str) -> &'a str {
    fields.get(name).and_then(Value::as_str).unwrap_or("")
}

impl Service {
    // ----- Stammdaten ------------------------------------------------------------------------------------

    /// Die Stammdaten (leer, wenn noch nichts eingetragen ist).
    pub fn stammdaten(&mut self) -> Result<RecordView> {
        match self.get(STAMMDATEN_ID) {
            Ok(view) => Ok(view),
            Err(ServiceError::NotFound) => Ok(RecordView { id: STAMMDATEN_ID.into(), kind: "stammdaten".into(), fields: Map::new() }),
            Err(e) => Err(e),
        }
    }

    pub fn save_stammdaten(&mut self, fields: &Map<String, Value>) -> Result<()> {
        self.save_with_id("stammdaten", STAMMDATEN_ID, fields).map(|_| ())
    }

    fn secrets(&self) -> Secrets {
        Secrets::new(&self.dir)
    }

    /// Das SMTP-Passwort bleibt auf diesem Rechner und wird nicht synchronisiert. Ein leerer Wert löscht es.
    pub fn set_smtp_password(&mut self, password: &str) -> Result<()> {
        self.secrets().set(SMTP_PASSWORD_KEY, password).map_err(io_err)
    }

    pub fn smtp_password_is_set(&self) -> bool {
        self.secrets().is_set(SMTP_PASSWORD_KEY)
    }

    fn require_personal_data(&mut self) -> Result<Fields> {
        let fields = self.stammdaten()?.fields;
        let def = schema::kind("stammdaten").expect("Typ stammdaten");
        let missing: Vec<&str> = REQUIRED_PERSONAL_DATA
            .iter()
            .filter(|name| text(&fields, name).trim().is_empty())
            .map(|name| def.fields.iter().find(|f| f.name == *name).map_or(*name, |f| f.label))
            .collect();
        if missing.is_empty() {
            Ok(fields)
        } else {
            Err(ServiceError::Invalid(format!("Bitte zuerst die Stammdaten vervollständigen. Es fehlt: {}.", missing.join(", "))))
        }
    }

    // ----- Dateien ---------------------------------------------------------------------------------------

    fn files_root(&self) -> Result<PathBuf> {
        let dir = self.config.attachments_dir.trim();
        if dir.is_empty() {
            return Err(ServiceError::Settings("Bitte zuerst den Ordner für PDFs und Anhänge in den Einstellungen eintragen.".into()));
        }
        let root = PathBuf::from(dir);
        fs::create_dir_all(&root).map_err(|e| ServiceError::Settings(format!("Der Ordner für PDFs und Anhänge ist nicht verfügbar: {e}")))?;
        Ok(root)
    }

    /// Löst einen relativen Pfad auf und stellt sicher, dass er im Ordner für PDFs und Anhänge bleibt.
    fn resolve_path(&self, relative: &str) -> Result<PathBuf> {
        let root = self.files_root()?;
        let rel = Path::new(relative);
        let safe = !relative.is_empty() && rel.components().all(|c| matches!(c, Component::Normal(_)));
        if !safe {
            return Err(ServiceError::Invalid("Ungültiger Dateipfad".into()));
        }
        let full = root.join(rel);
        // Symbolische Verknüpfungen dürfen nicht aus dem Ordner herausführen.
        if let (Ok(real_root), Ok(real)) = (root.canonicalize(), full.canonicalize()) {
            if !real.starts_with(&real_root) {
                return Err(ServiceError::Invalid("Ungültiger Dateipfad".into()));
            }
        }
        Ok(full)
    }

    /// Ordner einer Reise relativ zum Ordner für PDFs und Anhänge.
    fn trip_dir_relative(&mut self, trip: &RecordView) -> String {
        let start = text(&trip.fields, "reise_beginn");
        let date = start.get(..10).unwrap_or("ohne-datum");
        let title = slug(text(&trip.fields, "zweck"), 40);
        let short_id: String = trip.id.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
        let name = [date, title.as_str(), short_id.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join("-");
        format!("Dienstreisen/{name}")
    }

    /// Schreibt eine Datei in den Ordner der Reise. Bei gleichem Namen wird eine Nummer angehängt. Gibt den
    /// Pfad relativ zum Ordner für PDFs und Anhänge zurück.
    fn write_trip_file(&mut self, trip: &RecordView, file_name: &str, bytes: &[u8]) -> Result<String> {
        let root = self.files_root()?;
        let dir_rel = self.trip_dir_relative(trip);
        let dir = root.join(&dir_rel);
        fs::create_dir_all(&dir).map_err(io_err)?;
        let name = sanitize_file_name(file_name);
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut candidate = name.clone();
        let mut n = 2;
        while dir.join(&candidate).exists() {
            candidate = format!("{stem} ({n}){ext}");
            n += 1;
        }
        fs::write(dir.join(&candidate), bytes).map_err(io_err)?;
        Ok(format!("{dir_rel}/{candidate}"))
    }

    /// Löscht eine Datei im Ordner (fehlende Dateien sind kein Fehler).
    fn remove_file(&self, relative: &str) {
        if let Ok(path) = self.resolve_path(relative) {
            let _ = fs::remove_file(path);
        }
    }

    /// Speichert einen Bericht (Markdown) im Unterordner `Berichte` des Ordners für PDFs und Anhänge. Eine Datei
    /// mit gleichem Namen wird ersetzt, damit ein erneut erzeugter Bericht den alten ablöst. Gibt den relativen
    /// Pfad zurück.
    pub fn bericht_speichern(&mut self, name: &str, content: &str) -> Result<String> {
        if content.trim().is_empty() {
            return Err(ServiceError::Invalid("Der Bericht ist leer.".into()));
        }
        if content.len() > 5 * 1024 * 1024 {
            return Err(ServiceError::Invalid("Der Bericht ist zu groß.".into()));
        }
        let root = self.files_root()?;
        let dir = root.join("Berichte");
        fs::create_dir_all(&dir).map_err(io_err)?;
        // Trennzeichen im Namen (zum Beispiel „Schuljahr 2025/26“) werden zu Bindestrichen, statt den Anfang
        // des Namens zu verlieren.
        let replaced = name.replace(['/', '\\'], "-");
        let mut file = sanitize_file_name(replaced.trim_start_matches(['.', '-', ' ']));
        if !file.to_lowercase().ends_with(".md") {
            file.push_str(".md");
        }
        fs::write(dir.join(&file), content).map_err(io_err)?;
        Ok(format!("Berichte/{file}"))
    }

    pub fn file_read(&self, relative: &str) -> Result<(String, Vec<u8>)> {
        let path = self.resolve_path(relative)?;
        let bytes = fs::read(&path).map_err(|_| ServiceError::NotFound)?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Ok((name, bytes))
    }

    /// Absoluter Pfad einer vorhandenen Datei, zum Öffnen im Dateimanager oder PDF-Programm.
    pub fn file_path(&self, relative: &str) -> Result<String> {
        let path = self.resolve_path(relative)?;
        if !path.is_file() {
            return Err(ServiceError::NotFound);
        }
        Ok(path.to_string_lossy().to_string())
    }

    fn trip(&mut self, id: &str) -> Result<RecordView> {
        let view = self.get(id)?;
        if view.kind != "reise" {
            return Err(ServiceError::NotFound);
        }
        Ok(view)
    }

    fn set_trip_fields(&mut self, id: &str, values: Value) -> Result<()> {
        let map = values.as_object().cloned().unwrap_or_default();
        self.save("reise", Some(id), &map).map(|_| ())
    }

    /// Ersetzt die Datei in einem Feld der Reise: schreibt die neue, merkt sie sich und löscht die alte.
    fn replace_trip_file(&mut self, trip: &RecordView, field: &str, file_name: &str, bytes: &[u8]) -> Result<String> {
        let old = text(&trip.fields, field).to_string();
        let new = self.write_trip_file(trip, file_name, bytes)?;
        self.set_trip_fields(&trip.id, json!({ field: new }))?;
        if !old.is_empty() && old != new {
            self.remove_file(&old);
        }
        Ok(new)
    }

    // ----- Antrag und Abrechnung -------------------------------------------------------------------------

    fn signature_svg(personal: &Fields) -> Option<String> {
        let svg = text(personal, "unterschrift_svg");
        (!svg.trim().is_empty()).then(|| svg.to_string())
    }

    /// Erzeugt den Dienstreiseantrag (R001) mit Unterschrift und legt ihn im Ordner der Reise ab.
    pub fn reise_antrag_erzeugen(&mut self, id: &str) -> Result<String> {
        let personal = self.require_personal_data()?;
        let trip = self.trip(id)?;
        let svg = Self::signature_svg(&personal);
        let bytes = dienstreise::antrag_pdf(&trip.fields, &personal, &today_iso(), svg.as_deref()).map_err(pdf_err)?;
        let start = text(&trip.fields, "reise_beginn").get(..10).unwrap_or("").to_string();
        self.replace_trip_file(&trip, "antrag_pdf", &format!("antrag-{start}.pdf"), &bytes)
    }

    /// Erzeugt die Abrechnung aus dem Formular des Veranstalters und legt sie im Ordner der Reise ab.
    pub fn reise_abrechnung_erzeugen(&mut self, id: &str) -> Result<String> {
        let personal = self.require_personal_data()?;
        let trip = self.trip(id)?;
        let form_path = text(&trip.fields, "veranstalter_formular").to_string();
        if form_path.is_empty() {
            return Err(ServiceError::Invalid(
                "Bitte zuerst das Abrechnungsformular des Veranstalters hochladen. Andere Formulare werden noch nicht unterstützt.".into(),
            ));
        }
        let (_, form) = self.file_read(&form_path)?;
        let svg = Self::signature_svg(&personal);
        let bytes = dienstreise::abrechnung_pdf(&form, &trip.fields, &personal, &today_iso(), svg.as_deref()).map_err(pdf_err)?;
        let start = text(&trip.fields, "reise_beginn").get(..10).unwrap_or("").to_string();
        self.replace_trip_file(&trip, "abrechnung_pdf", &format!("abrechnung-{start}.pdf"), &bytes)
    }

    /// Hinterlegt das vom Veranstalter vorausgefüllte Abrechnungsformular. Es muss ein PDF mit Formularfeldern sein.
    pub fn reise_formular_setzen(&mut self, id: &str, name: &str, bytes: &[u8]) -> Result<String> {
        let trip = self.trip(id)?;
        match pdf::field_names(bytes) {
            Ok(fields) if !fields.is_empty() => {}
            Ok(_) | Err(PdfError::NoForm) => return Err(ServiceError::Invalid("Das ist kein ausfüllbares PDF-Formular.".into())),
            Err(e) => return Err(ServiceError::Invalid(format!("Das PDF lässt sich nicht lesen: {e}"))),
        }
        self.replace_trip_file(&trip, "veranstalter_formular", &format!("veranstalter-{}", sanitize_file_name(name)), bytes)
    }

    /// Hinterlegt die Genehmigung. Aus „Entwurf“ oder „Beantragt“ wird „Genehmigt“.
    pub fn reise_genehmigung_setzen(&mut self, id: &str, name: &str, bytes: &[u8]) -> Result<String> {
        let trip = self.trip(id)?;
        let path = self.replace_trip_file(&trip, "genehmigung_pdf", &format!("genehmigung-{}", sanitize_file_name(name)), bytes)?;
        if matches!(text(&trip.fields, "status"), "Entwurf" | "Beantragt") {
            self.set_trip_fields(id, json!({"status": "Genehmigt"}))?;
        }
        Ok(path)
    }

    // ----- Anhänge ---------------------------------------------------------------------------------------

    pub fn anhang_hinzufuegen(&mut self, reise: &str, art: &str, name: &str, bytes: &[u8], beilegen: Option<bool>) -> Result<String> {
        if bytes.is_empty() {
            return Err(ServiceError::Invalid("Die Datei ist leer.".into()));
        }
        let trip = self.trip(reise)?;
        let file_name = sanitize_file_name(name);
        let path = self.write_trip_file(&trip, &format!("anhang-{file_name}"), bytes)?;
        let mut fields = Map::new();
        fields.insert("reise".into(), json!(reise));
        fields.insert("art".into(), json!(art));
        fields.insert("name".into(), json!(file_name));
        fields.insert("datei".into(), json!(path));
        if let Some(b) = beilegen {
            fields.insert("beilegen".into(), json!(b));
        }
        match self.save("anhang", None, &fields) {
            Ok(id) => Ok(id),
            Err(e) => {
                // Die Datei bleibt nicht ohne Eintrag liegen.
                self.remove_file(&path);
                Err(e)
            }
        }
    }

    pub fn anhang_loeschen(&mut self, id: &str) -> Result<()> {
        let view = self.get(id)?;
        if view.kind != "anhang" {
            return Err(ServiceError::NotFound);
        }
        let file = text(&view.fields, "datei").to_string();
        self.delete(id)?;
        if !file.is_empty() {
            self.remove_file(&file);
        }
        Ok(())
    }

    fn trip_attachments(&mut self, reise: &str) -> Result<Vec<RecordView>> {
        // Anhänge sind versteckt, `list` liefert sie trotzdem nach Typ.
        Ok(self.list("anhang")?.into_iter().filter(|a| a.fields.get("reise").and_then(Value::as_str) == Some(reise)).collect())
    }

    // ----- Mail ------------------------------------------------------------------------------------------

    fn period(trip: &Fields) -> String {
        let start = text(trip, "reise_beginn");
        let end = text(trip, "reise_ende");
        let mut period = date_de(start.get(..10).unwrap_or(""));
        if let (Some(a), Some(b)) = (start.get(..10), end.get(..10)) {
            if a != b {
                period.push_str(&format!(" bis {}", date_de(b)));
            }
        }
        period
    }

    fn mail_text(kind: MailKind, trip: &Fields, personal: &Fields) -> (String, String) {
        let signer = format!("{} {}", text(personal, "vorname"), text(personal, "name")).trim().to_string();
        let period = Self::period(trip);
        let art = text(trip, "art");
        let art = if art.is_empty() { "Dienstreise" } else { art };
        match kind {
            MailKind::Antrag => (
                format!("Antrag auf Genehmigung einer {art} am {period}"),
                format!(
                    "Sehr geehrte Damen und Herren,\n\nanbei übersende ich meinen Antrag auf Genehmigung einer {art} nach {} ({period}).\n\nMit freundlichen Grüßen\n{signer}",
                    text(trip, "geschaeftsort")
                ),
            ),
            MailKind::Abrechnung => (
                format!("Reisekostenabrechnung {} ({period})", text(trip, "zweck")),
                format!(
                    "Sehr geehrte Damen und Herren,\n\nanbei übersende ich meinen Erstattungsantrag für die Reise nach {} ({period}) mit den zugehörigen Unterlagen.\n\nMit freundlichen Grüßen\n{signer}",
                    text(trip, "geschaeftsort")
                ),
            ),
        }
    }

    /// Mögliche Anhänge in dieser Reihenfolge: das Dokument selbst (immer dabei), dann bei der Abrechnung die
    /// Genehmigung und die Anhänge.
    fn candidates(&mut self, kind: MailKind, trip: &RecordView) -> Result<Vec<(DraftAttachment, String)>> {
        let mut out: Vec<(DraftAttachment, String)> = Vec::new();
        let add = |this: &Service, key: &str, label: String, path: &str, selected: bool, required: bool, out: &mut Vec<(DraftAttachment, String)>| -> Result<()> {
            if path.is_empty() {
                return Ok(());
            }
            let (name, bytes) = this.file_read(path)?;
            let shown = if label.is_empty() { name } else { format!("{label}: {name}") };
            out.push((DraftAttachment { key: key.to_string(), name: shown, size: bytes.len(), selected, required }, path.to_string()));
            Ok(())
        };
        match kind {
            MailKind::Antrag => {
                add(self, "antrag", "Dienstreiseantrag".into(), text(&trip.fields, "antrag_pdf"), true, true, &mut out)?;
            }
            MailKind::Abrechnung => {
                add(self, "abrechnung", "Reisekostenabrechnung".into(), text(&trip.fields, "abrechnung_pdf"), true, true, &mut out)?;
                add(self, "genehmigung", "Genehmigung".into(), text(&trip.fields, "genehmigung_pdf"), true, false, &mut out)?;
                for attachment in self.trip_attachments(&trip.id)? {
                    let art = text(&attachment.fields, "art").to_string();
                    let selected = attachment.fields.get("beilegen").and_then(Value::as_bool).unwrap_or(true);
                    let key = format!("anhang-{}", attachment.id);
                    let path = text(&attachment.fields, "datei").to_string();
                    if let Err(e) = add(self, &key, art, &path, selected, false, &mut out) {
                        // Eine fehlende Datei (zum Beispiel noch nicht synchronisiert) blockiert nicht alles.
                        if !matches!(e, ServiceError::NotFound) {
                            return Err(e);
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn recipient_and_sender(personal: &Fields, kind: MailKind) -> (String, String) {
        let to = match kind {
            MailKind::Antrag => text(personal, "empfaenger_antrag"),
            MailKind::Abrechnung => text(personal, "empfaenger_abrechnung"),
        };
        (to.trim().to_string(), text(personal, "absender").trim().to_string())
    }

    /// Vorschlag für die Mail mit Empfänger, Text und möglichen Anhängen.
    pub fn mail_entwurf(&mut self, id: &str, kind: MailKind) -> Result<MailDraft> {
        let trip = self.trip(id)?;
        let personal = self.stammdaten()?.fields;
        let document = match kind {
            MailKind::Antrag => "antrag_pdf",
            MailKind::Abrechnung => "abrechnung_pdf",
        };
        if text(&trip.fields, document).is_empty() {
            return Err(ServiceError::Invalid("Das Dokument wurde noch nicht erzeugt.".into()));
        }
        let (subject, body) = Self::mail_text(kind, &trip.fields, &personal);
        let (to, from) = Self::recipient_and_sender(&personal, kind);
        let attachments = self.candidates(kind, &trip)?.into_iter().map(|(a, _)| a).collect();
        let sent_field = match kind {
            MailKind::Antrag => "antrag_versendet_am",
            MailKind::Abrechnung => "abrechnung_versendet_am",
        };
        let already_sent = Some(text(&trip.fields, sent_field).to_string()).filter(|s| !s.is_empty());
        Ok(MailDraft { to, from, subject, body, attachments, already_sent })
    }

    fn smtp_settings(&self, personal: &Fields) -> SmtpSettings {
        let port = personal.get("smtp_port").and_then(Value::as_f64).filter(|p| (1.0..=65535.0).contains(p)).map_or(587, |p| p as u16);
        SmtpSettings {
            host: text(personal, "smtp_host").trim().to_string(),
            port,
            user: text(personal, "smtp_user").trim().to_string(),
            password: self.secrets().get(SMTP_PASSWORD_KEY).unwrap_or_default(),
            security: Security::parse(text(personal, "smtp_sicherheit")),
        }
    }

    /// Sendet die Mail. Gewählt werden die Anhänge mit den Schlüsseln aus `selected` (das Dokument selbst ist
    /// immer dabei). Nach dem Versand wechselt der Status der Reise und der Zeitpunkt wird festgehalten. Schlägt der
    /// Versand fehl, bleibt alles unverändert.
    pub fn mail_senden(&mut self, id: &str, kind: MailKind, subject: &str, body: &str, selected: &[String]) -> Result<()> {
        let trip = self.trip(id)?;
        let personal = self.stammdaten()?.fields;
        let (to, from) = Self::recipient_and_sender(&personal, kind);
        let smtp = self.smtp_settings(&personal);
        mail::check(&smtp, &from, &to).map_err(mail_err)?;

        let mut attachments = Vec::new();
        let mut total = 0usize;
        for (candidate, path) in self.candidates(kind, &trip)? {
            if candidate.required || selected.contains(&candidate.key) {
                let (name, bytes) = self.file_read(&path)?;
                total += bytes.len();
                attachments.push(MailAttachment { name, bytes });
            }
        }
        if attachments.is_empty() {
            return Err(ServiceError::Invalid("Das Dokument wurde noch nicht erzeugt.".into()));
        }
        if total > MAX_ATTACHMENT_BYTES {
            return Err(mail_err(MailError::TooLarge));
        }
        let subject = subject.trim();
        if subject.is_empty() {
            return Err(ServiceError::Invalid("Der Betreff darf nicht leer sein.".into()));
        }
        mail::send(&smtp, &Mail { from, to, subject: subject.to_string(), body: body.to_string(), attachments }).map_err(mail_err)?;

        let now = now_local();
        let status = text(&trip.fields, "status").to_string();
        let update = match kind {
            MailKind::Antrag => {
                let mut u = json!({"antrag_versendet_am": now});
                if status == "Entwurf" {
                    u["status"] = json!("Beantragt");
                }
                u
            }
            MailKind::Abrechnung => {
                let mut u = json!({"abrechnung_versendet_am": now});
                if matches!(status.as_str(), "Entwurf" | "Beantragt" | "Genehmigt") {
                    u["status"] = json!("Abgerechnet");
                }
                u
            }
        };
        self.set_trip_fields(id, update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;
    use crate::service::Config;
    use crate::test_smtp::FakeSmtp;
    use mail_parser::MimeHeaders;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
    const FORM: &[u8] = include_bytes!("../tests/fixtures/fibs_form.pdf");
    const SVG: &str = include_str!("../tests/fixtures/unterschrift.svg");

    fn temp(prefix: &str) -> PathBuf {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("{prefix}-{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    struct Env {
        service: Service,
        files: PathBuf,
        smtp: FakeSmtp,
    }

    /// Entsperrter Dienst mit Ordner für Dateien, Test-SMTP-Server und vollständigen Stammdaten.
    fn env() -> Env {
        let mut service = Service::open(&temp("bdb-reisen"), FAST).unwrap();
        service.create_vault("korrekt pferd batterie heftklammer").unwrap();
        let files = temp("bdb-dateien");
        service
            .set_config(Config { server_url: String::new(), token: String::new(), attachments_dir: files.to_string_lossy().to_string() })
            .unwrap();
        let smtp = FakeSmtp::start(Some(("max", "geheim")));
        service
            .save_stammdaten(&map(json!({
                "name": "Mustermann", "vorname": "Max", "plz": "89073", "ort": "Ulm", "strasse": "Hauptstraße 1",
                "dienststelle_kurz": "Musterschule Ulm", "personal_nr": "9876543", "iban": "DE89 3704 0044 0532 0130 00",
                "bic": "COBADEFFXXX", "unterschrift_ort": "Ulm", "unterschrift_svg": SVG,
                "empfaenger_antrag": "verwaltung@example.org", "empfaenger_abrechnung": "lff@example.org",
                "absender": "max.mustermann@example.org", "smtp_host": "127.0.0.1", "smtp_port": smtp.port,
                "smtp_user": "max", "smtp_sicherheit": "keine",
            })))
            .unwrap();
        service.set_smtp_password("geheim").unwrap();
        Env { service, files, smtp }
    }

    fn new_trip(s: &mut Service) -> String {
        s.save(
            "reise",
            None,
            &map(json!({
                "zweck": "Beratung Medienkonzept", "geschaeftsort": "Dillingen a. d. Donau",
                "reise_beginn": "2026-11-16T06:45", "dienstgeschaeft_beginn": "2026-11-16T10:00",
                "dienstgeschaeft_ende": "2026-11-17T15:30", "reise_ende": "2026-11-17T18:20",
                "bahn": true, "bahncard": true, "fahrkartenpreis": 123.45,
            })),
        )
        .unwrap()
    }

    #[test]
    fn trip_defaults_and_time_order() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        let trip = e.service.get(&id).unwrap();
        assert_eq!((trip.fields["status"].as_str(), trip.fields["art"].as_str()), (Some("Entwurf"), Some("Dienstreise")));
        assert_eq!(trip.fields["abfahrt_von"], "Wohnort");

        // Reihenfolge der Zeiten.
        let bad = e.service.save("reise", Some(&id), &map(json!({"reise_ende": "2026-11-16T05:00"})));
        assert!(matches!(&bad, Err(ServiceError::Invalid(m)) if m.contains("darf nicht nach")), "{bad:?}");
        let bad = e.service.save("reise", None, &map(json!({"zweck": "X", "geschaeftsort": "Y", "reise_beginn": "2026-11-16T12:00",
            "dienstgeschaeft_beginn": "2026-11-16T10:00", "dienstgeschaeft_ende": "2026-11-16T15:00", "reise_ende": "2026-11-16T16:00"})));
        assert!(matches!(bad, Err(ServiceError::Invalid(_))));
        assert_eq!(e.service.get(&id).unwrap().fields["reise_ende"], "2026-11-17T18:20", "abgelehnte Änderung wird nicht gespeichert");
        // Gleiche Zeitpunkte sind erlaubt.
        e.service.save("reise", Some(&id), &map(json!({"dienstgeschaeft_ende": "2026-11-17T18:20"}))).unwrap();
    }

    #[test]
    fn hidden_kinds_stay_out_of_lists_and_graph() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        e.service.anhang_hinzufuegen(&id, "Fahrkarte", "ticket.pdf", b"%PDF-1.4 x", None).unwrap();
        let all = e.service.list_all().unwrap();
        assert!(all.iter().all(|r| r.kind != "anhang" && r.kind != "stammdaten"), "{all:?}");
        assert!(all.iter().any(|r| r.kind == "reise"));
    }

    #[test]
    fn personal_data_are_a_singleton_and_checked_before_generating() {
        let mut e = env();
        assert_eq!(e.service.stammdaten().unwrap().fields["name"], "Mustermann");
        e.service.save_stammdaten(&map(json!({"name": "Musterfrau"}))).unwrap();
        assert_eq!(e.service.stammdaten().unwrap().fields["name"], "Musterfrau");
        assert_eq!(e.service.stammdaten().unwrap().fields["vorname"], "Max", "andere Felder bleiben");

        let mut empty = Service::open(&temp("bdb-leer"), FAST).unwrap();
        empty.create_vault("korrekt pferd batterie heftklammer").unwrap();
        assert!(empty.stammdaten().unwrap().fields.is_empty());
        let id = new_trip(&mut empty);
        let err = empty.reise_antrag_erzeugen(&id).unwrap_err().to_string();
        assert!(err.contains("Bitte zuerst die Stammdaten vervollständigen") && err.contains("Name") && err.contains("Personalnummer"), "{err}");
    }

    #[test]
    fn generates_the_application_and_stores_it_in_the_files_folder() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        let rel = e.service.reise_antrag_erzeugen(&id).unwrap();
        assert!(rel.starts_with("Dienstreisen/2026-11-16-beratung-medienkonzept-"), "{rel}");
        assert!(rel.ends_with("/antrag-2026-11-16.pdf"), "{rel}");
        let (name, bytes) = e.service.file_read(&rel).unwrap();
        assert_eq!(name, "antrag-2026-11-16.pdf");
        let values = pdf::read_values(&bytes).unwrap();
        assert_eq!(values["S1 Name"], pdf::FieldValue::text("Mustermann"));
        assert_eq!(values["S1 Zweck"], pdf::FieldValue::text("Beratung Medienkonzept"));
        assert_eq!(values["S1 Antrag"], pdf::FieldValue::State("Dienstreise".into()));
        assert_eq!(e.service.get(&id).unwrap().fields["antrag_pdf"], rel.as_str());
        assert!(e.files.join(&rel).is_file());

        // Erneut erzeugen ersetzt die Datei, es bleibt genau eine.
        let again = e.service.reise_antrag_erzeugen(&id).unwrap();
        let dir = e.files.join(&again).parent().unwrap().to_path_buf();
        assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
    }

    #[test]
    fn files_folder_must_be_configured() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        e.service.set_config(Config::default()).unwrap();
        let err = e.service.reise_antrag_erzeugen(&id).unwrap_err().to_string();
        assert!(err.contains("Ordner für PDFs und Anhänge"), "{err}");
    }

    #[test]
    fn organizer_form_must_be_a_fillable_pdf() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        for bad in [&b"kein pdf"[..], &b""[..], crate::dienstreise::ANTRAG_VORLAGE.get(..100).unwrap()] {
            assert!(matches!(e.service.reise_formular_setzen(&id, "x.pdf", bad), Err(ServiceError::Invalid(_))));
        }
        // Ohne Formular lässt sich keine Abrechnung erzeugen.
        let err = e.service.reise_abrechnung_erzeugen(&id).unwrap_err().to_string();
        assert!(err.contains("Abrechnungsformular des Veranstalters"), "{err}");

        e.service.reise_formular_setzen(&id, "FIBS Abrechnung.pdf", FORM).unwrap();
        let rel = e.service.reise_abrechnung_erzeugen(&id).unwrap();
        assert!(rel.ends_with("/abrechnung-2026-11-16.pdf"));
        let (_, bytes) = e.service.file_read(&rel).unwrap();
        let values = pdf::read_values(&bytes).unwrap();
        assert_eq!(values["Aktenzeichen"], pdf::FieldValue::text("AZ-2026/1234"), "Vorgaben des Veranstalters bleiben");
        assert_eq!(values["Fahrkartenpreis"], pdf::FieldValue::text("123,45"));
        assert_eq!(values["IBAN_1"], pdf::FieldValue::text("DE89"));
        // Die Datei des Veranstalters bleibt unverändert erhalten.
        let form_path = text(&e.service.get(&id).unwrap().fields, "veranstalter_formular").to_string();
        let (_, original) = e.service.file_read(&form_path).unwrap();
        assert_eq!(original, FORM);
    }

    #[test]
    fn approval_moves_the_status_forward_but_not_back() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        e.service.reise_genehmigung_setzen(&id, "Genehmigung.pdf", b"%PDF genehmigt").unwrap();
        assert_eq!(e.service.get(&id).unwrap().fields["status"], "Genehmigt");
        e.service.save("reise", Some(&id), &map(json!({"status": "Abgerechnet"}))).unwrap();
        e.service.reise_genehmigung_setzen(&id, "Genehmigung neu.pdf", b"%PDF neu").unwrap();
        assert_eq!(e.service.get(&id).unwrap().fields["status"], "Abgerechnet", "ein späterer Status bleibt");
        let files: Vec<_> = fs::read_dir(e.files.join(e.service.get(&id).unwrap().fields["genehmigung_pdf"].as_str().unwrap()).parent().unwrap()).unwrap().collect();
        assert_eq!(files.len(), 1, "die alte Genehmigung wird ersetzt");
    }

    #[test]
    fn attachments_are_stored_listed_and_deleted_with_their_files() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        let a = e.service.anhang_hinzufuegen(&id, "Fahrkarte", "../../ticket.pdf", b"%PDF ticket", None).unwrap();
        let b = e.service.anhang_hinzufuegen(&id, "Hotelrechnung", "ticket.pdf", b"%PDF hotel", Some(false)).unwrap();
        let first = e.service.get(&a).unwrap();
        assert_eq!(first.fields["name"], "ticket.pdf", "Pfadanteile entfallen");
        assert_eq!(first.fields["beilegen"], true, "Vorgabe: beilegen");
        assert_eq!(e.service.get(&b).unwrap().fields["beilegen"], false);
        // Gleicher Name: eigene Datei mit Nummer.
        let (p1, p2) = (text(&first.fields, "datei").to_string(), text(&e.service.get(&b).unwrap().fields, "datei").to_string());
        assert_ne!(p1, p2);
        assert!(p2.contains("(2)"), "{p2}");
        assert!(e.files.join(&p1).starts_with(&e.files) && e.files.join(&p1).is_file());

        assert!(matches!(e.service.anhang_hinzufuegen(&id, "Fahrkarte", "leer.pdf", b"", None), Err(ServiceError::Invalid(_))));
        assert!(matches!(e.service.anhang_hinzufuegen(&id, "Unsinn", "x.pdf", b"x", None), Err(ServiceError::Invalid(_))));
        assert_eq!(fs::read_dir(e.files.join(&p1).parent().unwrap()).unwrap().count(), 2, "ein Fehler hinterlässt keine verwaiste Datei");

        e.service.anhang_loeschen(&a).unwrap();
        assert!(!e.files.join(&p1).exists());
        assert!(matches!(e.service.get(&a), Err(ServiceError::NotFound)));
        assert!(matches!(e.service.anhang_loeschen(&id), Err(ServiceError::NotFound)), "nur Anhänge lassen sich so löschen");
    }

    #[test]
    fn file_paths_cannot_leave_the_folder() {
        let mut e = env();
        let outside = temp("bdb-aussen");
        fs::write(outside.join("geheim.txt"), b"geheim").unwrap();
        for bad in ["../geheim.txt", "/etc/passwd", "Dienstreisen/../../x", "", ".", "a/../../b"] {
            assert!(matches!(e.service.file_read(bad), Err(ServiceError::Invalid(_))), "{bad}");
            assert!(e.service.file_path(bad).is_err(), "{bad}");
        }
        assert!(matches!(e.service.file_read("gibt/es/nicht.pdf"), Err(ServiceError::NotFound)));
        #[cfg(unix)]
        {
            // Eine Verknüpfung, die aus dem Ordner herausführt, wird abgelehnt.
            std::os::unix::fs::symlink(&outside, e.files.join("link")).unwrap();
            assert!(matches!(e.service.file_read("link/geheim.txt"), Err(ServiceError::Invalid(_))));
        }
        let id = new_trip(&mut e.service);
        let rel = e.service.reise_antrag_erzeugen(&id).unwrap();
        assert!(e.service.file_path(&rel).unwrap().ends_with(".pdf"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("C:\\Users\\x\\Rechnung.pdf"), "Rechnung.pdf");
        assert_eq!(sanitize_file_name("  .versteckt  "), "versteckt");
        assert_eq!(sanitize_file_name("a:b*c?.pdf"), "a_b_c_.pdf");
        assert_eq!(sanitize_file_name(""), "datei");
        assert_eq!(sanitize_file_name("..."), "datei");
        assert_eq!(sanitize_file_name("Größe äöü.pdf"), "Größe äöü.pdf");
        assert_eq!(sanitize_file_name(&"x".repeat(300)).chars().count(), 120);
        assert_eq!(slug("Beratung: Müller & Söhne, Größe!", 40), "beratung-mueller-soehne-groesse");
        assert_eq!(slug("", 10), "");
        assert_eq!(slug("abcdefghij", 5), "abcde");
    }

    #[test]
    fn mail_draft_follows_the_python_wording() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        assert!(matches!(e.service.mail_entwurf(&id, MailKind::Antrag), Err(ServiceError::Invalid(_))), "ohne Dokument kein Entwurf");
        e.service.reise_antrag_erzeugen(&id).unwrap();
        let draft = e.service.mail_entwurf(&id, MailKind::Antrag).unwrap();
        assert_eq!(draft.to, "verwaltung@example.org");
        assert_eq!(draft.from, "max.mustermann@example.org");
        assert_eq!(draft.subject, "Antrag auf Genehmigung einer Dienstreise am 16.11.2026 bis 17.11.2026");
        assert!(draft.body.contains("nach Dillingen a. d. Donau (16.11.2026 bis 17.11.2026)"), "{}", draft.body);
        assert!(draft.body.ends_with("Mit freundlichen Grüßen\nMax Mustermann"));
        assert_eq!(draft.attachments.len(), 1);
        assert!(draft.attachments[0].required && draft.attachments[0].selected);
        assert_eq!(draft.already_sent, None);

        // Eintägig: ohne „bis“.
        let day = e.service.save("reise", None, &map(json!({"zweck": "Tag", "geschaeftsort": "Ulm", "art": "Fortbildungsreise",
            "reise_beginn": "2026-12-01T08:00", "dienstgeschaeft_beginn": "2026-12-01T09:00", "dienstgeschaeft_ende": "2026-12-01T16:00", "reise_ende": "2026-12-01T17:00"}))).unwrap();
        e.service.reise_antrag_erzeugen(&day).unwrap();
        let draft = e.service.mail_entwurf(&day, MailKind::Antrag).unwrap();
        assert_eq!(draft.subject, "Antrag auf Genehmigung einer Fortbildungsreise am 01.12.2026");
    }

    #[test]
    fn application_mail_changes_status_and_delivers_the_pdf() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        let rel = e.service.reise_antrag_erzeugen(&id).unwrap();
        let draft = e.service.mail_entwurf(&id, MailKind::Antrag).unwrap();

        e.service.mail_senden(&id, MailKind::Antrag, &draft.subject, &draft.body, &[]).unwrap();

        let trip = e.service.get(&id).unwrap();
        assert_eq!(trip.fields["status"], "Beantragt");
        assert!(trip.fields["antrag_versendet_am"].as_str().unwrap().starts_with("20"));
        assert_eq!(e.service.mail_entwurf(&id, MailKind::Antrag).unwrap().already_sent, trip.fields["antrag_versendet_am"].as_str().map(str::to_string));

        let got = e.smtp.received.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].to, ["verwaltung@example.org"]);
        assert_eq!(got[0].authenticated_as.as_deref(), Some("max"));
        let parsed = mail_parser::MessageParser::default().parse(&got[0].data).unwrap();
        assert_eq!(parsed.subject(), Some(draft.subject.as_str()));
        let attachment = parsed.attachments().next().unwrap();
        assert_eq!(attachment.attachment_name(), Some("antrag-2026-11-16.pdf"));
        assert_eq!(attachment.contents(), fs::read(e.files.join(&rel)).unwrap().as_slice());
    }

    #[test]
    fn settlement_mail_sends_selected_attachments_and_moves_to_settled() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        e.service.reise_formular_setzen(&id, "form.pdf", FORM).unwrap();
        e.service.reise_abrechnung_erzeugen(&id).unwrap();
        e.service.reise_genehmigung_setzen(&id, "genehmigung.pdf", b"%PDF genehmigt").unwrap();
        let ticket = e.service.anhang_hinzufuegen(&id, "Fahrkarte", "ticket.pdf", b"%PDF ticket", None).unwrap();
        e.service.anhang_hinzufuegen(&id, "Hotelrechnung", "hotel.pdf", b"%PDF hotel", Some(false)).unwrap();

        let draft = e.service.mail_entwurf(&id, MailKind::Abrechnung).unwrap();
        assert_eq!(draft.to, "lff@example.org");
        assert_eq!(draft.subject, "Reisekostenabrechnung Beratung Medienkonzept (16.11.2026 bis 17.11.2026)");
        let keys: Vec<(&str, bool, bool)> = draft.attachments.iter().map(|a| (a.key.as_str(), a.selected, a.required)).collect();
        assert_eq!(keys[0], ("abrechnung", true, true));
        assert_eq!(keys[1], ("genehmigung", true, false));
        assert!(keys.contains(&(format!("anhang-{ticket}").as_str(), true, false)), "{keys:?}");
        assert_eq!(draft.attachments.iter().filter(|a| !a.selected).count(), 1, "„nicht beilegen“ ist vorab abgewählt");

        // Gesendet wird, was gewählt ist: Abrechnung (immer), Genehmigung und das Ticket.
        let selected: Vec<String> = draft.attachments.iter().filter(|a| a.selected).map(|a| a.key.clone()).collect();
        e.service.mail_senden(&id, MailKind::Abrechnung, &draft.subject, &draft.body, &selected).unwrap();
        assert_eq!(e.service.get(&id).unwrap().fields["status"], "Abgerechnet");
        let got = e.smtp.received.lock().unwrap().clone();
        let parsed = mail_parser::MessageParser::default().parse(&got[0].data).unwrap();
        let names: Vec<String> = parsed.attachments().map(|a| a.attachment_name().unwrap().to_string()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names[0].starts_with("abrechnung-") && names.iter().any(|n| n.contains("genehmigung")) && names.iter().any(|n| n.contains("ticket")));
        assert!(!names.iter().any(|n| n.contains("hotel")));

        // Auch ohne Auswahl ist die Abrechnung selbst dabei.
        e.service.mail_senden(&id, MailKind::Abrechnung, "Nochmal", "Text", &[]).unwrap();
        let second = mail_parser::MessageParser::default().parse(&e.smtp.received.lock().unwrap()[1].data).unwrap().attachments().count();
        assert_eq!(second, 1);
    }

    #[test]
    fn failed_sending_changes_nothing() {
        let mut e = env();
        let id = new_trip(&mut e.service);
        e.service.reise_antrag_erzeugen(&id).unwrap();

        // Falsches Passwort.
        e.service.set_smtp_password("falsch").unwrap();
        let err = e.service.mail_senden(&id, MailKind::Antrag, "Betreff", "Text", &[]).unwrap_err().to_string();
        assert!(err.starts_with("Versand fehlgeschlagen"), "{err}");
        let trip = e.service.get(&id).unwrap();
        assert_eq!(trip.fields["status"], "Entwurf");
        assert!(!trip.fields.contains_key("antrag_versendet_am"));
        assert!(e.smtp.received.lock().unwrap().is_empty());

        // Fehlende Einstellungen und leerer Betreff.
        e.service.set_smtp_password("geheim").unwrap();
        assert!(e.service.mail_senden(&id, MailKind::Antrag, "  ", "Text", &[]).unwrap_err().to_string().contains("Betreff"));
        e.service.save_stammdaten(&map(json!({"empfaenger_antrag": ""}))).unwrap();
        let err = e.service.mail_senden(&id, MailKind::Antrag, "Betreff", "Text", &[]).unwrap_err().to_string();
        assert_eq!(err, "In den Einstellungen fehlt: Empfänger.");
        assert!(e.smtp.received.lock().unwrap().is_empty());
    }

    #[test]
    fn smtp_password_is_not_part_of_the_synced_data() {
        let mut e = env();
        assert!(e.service.smtp_password_is_set());
        let dump = serde_json::to_string(&e.service.stammdaten().unwrap().fields).unwrap();
        assert!(!dump.contains("geheim"));
        e.service.set_smtp_password("").unwrap();
        assert!(!e.service.smtp_password_is_set());
    }

    #[test]
    fn reports_are_saved_in_the_reports_folder_and_replaced() {
        let mut e = env();
        let path = e.service.bericht_speichern("Tätigkeitsbericht Schuljahr 2025/26", "# Bericht\n\n- Anzahl: 3\n").unwrap();
        assert_eq!(path, "Berichte/Tätigkeitsbericht Schuljahr 2025-26.md", "Schrägstriche im Namen werden zu Bindestrichen");
        assert_eq!(fs::read_to_string(e.files.join(&path)).unwrap(), "# Bericht\n\n- Anzahl: 3\n");
        // Gleicher Name ersetzt die Datei, die Endung wird nicht verdoppelt.
        let again = e.service.bericht_speichern("Tätigkeitsbericht Schuljahr 2025/26.md", "# Neu\n").unwrap();
        assert_eq!(again, path);
        assert_eq!(fs::read_to_string(e.files.join(&path)).unwrap(), "# Neu\n");
        assert_eq!(fs::read_dir(e.files.join("Berichte")).unwrap().count(), 1);
        // Pfadanteile und leere Berichte.
        assert_eq!(e.service.bericht_speichern("../../böse", "x").unwrap(), "Berichte/böse.md");
        assert!(matches!(e.service.bericht_speichern("leer", "  \n"), Err(ServiceError::Invalid(_))));
        assert!(e.service.file_read(&path).is_ok(), "gespeicherte Berichte lassen sich über file_read öffnen");
        e.service.set_config(Config::default()).unwrap();
        assert!(e.service.bericht_speichern("x", "y").unwrap_err().to_string().contains("Ordner für PDFs und Anhänge"));
    }

    #[test]
    fn mail_kind_parsing() {
        assert_eq!(MailKind::parse("antrag").unwrap(), MailKind::Antrag);
        assert_eq!(MailKind::parse("abrechnung").unwrap(), MailKind::Abrechnung);
        assert!(MailKind::parse("sonst").is_err());
    }
}
