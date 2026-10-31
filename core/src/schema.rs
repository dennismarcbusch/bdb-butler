//! Typen und Felder der Einträge. Die Definitionen gelten für Oberfläche (Formulare), MCP-Server
//! (Werkzeugbeschreibungen) und Prüfung beim Speichern gleichermaßen.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldType {
    /// Einzeiliger Text.
    Text,
    /// Mehrzeiliger Text.
    LongText,
    /// Mehrzeiliger Text im Markdown-Format (mit Wikilinks `[[Titel]]`).
    Markdown,
    /// Datum `JJJJ-MM-TT`.
    Date,
    /// Datum und Uhrzeit `JJJJ-MM-TTTHH:MM`.
    DateTime,
    Number,
    Bool,
    Choice { options: &'static [&'static str] },
    /// Verweis auf einen Eintrag eines Typs (ID).
    Ref { kind: &'static str },
    /// Verweise auf mehrere Einträge eines Typs (Liste von IDs).
    RefList { kind: &'static str },
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    pub name: &'static str,
    pub label: &'static str,
    #[serde(flatten)]
    pub field_type: FieldType,
    pub required: bool,
    /// Vorgabewert (nur für Auswahlfelder): gilt beim Anlegen, wenn das Feld fehlt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KindDef {
    pub kind: &'static str,
    pub label: &'static str,
    pub label_plural: &'static str,
    /// Feld, das in Listen als Titel erscheint.
    pub title_field: &'static str,
    /// Feld, nach dem Listen sortiert werden (Texte und Daten sortieren alphabetisch, `YYYY-MM-DD` auch zeitlich).
    pub sort_field: &'static str,
    /// Absteigend sortieren (neueste zuerst).
    pub sort_desc: bool,
    /// Typen mit Personenbezug sind für die KI standardmäßig gesperrt.
    pub personal: bool,
    pub fields: Vec<FieldDef>,
}

const fn text(name: &'static str, label: &'static str, required: bool) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Text, required, default: None }
}

const fn long_text(name: &'static str, label: &'static str) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::LongText, required: false, default: None }
}

const fn markdown(name: &'static str, label: &'static str) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Markdown, required: false, default: None }
}

const fn date(name: &'static str, label: &'static str, required: bool) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Date, required, default: None }
}

const fn number(name: &'static str, label: &'static str) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Number, required: false, default: None }
}

const fn choice(name: &'static str, label: &'static str, options: &'static [&'static str]) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Choice { options }, required: false, default: None }
}

const fn reference(name: &'static str, label: &'static str, kind: &'static str) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::Ref { kind }, required: false, default: None }
}

const fn reference_list(name: &'static str, label: &'static str, kind: &'static str) -> FieldDef {
    FieldDef { name, label, field_type: FieldType::RefList { kind }, required: false, default: None }
}

const fn required(mut def: FieldDef) -> FieldDef {
    def.required = true;
    def
}

const fn with_default(mut def: FieldDef, value: &'static str) -> FieldDef {
    def.default = Some(value);
    def
}

pub const SCHULARTEN: &[&str] = &[
    "Grundschule",
    "Mittelschule",
    "Realschule",
    "Gymnasium",
    "Wirtschaftsschule",
    "Förderzentrum",
    "Berufsschule",
    "Fach- und Berufsoberschule",
    "Sonstige",
];

pub const BERATUNGSARTEN: &[&str] = &["Vor Ort", "Online", "Telefon", "Schriftlich"];

pub const PROTOKOLLARTEN: &[&str] = &["Beratung", "Jour fixe", "Dienstbesprechung", "Sonstiges"];

pub const AUFTRAGGEBER: &[&str] = &["Ministerium", "Sachgebietsleitung", "Schulleitung", "Sonstige"];

/// Zustände eines Auftrags. „Erledigt“ ist der einzige abgeschlossene Zustand.
pub const AUFTRAGSSTATUS: &[&str] = &["Offen", "In Arbeit", "Wartet", "Erledigt"];

pub const ROLLEN: &[&str] = &[
    "Schulleitung",
    "Stellvertretung",
    "Digitalbeauftragte/r",
    "Systembetreuung",
    "Lehrkraft",
    "Sekretariat",
    "Sachaufwandsträger",
    "Sonstige",
];

/// Alle bekannten Typen. Neue Module ergänzen ihre Typen hier.
pub fn kinds() -> Vec<KindDef> {
    vec![
        KindDef {
            kind: "schule",
            label: "Schule",
            label_plural: "Schulen",
            title_field: "name",
            sort_field: "name",
            sort_desc: false,
            personal: false,
            fields: vec![
                text("name", "Name", true),
                choice("schulart", "Schulart", SCHULARTEN),
                text("schulnummer", "Schulnummer", false),
                text("strasse", "Straße", false),
                text("plz", "PLZ", false),
                text("ort", "Ort", false),
                text("telefon", "Telefon", false),
                text("email", "E-Mail", false),
                text("website", "Webseite", false),
                long_text("notizen", "Notizen"),
            ],
        },
        KindDef {
            kind: "ansprechpartner",
            label: "Ansprechpartner",
            label_plural: "Ansprechpartner",
            title_field: "name",
            sort_field: "name",
            sort_desc: false,
            personal: true,
            fields: vec![
                text("name", "Name", true),
                choice("rolle", "Rolle", ROLLEN),
                reference("schule", "Schule", "schule"),
                text("email", "E-Mail", false),
                text("telefon", "Telefon", false),
                reference_list("verknuepfungen", "Verknüpft mit", "ansprechpartner"),
                long_text("notizen", "Notizen"),
            ],
        },
        KindDef {
            kind: "beratene_person",
            label: "Beratene Person",
            label_plural: "Beratene Personen",
            title_field: "name",
            sort_field: "name",
            sort_desc: false,
            personal: true,
            fields: vec![
                text("name", "Name", true),
                text("funktion", "Funktion", false),
                reference("schule", "Schule", "schule"),
                text("email", "E-Mail", false),
                text("telefon", "Telefon", false),
                long_text("notizen", "Notizen"),
            ],
        },
        KindDef {
            kind: "beratung",
            label: "Beratung",
            label_plural: "Beratungen",
            title_field: "thema",
            sort_field: "datum",
            sort_desc: true,
            personal: false,
            fields: vec![
                text("thema", "Thema", true),
                date("datum", "Datum", true),
                choice("art", "Art", BERATUNGSARTEN),
                number("dauer", "Dauer in Minuten"),
                reference("schule", "Schule", "schule"),
                reference_list("personen", "Beratene Personen", "beratene_person"),
                markdown("inhalt", "Inhalt und Ergebnis"),
            ],
        },
        KindDef {
            kind: "protokoll",
            label: "Protokoll",
            label_plural: "Protokolle",
            title_field: "titel",
            sort_field: "datum",
            sort_desc: true,
            // Protokolle nennen oft Personen. Deshalb sind sie für die KI zunächst gesperrt.
            personal: true,
            fields: vec![
                text("titel", "Titel", true),
                required(choice("art", "Art", PROTOKOLLARTEN)),
                date("datum", "Datum", true),
                text("teilnehmer", "Teilnehmer", false),
                reference("schule", "Schule", "schule"),
                reference("beratung", "Beratung", "beratung"),
                markdown("text", "Protokoll (Markdown)"),
            ],
        },
        KindDef {
            kind: "auftrag",
            label: "Auftrag",
            label_plural: "Auftragsbuch",
            title_field: "titel",
            sort_field: "frist",
            sort_desc: false,
            personal: false,
            fields: vec![
                text("titel", "Titel", true),
                choice("auftraggeber", "Auftraggeber", AUFTRAGGEBER),
                required(with_default(choice("status", "Status", AUFTRAGSSTATUS), "Offen")),
                date("erteilt_am", "Erteilt am", false),
                date("frist", "Frist", false),
                date("erledigt_am", "Erledigt am", false),
                reference("schule", "Schule", "schule"),
                reference("protokoll", "Aus Protokoll", "protokoll"),
                markdown("beschreibung", "Beschreibung"),
            ],
        },
    ]
}

pub fn kind(name: &str) -> Option<KindDef> {
    kinds().into_iter().find(|k| k.kind == name)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValueError {
    #[error("{0}")]
    Invalid(String),
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, ValueError> {
    Err(ValueError::Invalid(msg.into()))
}

/// Maximale Länge einzeiliger Texte und mehrzeiliger Texte (in Zeichen).
const MAX_TEXT: usize = 2_000;
const MAX_LONG_TEXT: usize = 1_000_000;
const MAX_REFS: usize = 500;

/// Prüft einen Wert gegen die Felddefinition und gibt ihn normalisiert zurück. Leere Texte und `null` löschen
/// das Feld (Ergebnis `Value::Null`). Verweise werden hier nur auf die Form geprüft, ob das Ziel existiert,
/// prüft der Dienst.
pub fn normalize(def: &FieldDef, value: &Value) -> Result<Value, ValueError> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let label = def.label;
    match &def.field_type {
        FieldType::Text | FieldType::LongText | FieldType::Markdown | FieldType::Date | FieldType::DateTime | FieldType::Choice { .. } | FieldType::Ref { .. } => {
            let Some(text) = value.as_str() else {
                return invalid(format!("{label}: Text erwartet"));
            };
            let text = text.trim();
            if text.is_empty() {
                return Ok(Value::Null);
            }
            match &def.field_type {
                FieldType::Text => check_len(label, text, MAX_TEXT)?,
                FieldType::LongText | FieldType::Markdown => check_len(label, text, MAX_LONG_TEXT)?,
                FieldType::Date => check_date(label, text)?,
                FieldType::DateTime => check_datetime(label, text)?,
                FieldType::Choice { options } => {
                    if !options.contains(&text) {
                        return invalid(format!("{label}: „{text}“ ist keine gültige Auswahl"));
                    }
                }
                FieldType::Ref { .. } => check_id(label, text)?,
                _ => unreachable!(),
            }
            Ok(Value::String(text.to_string()))
        }
        FieldType::Number => {
            if value.is_number() {
                Ok(value.clone())
            } else {
                invalid(format!("{label}: Zahl erwartet"))
            }
        }
        FieldType::Bool => {
            if value.is_boolean() {
                Ok(value.clone())
            } else {
                invalid(format!("{label}: Ja/Nein erwartet"))
            }
        }
        FieldType::RefList { .. } => {
            let Some(items) = value.as_array() else {
                return invalid(format!("{label}: Liste erwartet"));
            };
            if items.len() > MAX_REFS {
                return invalid(format!("{label}: zu viele Einträge"));
            }
            let mut out: Vec<String> = Vec::new();
            for item in items {
                let Some(id) = item.as_str() else {
                    return invalid(format!("{label}: Liste von IDs erwartet"));
                };
                check_id(label, id)?;
                if !out.iter().any(|x| x == id) {
                    out.push(id.to_string());
                }
            }
            if out.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(Value::from(out))
            }
        }
    }
}

fn check_len(label: &str, text: &str, max: usize) -> Result<(), ValueError> {
    if text.chars().count() > max {
        invalid(format!("{label}: zu lang (höchstens {max} Zeichen)"))
    } else {
        Ok(())
    }
}

fn check_id(label: &str, id: &str) -> Result<(), ValueError> {
    let ok = !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if ok {
        Ok(())
    } else {
        invalid(format!("{label}: ungültiger Verweis"))
    }
}

fn digits(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        s.parse().ok()
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => 29,
        _ => 28,
    }
}

fn valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts.as_slice() else { return false };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return false;
    }
    let (Some(y), Some(m), Some(d)) = (digits(y), digits(m), digits(d)) else { return false };
    (1900..=2200).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)
}

fn valid_time(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m) = match parts.as_slice() {
        [h, m] => (h, m),
        [h, m, sec] if sec.len() == 2 && digits(sec).is_some_and(|v| v < 60) => (h, m),
        _ => return false,
    };
    h.len() == 2 && m.len() == 2 && digits(h).is_some_and(|v| v < 24) && digits(m).is_some_and(|v| v < 60)
}

fn check_date(label: &str, text: &str) -> Result<(), ValueError> {
    if valid_date(text) {
        Ok(())
    } else {
        invalid(format!("{label}: Datum im Format JJJJ-MM-TT erwartet"))
    }
}

fn check_datetime(label: &str, text: &str) -> Result<(), ValueError> {
    match text.split_once('T') {
        Some((date, time)) if valid_date(date) && valid_time(time) => Ok(()),
        _ => invalid(format!("{label}: Datum und Uhrzeit im Format JJJJ-MM-TTTHH:MM erwartet")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def(field_type: FieldType) -> FieldDef {
        FieldDef { name: "f", label: "Feld", field_type, required: false, default: None }
    }

    #[test]
    fn kinds_are_consistent() {
        let all = kinds();
        let mut names: Vec<_> = all.iter().map(|k| k.kind).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len(), "Typnamen müssen eindeutig sein");

        for k in &all {
            assert!(k.fields.iter().any(|f| f.name == k.title_field), "{}: Titelfeld fehlt", k.kind);
            assert!(k.fields.iter().any(|f| f.name == k.sort_field), "{}: Sortierfeld fehlt", k.kind);
            let mut fields: Vec<_> = k.fields.iter().map(|f| f.name).collect();
            fields.sort();
            fields.dedup();
            assert_eq!(fields.len(), k.fields.len(), "{}: Feldnamen müssen eindeutig sein", k.kind);
            for f in &k.fields {
                match &f.field_type {
                    FieldType::Ref { kind: target } | FieldType::RefList { kind: target } => {
                        assert!(all.iter().any(|x| x.kind == *target), "{}.{}: unbekanntes Ziel {target}", k.kind, f.name)
                    }
                    FieldType::Choice { options } => assert!(!options.is_empty()),
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn defaults_are_valid_options() {
        for k in kinds() {
            for f in &k.fields {
                if let Some(d) = f.default {
                    match &f.field_type {
                        FieldType::Choice { options } => assert!(options.contains(&d), "{}.{}: Vorgabe „{d}“ ist keine Option", k.kind, f.name),
                        _ => panic!("{}.{}: Vorgaben gibt es nur für Auswahlfelder", k.kind, f.name),
                    }
                }
            }
        }
        let status = kind("auftrag").unwrap().fields.into_iter().find(|f| f.name == "status").unwrap();
        assert_eq!((status.required, status.default), (true, Some("Offen")));
    }

    #[test]
    fn persons_are_marked_personal() {
        assert!(kind("ansprechpartner").unwrap().personal);
        assert!(kind("beratene_person").unwrap().personal);
        assert!(kind("protokoll").unwrap().personal, "Protokolle nennen oft Personen und sind zunächst gesperrt");
        assert!(!kind("beratung").unwrap().personal, "Beratungen verweisen nur per ID auf Personen");
        assert!(!kind("schule").unwrap().personal);
        assert!(kind("gibt-es-nicht").is_none());
    }

    #[test]
    fn schema_serializes_for_the_ui() {
        let json = serde_json::to_value(kinds()).unwrap();
        let schule = &json[0];
        assert_eq!(schule["kind"], "schule");
        let schulart = schule["fields"].as_array().unwrap().iter().find(|f| f["name"] == "schulart").unwrap();
        assert_eq!(schulart["type"], "choice");
        assert!(schulart["options"].as_array().unwrap().contains(&json!("Gymnasium")));
        let person = &json[1];
        let schule_ref = person["fields"].as_array().unwrap().iter().find(|f| f["name"] == "schule").unwrap();
        assert_eq!((schule_ref["type"].as_str(), schule_ref["kind"].as_str()), (Some("ref"), Some("schule")));
    }

    #[test]
    fn text_is_trimmed_and_empty_clears() {
        let d = def(FieldType::Text);
        assert_eq!(normalize(&d, &json!("  Hallo ")).unwrap(), json!("Hallo"));
        assert_eq!(normalize(&d, &json!("   ")).unwrap(), Value::Null);
        assert_eq!(normalize(&d, &Value::Null).unwrap(), Value::Null);
        assert!(normalize(&d, &json!(5)).is_err());
        assert!(normalize(&d, &json!("x".repeat(MAX_TEXT + 1))).is_err());
        assert!(normalize(&def(FieldType::LongText), &json!("x".repeat(MAX_TEXT + 1))).is_ok());
    }

    #[test]
    fn dates_are_validated() {
        let d = def(FieldType::Date);
        for good in ["2026-10-31", "2024-02-29", "1900-01-01"] {
            assert!(normalize(&d, &json!(good)).is_ok(), "{good}");
        }
        for bad in ["2026-02-29", "2026-13-01", "2026-00-10", "2026-10-32", "26-10-31", "2026/10/31", "heute", "2100-02-29", "1899-12-31"] {
            assert!(normalize(&d, &json!(bad)).is_err(), "{bad}");
        }
        assert!(normalize(&d, &json!("2000-02-29")).is_ok());
    }

    #[test]
    fn datetimes_are_validated() {
        let d = def(FieldType::DateTime);
        for good in ["2026-10-31T08:30", "2026-10-31T23:59:59"] {
            assert!(normalize(&d, &json!(good)).is_ok(), "{good}");
        }
        for bad in ["2026-10-31", "2026-10-31T24:00", "2026-10-31T08:60", "2026-10-31 08:30", "2026-10-31T8:30", "2026-10-31T08:30:60"] {
            assert!(normalize(&d, &json!(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn choice_number_bool() {
        let c = def(FieldType::Choice { options: &["a", "b"] });
        assert_eq!(normalize(&c, &json!("a")).unwrap(), json!("a"));
        assert!(normalize(&c, &json!("c")).is_err());
        assert!(normalize(&def(FieldType::Number), &json!(3.5)).is_ok());
        assert!(normalize(&def(FieldType::Number), &json!("3")).is_err());
        assert!(normalize(&def(FieldType::Bool), &json!(false)).is_ok());
        assert!(normalize(&def(FieldType::Bool), &json!("ja")).is_err());
    }

    #[test]
    fn references_are_validated_and_deduplicated() {
        let r = def(FieldType::Ref { kind: "schule" });
        assert!(normalize(&r, &json!("a1b2-c3")).is_ok());
        assert!(normalize(&r, &json!("a b")).is_err());
        assert!(normalize(&r, &json!("../x")).is_err());

        let l = def(FieldType::RefList { kind: "schule" });
        assert_eq!(normalize(&l, &json!(["a", "b", "a"])).unwrap(), json!(["a", "b"]));
        assert_eq!(normalize(&l, &json!([])).unwrap(), Value::Null);
        assert!(normalize(&l, &json!("a")).is_err());
        assert!(normalize(&l, &json!([1])).is_err());
        assert!(normalize(&l, &json!(["a b"])).is_err());
    }
}
