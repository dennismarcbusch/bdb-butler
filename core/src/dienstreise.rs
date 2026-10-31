//! Dienstreiseantrag (R001) und Reisekostenabrechnung: Zuordnung von Reise und Stammdaten zu Formularfeldern.
//!
//! Portierung von `dienstreisebuster/reisen/pdf/antrag.py` und `abrechnung.py`. Die Feldwerte sind gegen die
//! Python-Fassung abgesichert (`tests/fixtures/reference.json`, erzeugt von `make_reference.py`).
//!
//! Abweichungen von der Python-Fassung:
//! - Das Datum der Unterschrift im Antrag ist der Tag der Erzeugung (Python: Tag des Anlegens der Reise).
//! - Ist bei der Verpflegung keine Mahlzeit gewählt, bleibt jede Mahlzeit leer (Python kreuzte alle an).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::pdf::{self, FieldValue, PdfError, Signature};

/// R001, Stand 04/2019.
pub const ANTRAG_VORLAGE: &[u8] = include_bytes!("../assets/r001_antrag.pdf");

/// R002, Erstattungsantrag Reisekosten des Landesamts für Finanzen, Stand 04/2025.
pub const R002_VORLAGE: &[u8] = include_bytes!("../assets/r002_lff.pdf");

pub type Fields = Map<String, Value>;
pub type Values = BTreeMap<String, FieldValue>;

fn text<'a>(f: &'a Fields, name: &str) -> &'a str {
    f.get(name).and_then(Value::as_str).unwrap_or("")
}

fn flag(f: &Fields, name: &str) -> bool {
    f.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// `JJJJ-MM-TT` als `TT.MM.JJJJ`.
fn date_de(iso: &str) -> String {
    let part = iso.get(..10).unwrap_or(iso);
    match (part.get(0..4), part.get(5..7), part.get(8..10)) {
        (Some(y), Some(m), Some(d)) => format!("{d}.{m}.{y}"),
        _ => String::new(),
    }
}

/// `JJJJ-MM-TTTHH:MM` als `HH:MM`.
fn time_de(iso: &str) -> String {
    iso.get(11..16).unwrap_or("").to_string()
}

fn or_default<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.trim().is_empty() {
        default
    } else {
        value
    }
}

fn place_state(choice: &str) -> FieldValue {
    FieldValue::State(
        match choice {
            "Wohnort" | "wohnort" => "WOrt",
            "Dienststelle" | "dienststelle" => "DOrt",
            _ => "Off",
        }
        .to_string(),
    )
}

fn t(s: &str) -> FieldValue {
    FieldValue::text(s)
}

/// Feldwerte für den Antrag. `today` ist das Datum der Unterschrift als `JJJJ-MM-TT`.
pub fn antrag_values(reise: &Fields, st: &Fields, today: &str) -> Values {
    let bahn = flag(reise, "bahn");
    let oepnv = flag(reise, "oepnv");
    let start = text(reise, "reise_beginn");
    let dg_start = text(reise, "dienstgeschaeft_beginn");
    let dg_end = text(reise, "dienstgeschaeft_ende");
    let end = text(reise, "reise_ende");
    let mut v: Values = BTreeMap::new();
    let mut put = |name: &str, value: FieldValue| {
        v.insert(name.to_string(), value);
    };

    let fortbildung = matches!(text(reise, "art"), "Fortbildungsreise" | "fortbildung");
    put("S1 Antrag", FieldValue::State(if fortbildung { "Fortbildung" } else { "Dienstreise" }.to_string()));
    put("S1 Name", t(text(st, "name")));
    put("S1 Vorname", t(text(st, "vorname")));
    put("S1 PLZ", t(text(st, "plz")));
    put("S1 Ort", t(text(st, "ort")));
    put("S1 Str", t(text(st, "strasse")));
    put("S1 Beschäftigungsbehörde", t(text(st, "dienststelle_kurz")));
    put("S1 Abt", t(text(st, "abteilung")));
    put("S1 Entgeld", t(text(st, "besoldungsgruppe")));
    put("S1 Telefon", t(text(st, "telefon")));
    put("S1 E-Mail", t(text(st, "email")));
    put("S1 OrgNr", t(text(st, "org_nr")));
    put("S1 PersonalNr", t(text(st, "personal_nr")));
    put("S1 Zweck", t(text(reise, "zweck")));
    put("S1 GeschOrt", t(text(reise, "geschaeftsort")));
    put("S1 Rück", FieldValue::State(if flag(reise, "taegliche_rueckkehr") { "Ja" } else { "Nein" }.to_string()));
    put("S1Hin Ort", place_state(text(reise, "abfahrt_von")));
    put("S1 Hin andOrt", t(text(reise, "abfahrt_von_text")));
    put("S1 Hin Antritt", t(&date_de(start)));
    put("S1 Hin Uhrzeit", t(&time_de(start)));
    put("S1 Hin Beginn", t(&date_de(dg_start)));
    put("S1 Hin Beginn Uhrzeit2", t(&time_de(dg_start)));
    put("S1R Ort", place_state(text(reise, "rueckkehr_nach")));
    put("S1 Rü andOrt", t(text(reise, "rueckkehr_nach_text")));
    put("S1 Rü Antritt", t(&date_de(end)));
    put("S1 Rü Uhrzeit", t(&time_de(end)));
    put("S1 Rü Beginn", t(&date_de(dg_end)));
    // Das Feld heißt im Formular irreführend „Hin Beginn Uhrzeit“, gehört aber zum Ende des Dienstgeschäfts.
    put("S1 Hin Beginn Uhrzeit", t(&time_de(dg_end)));
    put("S1 VorsÜKosten", t(or_default(text(reise, "uebernachtungskosten"), "- keine -")));
    put("S1 VorausSonst", t(or_default(text(reise, "sonstige_kosten"), "- keine -")));
    put("S1 Ersatz", t(text(reise, "kostentraeger_dritte")));
    // „Regelmäßig verkehrende Beförderungsmittel“, davon ÖPNV und Bahn, mit BahnCard.
    put("S1 Beförd 1", FieldValue::check(oepnv || bahn, "Ja"));
    put("S1 Beförd 2", FieldValue::check(oepnv, "Ja"));
    put("S1 Beförd 4", FieldValue::check(bahn, "Ja"));
    put("S1 Beförd 5a", FieldValue::check(bahn && flag(reise, "bahncard"), "Ja"));
    put("S1 Ort U1", t(text(st, "unterschrift_ort")));
    put("S1 Datum U1", t(&date_de(today)));
    v
}

fn decimal_de(value: f64) -> String {
    format!("{value:.2}").replace('.', ",")
}

fn iban_parts(iban: &str) -> Values {
    let clean: String = iban.split_whitespace().collect();
    let chars: Vec<char> = clean.chars().collect();
    let slice = |from: usize, to: usize| -> String { chars.iter().skip(from).take(to.saturating_sub(from)).collect() };
    let parts = [slice(0, 4), slice(4, 8), slice(8, 12), slice(12, 16), slice(16, 20), chars.iter().skip(20).collect()];
    parts.into_iter().enumerate().map(|(i, p)| (format!("IBAN_{}", i + 1), FieldValue::Text(p))).collect()
}

fn meal_key(choice: &str) -> &'static str {
    match choice {
        "Frühstück" | "fruehstueck" => "Fruehstueck",
        "Mittagessen" | "mittagessen" => "Mittagessen",
        "Abendessen" | "abendessen" => "Abendessen",
        _ => "",
    }
}

/// Feldwerte für die Abrechnung im FIBS-Format (Akademie für Lehrerfortbildung, Stand 04/2025). Das Formular
/// kommt vorausgefüllt vom Veranstalter. Diese Felder bleiben unangetastet, ergänzt werden nur Angaben aus
/// Reise und Stammdaten.
pub fn abrechnung_values(reise: &Fields, st: &Fields, today: &str) -> Values {
    let start = text(reise, "reise_beginn");
    let end = text(reise, "reise_ende");
    let bahncard = flag(reise, "bahncard");
    let verpflegung = flag(reise, "verpflegung_gestellt");
    let hotel = flag(reise, "hotel_selbst_bezahlt");
    let mut v: Values = BTreeMap::new();
    let mut put = |name: &str, value: FieldValue| {
        v.insert(name.to_string(), value);
    };

    put("PLZ", t(text(st, "plz")));
    put("Ort", t(text(st, "ort")));
    put("Telefon", t(text(st, "telefon")));
    put("E-Mail", t(text(st, "email")));
    put("BIC", t(text(st, "bic")));
    put("Beginn_der_Reise", t(&format!("{} {}", date_de(start), time_de(start))));
    put("Ende_der_Reise", t(&format!("{} {}", date_de(end), time_de(end))));
    put("Bahncard_Ja", FieldValue::check(bahncard, "On"));
    put("Bahncard_Nein", FieldValue::check(!bahncard, "On"));
    put("Verpflegung_Ja", FieldValue::check(verpflegung, "On"));
    put("Verpflegung_Nein", FieldValue::check(!verpflegung, "On"));
    put("Hotel_Selbstzahlung_Ja", FieldValue::check(hotel, "On"));
    put("Hotel_Selbstzahlung_Nein", FieldValue::check(!hotel, "On"));
    put("Abrechnungshinweise_Teilnehmer_multiline", t(text(reise, "hinweise")));
    put("Unterschrift_Ort", t(text(st, "unterschrift_ort")));
    if let Some(price) = reise.get("fahrkartenpreis").and_then(Value::as_f64) {
        put("Fahrkartenpreis", t(&decimal_de(price)));
    }
    let (von, bis) = (text(reise, "verpflegung_von"), text(reise, "verpflegung_bis"));
    if verpflegung && !von.is_empty() && !bis.is_empty() {
        put("Verpflegung_Begin_Datum", t(&date_de(von)));
        put("Verpflegung_Ende_Datum", t(&date_de(bis)));
        for (prefix, choice) in [("Begin", text(reise, "verpflegung_von_mahlzeit")), ("Ende", text(reise, "verpflegung_bis_mahlzeit"))] {
            let selected = meal_key(choice);
            for name in ["Fruehstueck", "Mittagessen", "Abendessen"] {
                put(&format!("Verpflegung_{prefix}_{name}"), FieldValue::check(!selected.is_empty() && name == selected, "On"));
            }
        }
    }
    v.extend(iban_parts(text(st, "iban")));
    v.insert("Unterschrift_Datum".to_string(), FieldValue::Text(date_de(today)));
    v
}

fn place_choice(choice: &str) -> FieldValue {
    FieldValue::State(
        match choice {
            "Wohnort" | "wohnort" => "Wohnort",
            "Dienststelle" | "dienststelle" => "Dienststelle",
            "Anderer Ort" | "anderer" => "anderer Ort",
            _ => "Off",
        }
        .to_string(),
    )
}

/// Feldwerte für den Erstattungsantrag R002 des LfF (für Reisen ohne vorausgefülltes Formular des Veranstalters).
///
/// Befüllt wird nur, was sich eindeutig zuordnen lässt: persönliche Angaben, Bankverbindung, Reisegrund,
/// Zweck, Orte, Zeiten, Abfahrt und Rückkehr, Genehmigung (wenn eine hinterlegt ist) sowie die Bahn-Fahrkarte mit
/// Preis. Tagegeld, Übernachtung, Art der BahnCard, Mitfahrer und weitere Verkehrsmittel bleiben leer und sind im
/// PDF von Hand zu ergänzen.
pub fn r002_values(reise: &Fields, st: &Fields, has_approval: bool) -> Values {
    let start = text(reise, "reise_beginn");
    let end = text(reise, "reise_ende");
    let fortbildung = matches!(text(reise, "art"), "Fortbildungsreise" | "fortbildung");
    let street_and_town = [text(st, "strasse").trim().to_string(), format!("{} {}", text(st, "plz"), text(st, "ort")).trim().to_string()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let iban: String = text(st, "iban").split_whitespace().collect();

    let mut v: Values = BTreeMap::new();
    let mut put = |name: &str, value: FieldValue| {
        v.insert(name.to_string(), value);
    };
    put("S1 Name 1", t(text(st, "name")));
    put("S1 VorName", t(text(st, "vorname")));
    put("S1 PersNr", t(text(st, "personal_nr")));
    put("S1 DienstBez", t(text(st, "besoldungsgruppe")));
    put("S1 Schulbez", t(text(st, "abteilung")));
    put("S1 eMail", t(text(st, "email")));
    put("S1 Anschrift", t(&street_and_town));
    put("S1 Tel", t(text(st, "telefon")));
    put("S1 BeschäftgBeh", t(text(st, "dienststelle_kurz")));
    put("S1 IBAN des Kontoinhabers_bish", t(&iban));
    put("S1 BIC_bish", t(text(st, "bic")));
    put("S1 KreditInst", t(text(st, "kreditinstitut")));
    put("S1 DR", FieldValue::check(!fortbildung, "Dienstreise"));
    put("S1 FortbBReise", FieldValue::check(fortbildung, "Fortbildungsreise"));
    put("S1 Genehmigung", FieldValue::State(if has_approval { "liegt bei" } else { "Off" }.to_string()));
    put("S1 Reisezweck", t(text(reise, "zweck")));
    put("S1 GeschOrte", t(text(reise, "geschaeftsort")));
    put("S1 AntrittDat", t(&date_de(start)));
    put("S1 AntrittUhr", t(&time_de(start)));
    put("S1 EndeDat", t(&date_de(end)));
    put("S1 EndeUhr", t(&time_de(end)));
    put("S1 Abfahrt", place_choice(text(reise, "abfahrt_von")));
    put("S1 OrtText", t(text(reise, "abfahrt_von_text")));
    put("S1 Rückreise", place_choice(text(reise, "rueckkehr_nach")));
    put("S1 ROrtText", t(text(reise, "rueckkehr_nach_text")));
    if flag(reise, "bahn") {
        put("S2 Bahn", FieldValue::State("Bahn".to_string()));
        if let Some(price) = reise.get("fahrkartenpreis").and_then(Value::as_f64) {
            put("S2 Betrag3", t(&decimal_de(price)));
        }
    }
    v
}

/// Erzeugt den Erstattungsantrag R002 mit den Grunddaten. Eine Unterschrift setzt die App nicht ein, der
/// Antrag wird ausgedruckt oder im PDF-Programm unterschrieben.
pub fn r002_pdf(reise: &Fields, st: &Fields, has_approval: bool) -> Result<Vec<u8>, PdfError> {
    pdf::fill(R002_VORLAGE, &r002_values(reise, st, has_approval), None)
}

/// Erzeugt den Dienstreiseantrag (R001) mit Unterschrift. `svg` ist die Unterschrift aus den Stammdaten.
pub fn antrag_pdf(reise: &Fields, st: &Fields, today: &str, svg: Option<&str>) -> Result<Vec<u8>, PdfError> {
    let values = antrag_values(reise, st, today);
    let signature = match svg.filter(|s| !s.trim().is_empty()) {
        Some(svg) => {
            let [x0, y0, x1, y1] = pdf::field_rect(ANTRAG_VORLAGE, "S1 Unter U1", 0)?;
            Some(Signature { svg, page: 0, rect: [x0, y0 - 6.0, x1, y1 + 14.0] })
        }
        None => None,
    };
    pdf::fill(ANTRAG_VORLAGE, &values, signature.as_ref())
}

/// Füllt das vom Veranstalter vorausgefüllte Abrechnungsformular aus. Die Unterschrift steht rechts neben dem
/// Datumsfeld auf Seite 2.
pub fn abrechnung_pdf(form: &[u8], reise: &Fields, st: &Fields, today: &str, svg: Option<&str>) -> Result<Vec<u8>, PdfError> {
    let values = abrechnung_values(reise, st, today);
    let signature = match svg.filter(|s| !s.trim().is_empty()) {
        Some(svg) => {
            let [_, y0, x1, y1] = pdf::field_rect(form, "Unterschrift_Datum", 1)?;
            Some(Signature { svg, page: 1, rect: [x1 + 20.0, y0 - 8.0, x1 + 20.0 + 180.0, y1 + 16.0] })
        }
        None => None,
    };
    pdf::fill(form, &values, signature.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REFERENCE: &str = include_str!("../tests/fixtures/reference.json");

    /// Eingaben der Referenz als Felder eines Eintrags: leere Texte und `null` fehlen, wie in der Datenbank.
    fn fields_of(value: &Value) -> Fields {
        value
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, v)| !v.is_null() && v.as_str() != Some(""))
            .map(|(k, v)| {
                // Der Fahrkartenpreis steht in der Referenz als Zeichenkette (Decimal), im Eintrag als Zahl.
                if k == "fahrkartenpreis" {
                    (k.clone(), json!(v.as_str().unwrap().parse::<f64>().unwrap()))
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect()
    }

    fn expected(map: &Value) -> Values {
        map.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                let s = v.as_str().unwrap();
                let value = match s.strip_prefix('/') {
                    Some(state) => FieldValue::State(state.to_string()),
                    None => FieldValue::Text(s.to_string()),
                };
                (k.clone(), value)
            })
            .collect()
    }

    fn cases() -> Vec<Value> {
        serde_json::from_str::<Value>(REFERENCE).unwrap()["cases"].as_array().unwrap().clone()
    }

    const SIGNATURE: &str = include_str!("../tests/fixtures/unterschrift.svg");

    /// Schreibt Beispiel-PDFs nach `$BDB_PDF_OUT` für den optischen Vergleich mit der Python-Fassung.
    #[test]
    fn write_samples_for_manual_comparison() {
        let Ok(dir) = std::env::var("BDB_PDF_OUT") else { return };
        for case in cases() {
            let bytes = antrag_pdf(&fields_of(&case["reise"]), &fields_of(&case["stammdaten"]), case["today"].as_str().unwrap(), Some(SIGNATURE)).unwrap();
            std::fs::write(format!("{dir}/rust_antrag_{}.pdf", case["name"].as_str().unwrap()), bytes).unwrap();
            let r002 = r002_pdf(&fields_of(&case["reise"]), &fields_of(&case["stammdaten"]), true).unwrap();
            std::fs::write(format!("{dir}/rust_r002_{}.pdf", case["name"].as_str().unwrap()), r002).unwrap();
        }
    }

    #[test]
    fn antrag_values_match_the_python_reference() {
        for case in cases() {
            let reise = fields_of(&case["reise"]);
            let st = fields_of(&case["stammdaten"]);
            let today = case["today"].as_str().unwrap();
            let got = antrag_values(&reise, &st, today);
            let want = expected(&case["antrag"]);
            assert_eq!(got, want, "Fall {}", case["name"]);
        }
    }

    #[test]
    fn abrechnung_values_match_the_python_reference() {
        for case in cases() {
            let reise = fields_of(&case["reise"]);
            let st = fields_of(&case["stammdaten"]);
            let today = case["today"].as_str().unwrap();
            let got = abrechnung_values(&reise, &st, today);
            let want = expected(&case["abrechnung"]);
            assert_eq!(got, want, "Fall {}", case["name"]);
        }
    }

    #[test]
    fn r002_values_cover_the_unambiguous_fields_only() {
        let case = &cases()[0];
        let reise = fields_of(&case["reise"]);
        let st = fields_of(&case["stammdaten"]);
        let v = r002_values(&reise, &st, true);
        assert_eq!(v["S1 Name 1"], t("Mustermann"));
        assert_eq!(v["S1 Anschrift"], t("Hauptstraße 1, 89073 Ulm"));
        assert_eq!(v["S1 DienstBez"], t("A13"));
        assert_eq!(v["S1 Schulbez"], t("Fachbereich Digitale Bildung"));
        assert_eq!(v["S1 IBAN des Kontoinhabers_bish"], t("DE89370400440532013000"), "die IBAN ohne Leerzeichen, wie im Kammfeld");
        assert_eq!(v["S1 DR"], FieldValue::State("Dienstreise".into()));
        assert_eq!(v["S1 FortbBReise"], FieldValue::State("Off".into()));
        assert_eq!(v["S1 Genehmigung"], FieldValue::State("liegt bei".into()));
        assert_eq!((v["S1 AntrittDat"].clone(), v["S1 AntrittUhr"].clone()), (t("16.11.2026"), t("06:45")));
        assert_eq!((v["S1 EndeDat"].clone(), v["S1 EndeUhr"].clone()), (t("17.11.2026"), t("18:20")));
        assert_eq!(v["S1 Abfahrt"], FieldValue::State("Wohnort".into()));
        assert_eq!(v["S2 Bahn"], FieldValue::State("Bahn".into()));
        assert_eq!(v["S2 Betrag3"], t("123,45"));
        // Nichts, was sich nicht sicher zuordnen lässt.
        for name in ["S2 Bahncard25", "S2 Bahncard50", "S2 Klasse1", "S1 Tag1", "S2 Betrag1", "S2 ÜbernachtDatvon", "S1 SteuerID"] {
            assert!(!v.contains_key(name), "{name} bleibt leer");
        }

        // Fortbildungsreise, andere Orte, ohne Genehmigung, ohne Bahn.
        let other = fields_of(&json!({"art": "Fortbildungsreise", "abfahrt_von": "Anderer Ort", "abfahrt_von_text": "Augsburg", "rueckkehr_nach": "Dienststelle"}));
        let v = r002_values(&other, &Fields::new(), false);
        assert_eq!(v["S1 FortbBReise"], FieldValue::State("Fortbildungsreise".into()));
        assert_eq!(v["S1 DR"], FieldValue::State("Off".into()));
        assert_eq!(v["S1 Genehmigung"], FieldValue::State("Off".into()));
        assert_eq!((v["S1 Abfahrt"].clone(), v["S1 OrtText"].clone()), (FieldValue::State("anderer Ort".into()), t("Augsburg")));
        assert_eq!(v["S1 Rückreise"], FieldValue::State("Dienststelle".into()));
        assert!(!v.contains_key("S2 Bahn") && !v.contains_key("S2 Betrag3"));
        assert_eq!(v["S1 Anschrift"], t(""));
    }

    #[test]
    fn r002_form_is_filled_and_keeps_all_fields() {
        let case = &cases()[0];
        let reise = fields_of(&case["reise"]);
        let st = fields_of(&case["stammdaten"]);
        let out = r002_pdf(&reise, &st, false).unwrap();
        let read = pdf::read_values(&out).unwrap();
        for (name, want) in r002_values(&reise, &st, false) {
            assert_eq!(read.get(&name), Some(&want), "Feld {name}");
        }
        assert_eq!(pdf::field_names(&out).unwrap().len(), pdf::field_names(R002_VORLAGE).unwrap().len());
    }

    #[test]
    fn display_labels_work_like_stored_values() {
        // Die Oberfläche speichert Anzeigetexte („Fortbildungsreise“, „Wohnort“), Python Schlüssel.
        let reise = fields_of(&json!({"art": "Fortbildungsreise", "abfahrt_von": "Dienststelle", "rueckkehr_nach": "Anderer Ort"}));
        let v = antrag_values(&reise, &Fields::new(), "2026-10-31");
        assert_eq!(v["S1 Antrag"], FieldValue::State("Fortbildung".into()));
        assert_eq!(v["S1Hin Ort"], FieldValue::State("DOrt".into()));
        assert_eq!(v["S1R Ort"], FieldValue::State("Off".into()));
        assert_eq!(v["S1 VorsÜKosten"], FieldValue::text("- keine -"));
    }

    #[test]
    fn missing_meal_selects_nothing() {
        let reise = fields_of(&json!({"verpflegung_gestellt": true, "verpflegung_von": "2026-11-16", "verpflegung_bis": "2026-11-17"}));
        let v = abrechnung_values(&reise, &Fields::new(), "2026-10-31");
        for name in ["Fruehstueck", "Mittagessen", "Abendessen"] {
            assert_eq!(v[&format!("Verpflegung_Begin_{name}")], FieldValue::State("Off".into()));
        }
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(date_de("2026-11-16T06:45"), "16.11.2026");
        assert_eq!(time_de("2026-11-16T06:45"), "06:45");
        assert_eq!(date_de(""), "");
        assert_eq!(time_de("2026-11-16"), "");
        assert_eq!(decimal_de(123.456), "123,46");
        assert_eq!(decimal_de(7.0), "7,00");
        let iban = iban_parts("DE89 3704 0044 0532 0130 00");
        assert_eq!(iban["IBAN_1"], FieldValue::text("DE89"));
        assert_eq!(iban["IBAN_6"], FieldValue::text("00"));
        assert_eq!(iban_parts("")["IBAN_1"], FieldValue::text(""));
        assert_eq!(iban_parts("DE89")["IBAN_2"], FieldValue::text(""));
    }
}
