//! Tests für das Ausfüllen der PDFs: echte Vorlage R001, ein Nachbau des Veranstalterformulars und die
//! Unterschrift. Die Feldwerte wurden einmalig gegen die Python-Fassung geprüft (identische Werte und Zustände,
//! pixelgleiche Darstellung), siehe `tests/fixtures/make_reference.py`.

use std::collections::BTreeMap;

use bdb_core::dienstreise::{abrechnung_pdf, abrechnung_values, antrag_pdf, antrag_values, Fields, ANTRAG_VORLAGE};
use bdb_core::pdf::{self, FieldValue, PdfError, Signature};
use lopdf::{Document, Object};
use serde_json::{json, Value};

const REFERENCE: &str = include_str!("fixtures/reference.json");
const SIGNATURE: &str = include_str!("fixtures/unterschrift.svg");
const FORM: &[u8] = include_bytes!("fixtures/fibs_form.pdf");

fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(REFERENCE).unwrap()["cases"].as_array().unwrap().clone()
}

fn fields_of(value: &Value) -> Fields {
    value
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.is_null() && v.as_str() != Some(""))
        .map(|(k, v)| {
            if k == "fahrkartenpreis" {
                (k.clone(), json!(v.as_str().unwrap().parse::<f64>().unwrap()))
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}

fn text(s: &str) -> FieldValue {
    FieldValue::text(s)
}

/// Rechteck der Koordinaten, die der letzte Inhaltsstrom einer Seite zeichnet (nach Anwendung der Matrix).
fn drawn_bbox(pdf: &[u8], page_index: usize) -> [f32; 4] {
    let doc = Document::load_mem(pdf).unwrap();
    let page_id = *doc.get_pages().values().nth(page_index).unwrap();
    let contents = doc.get_dictionary(page_id).unwrap().get(b"Contents").unwrap().as_array().unwrap().clone();
    let last = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last).unwrap().as_stream().unwrap();
    let content = String::from_utf8(stream.content.clone()).unwrap();

    // Erste Zeile nach `q`: `s 0 0 -s tx ty cm`.
    let cm_line = content.lines().find(|l| l.ends_with(" cm")).expect("Matrix");
    let m: Vec<f32> = cm_line.trim_end_matches(" cm").split_whitespace().map(|n| n.parse().unwrap()).collect();
    let (a, d, e, f) = (m[0], m[3], m[4], m[5]);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let coords: Vec<f32> = match parts.last().copied() {
            Some("m") | Some("l") => parts[..parts.len() - 1].iter().filter_map(|p| p.parse().ok()).collect(),
            Some("c") => parts[..parts.len() - 1].iter().filter_map(|p| p.parse().ok()).collect(),
            _ => continue,
        };
        for pair in coords.chunks_exact(2) {
            let (x, y) = (a * pair[0] + e, d * pair[1] + f);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    [min_x, min_y, max_x, max_y]
}

fn inside(inner: [f32; 4], outer: [f32; 4], tolerance: f32) -> bool {
    inner[0] >= outer[0] - tolerance && inner[1] >= outer[1] - tolerance && inner[2] <= outer[2] + tolerance && inner[3] <= outer[3] + tolerance
}

#[test]
fn template_has_the_expected_fields() {
    let names = pdf::field_names(ANTRAG_VORLAGE).unwrap();
    assert_eq!(names.len(), 109);
    for n in ["S1 Name", "S1 Antrag", "S1 Unter U1", "S1 Hin Beginn Uhrzeit", "S1 Beschäftigungsbehörde"] {
        assert!(names.iter().any(|x| x == n), "Feld {n}");
    }
    let rect = pdf::field_rect(ANTRAG_VORLAGE, "S1 Unter U1", 0).unwrap();
    assert!(rect[2] > rect[0] && rect[3] > rect[1]);
    assert!(rect[2] < 596.0 && rect[3] < 842.0);
}

#[test]
fn filled_antrag_contains_all_values() {
    for case in cases() {
        let reise = fields_of(&case["reise"]);
        let st = fields_of(&case["stammdaten"]);
        let today = case["today"].as_str().unwrap();
        let out = antrag_pdf(&reise, &st, today, None).unwrap();
        let read = pdf::read_values(&out).unwrap();
        for (name, want) in antrag_values(&reise, &st, today) {
            assert_eq!(read.get(&name), Some(&want), "Fall {}, Feld {name}", case["name"]);
        }
        assert_eq!(pdf::field_names(&out).unwrap().len(), 109, "die Anzahl der Felder ändert sich nicht");
    }
}

#[test]
fn radio_groups_and_checkboxes_show_exactly_the_chosen_state() {
    let case = &cases()[0];
    let out = antrag_pdf(&fields_of(&case["reise"]), &fields_of(&case["stammdaten"]), "2026-10-31", None).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    // Alle Widgets mit /AS einsammeln: Gruppenname -> Zustände.
    let mut states: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, page_id) in doc.get_pages() {
        for annot in doc.get_page_annotations(page_id).unwrap() {
            let Ok(state) = annot.get(b"AS").and_then(|s| s.as_name()) else { continue };
            let name_of = |d: &lopdf::Dictionary| d.get(b"T").ok().and_then(|t| t.as_str().ok()).map(|b| b.iter().map(|&c| c as char).collect::<String>());
            let name = name_of(annot).or_else(|| {
                let parent = annot.get(b"Parent").ok()?.as_reference().ok()?;
                name_of(doc.get_dictionary(parent).ok()?)
            });
            if let Some(name) = name {
                states.entry(name).or_default().push(String::from_utf8_lossy(state).to_string());
            }
        }
    }
    // „Dienstreise“ ist gewählt: genau ein Widget der Gruppe zeigt den Zustand.
    let antrag = &states["S1 Antrag"];
    assert_eq!(antrag.iter().filter(|s| *s != "Off").count(), 1, "{antrag:?}");
    assert!(antrag.contains(&"Dienstreise".to_string()));
    // Wohnort-Radio für Hin- und Rückfahrt.
    assert!(states["S1Hin Ort"].contains(&"WOrt".to_string()));
    assert!(states["S1R Ort"].contains(&"WOrt".to_string()));
    // Keine tägliche Rückkehr: „Nein“.
    assert!(states["S1 Rück"].contains(&"Nein".to_string()) && !states["S1 Rück"].contains(&"Ja".to_string()));
    // Bahn und BahnCard angekreuzt, ÖPNV nicht.
    assert_eq!(states["S1 Beförd 4"], ["Ja"]);
    assert_eq!(states["S1 Beförd 5a"], ["Ja"]);
    assert_eq!(states["S1 Beförd 2"], ["Off"]);
}

#[test]
fn text_fields_get_an_appearance_stream_and_need_appearances_is_set() {
    let case = &cases()[0];
    let out = antrag_pdf(&fields_of(&case["reise"]), &fields_of(&case["stammdaten"]), "2026-10-31", None).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let acro = doc.catalog().unwrap().get(b"AcroForm").unwrap();
    let acro = match acro {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!(),
    };
    assert!(acro.get(b"NeedAppearances").unwrap().as_bool().unwrap());

    // Das Feld „S1 Name“ hat ein Erscheinungsbild mit dem Text.
    let mut found = false;
    for (_, page_id) in doc.get_pages() {
        for annot in doc.get_page_annotations(page_id).unwrap() {
            if annot.get(b"T").ok().and_then(|t| t.as_str().ok()) == Some(b"S1 Name".as_slice()) {
                let ap = annot.get(b"AP").unwrap().as_dict().unwrap();
                let stream_id = ap.get(b"N").unwrap().as_reference().unwrap();
                let stream = doc.get_object(stream_id).unwrap().as_stream().unwrap();
                assert!(String::from_utf8_lossy(&stream.content).contains("(Mustermann) Tj"));
                found = true;
            }
        }
    }
    assert!(found);
}

#[test]
fn special_characters_and_multiline_text_survive() {
    let mut values = BTreeMap::new();
    values.insert("S1 Zweck".to_string(), text("Größe, Müll, 12,50 € – „Test“ und 日本語"));
    values.insert("S1 Erläuterung".to_string(), text("Zeile eins\nZeile zwei\nZeile drei"));
    values.insert("S1 Ort".to_string(), text("Straße (mit) \\ Klammern"));
    let out = pdf::fill(ANTRAG_VORLAGE, &values, None).unwrap();
    let read = pdf::read_values(&out).unwrap();
    for (name, value) in &values {
        assert_eq!(read.get(name), Some(value), "Feld {name}");
    }
    // Nochmaliges Ausfüllen der Ausgabe funktioniert und überschreibt.
    values.insert("S1 Zweck".to_string(), text("neu"));
    let again = pdf::fill(&out, &values, None).unwrap();
    assert_eq!(pdf::read_values(&again).unwrap()["S1 Zweck"], text("neu"));
}

#[test]
fn unknown_fields_and_wrong_types_are_ignored() {
    let mut values = BTreeMap::new();
    values.insert("Gibt es nicht".to_string(), text("x"));
    values.insert("S1 Name".to_string(), FieldValue::State("Ja".to_string())); // Textfeld mit Zustand: ignoriert
    values.insert("S1 Antrag".to_string(), text("Dienstreise")); // Radiogruppe mit Text: ignoriert
    values.insert("Drucken".to_string(), FieldValue::State("Ja".to_string())); // Pushbutton bleibt unberührt
    let out = pdf::fill(ANTRAG_VORLAGE, &values, None).unwrap();
    let read = pdf::read_values(&out).unwrap();
    assert!(!read.contains_key("Gibt es nicht"));
    assert_ne!(read.get("S1 Name"), Some(&FieldValue::State("Ja".to_string())));
}

#[test]
fn signature_lands_inside_the_signature_box() {
    let case = &cases()[0];
    let reise = fields_of(&case["reise"]);
    let st = fields_of(&case["stammdaten"]);
    let out = antrag_pdf(&reise, &st, "2026-10-31", Some(SIGNATURE)).unwrap();
    let [x0, y0, x1, y1] = pdf::field_rect(ANTRAG_VORLAGE, "S1 Unter U1", 0).unwrap();
    let target = [x0, y0 - 6.0, x1, y1 + 14.0];
    let bbox = drawn_bbox(&out, 0);
    assert!(inside(bbox, target, 3.0), "Unterschrift {bbox:?} liegt nicht in {target:?}");
    // Und sie füllt das Rechteck nennenswert (nicht nur ein Punkt).
    assert!((bbox[2] - bbox[0]) > 0.5 * (target[2] - target[0]) || (bbox[3] - bbox[1]) > 0.5 * (target[3] - target[1]));

    // Ohne Unterschrift gibt es keinen zusätzlichen Inhalt, mit leerer Unterschrift ebenfalls nicht.
    let plain = antrag_pdf(&reise, &st, "2026-10-31", None).unwrap();
    let blank = antrag_pdf(&reise, &st, "2026-10-31", Some("  ")).unwrap();
    let count = |bytes: &[u8]| {
        let doc = Document::load_mem(bytes).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        match doc.get_dictionary(page_id).unwrap().get(b"Contents").unwrap() {
            Object::Array(a) => a.len(),
            _ => 1,
        }
    };
    assert_eq!(count(&plain), count(&blank));
    assert_eq!(count(&out), count(&plain) + 1);
}

#[test]
fn signature_works_on_arbitrary_rectangles_and_svg_shapes() {
    let values = BTreeMap::new();
    // Quadratische Kurven, Rechtecke, Kreise, Verschiebung, Skalierung und gefüllte Flächen. Eingepasst wird die
    // Zeichenfläche des SVG (120 x 60), der Inhalt liegt darin.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60" viewBox="0 0 120 60">
        <g transform="translate(10 10) scale(0.8)">
          <path d="M0 40 Q 30 20 60 30 T 100 20" fill="none" stroke="blue" stroke-width="2"/>
          <rect x="5" y="5" width="20" height="10" fill="red" stroke="black"/>
          <circle cx="90" cy="40" r="8" fill="green"/>
        </g></svg>"#;
    for rect in [[50.0, 50.0, 250.0, 130.0], [300.0, 400.0, 340.0, 560.0], [10.0, 10.0, 500.0, 40.0]] {
        let out = pdf::fill(ANTRAG_VORLAGE, &values, Some(&Signature { svg, page: 0, rect })).unwrap();
        let bbox = drawn_bbox(&out, 0);
        assert!(inside(bbox, rect, 2.0), "{bbox:?} in {rect:?}");
        assert!(bbox.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn signature_errors() {
    let values = BTreeMap::new();
    let rect = [50.0, 50.0, 250.0, 130.0];
    let sig = |svg: &'static str, page, rect| Signature { svg, page, rect };
    assert!(matches!(pdf::fill(ANTRAG_VORLAGE, &values, Some(&sig("kein svg", 0, rect))), Err(PdfError::Signature(_))));
    assert!(matches!(
        pdf::fill(ANTRAG_VORLAGE, &values, Some(&sig(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#, 0, rect))),
        Err(PdfError::Signature(_))
    ));
    assert!(matches!(pdf::fill(ANTRAG_VORLAGE, &values, Some(&sig(SIGNATURE, 5, rect))), Err(PdfError::NoSuchPage(5))));
    assert!(matches!(pdf::fill(ANTRAG_VORLAGE, &values, Some(&sig(SIGNATURE, 0, [10.0, 10.0, 10.0, 50.0]))), Err(PdfError::Signature(_))));
}

#[test]
fn pdf_errors() {
    assert!(matches!(pdf::fill(b"kein pdf", &BTreeMap::new(), None), Err(PdfError::Read(_))));
    assert!(matches!(pdf::field_names(b""), Err(PdfError::Read(_))));
    assert!(matches!(pdf::field_rect(ANTRAG_VORLAGE, "Gibt es nicht", 0), Err(PdfError::NoSuchField(_))));
    assert!(matches!(pdf::field_rect(ANTRAG_VORLAGE, "S1 Name", 3), Err(PdfError::NoSuchPage(3))));

    // Ein PDF ohne Formular.
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(lopdf::dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()] });
    doc.objects.insert(pages_id, Object::Dictionary(lopdf::dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
    let catalog = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert!(matches!(pdf::fill(&bytes, &BTreeMap::new(), None), Err(PdfError::NoForm)));
    assert!(matches!(pdf::field_names(&bytes), Err(PdfError::NoForm)));
}

#[test]
fn abrechnung_fills_the_organizer_form_without_touching_prefilled_fields() {
    for case in cases() {
        let reise = fields_of(&case["reise"]);
        let st = fields_of(&case["stammdaten"]);
        let today = case["today"].as_str().unwrap();
        let out = abrechnung_pdf(FORM, &reise, &st, today, Some(SIGNATURE)).unwrap();
        let read = pdf::read_values(&out).unwrap();

        // Vom Veranstalter vorausgefüllt: unverändert.
        assert_eq!(read["Aktenzeichen"], text("AZ-2026/1234"));
        // Alle ergänzten Werte stehen im Formular.
        for (name, want) in abrechnung_values(&reise, &st, today) {
            assert_eq!(read.get(&name), Some(&want), "Fall {}, Feld {name}", case["name"]);
        }
        // IBAN in Gruppen, Betrag mit Komma.
        assert_eq!(read["IBAN_1"], text("DE89"));
        assert_eq!(read["IBAN_6"], text("00"));

        // Die Unterschrift steht auf Seite 2 rechts neben dem Datumsfeld.
        let [_, y0, x1, y1] = pdf::field_rect(FORM, "Unterschrift_Datum", 1).unwrap();
        let target = [x1 + 20.0, y0 - 8.0, x1 + 200.0, y1 + 16.0];
        assert!(inside(drawn_bbox(&out, 1), target, 3.0));
    }
    // Der Betrag steht nur in Fall 1 (mit Fahrkartenpreis).
    let first = &cases()[0];
    let out = abrechnung_pdf(FORM, &fields_of(&first["reise"]), &fields_of(&first["stammdaten"]), "2026-10-31", None).unwrap();
    assert_eq!(pdf::read_values(&out).unwrap()["Fahrkartenpreis"], text("123,45"));
}

#[test]
fn abrechnung_with_an_unsuitable_form_reports_errors() {
    let first = &cases()[0];
    let (reise, st) = (fields_of(&first["reise"]), fields_of(&first["stammdaten"]));
    // Der Antrag R001 hat nur eine Seite: die Unterschrift auf Seite 2 gibt es nicht.
    let err = abrechnung_pdf(ANTRAG_VORLAGE, &reise, &st, "2026-10-31", Some(SIGNATURE)).unwrap_err();
    assert!(matches!(err, PdfError::NoSuchPage(_) | PdfError::NoSuchField(_)), "{err:?}");
    assert!(abrechnung_pdf(b"kein pdf", &reise, &st, "2026-10-31", None).is_err());
}
