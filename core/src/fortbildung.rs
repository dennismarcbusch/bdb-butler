//! Prozess einer Fortbildung: Bedarf, Planung, Ausschreibung in FIBS, Durchführung, Nachbereitung.
//!
//! Bedarfsanmeldungen (`bedarf`) werden gesammelt und einer Fortbildung zugeordnet. Die Fortbildung durchläuft
//! die Phasen aus [`PHASES`]. Vorwärts darf sie nur, wenn die Angaben der übersprungenen Phasen vollständig
//! sind. Zurück geht immer (zum Korrigieren).

use serde_json::{Map, Value};

/// Phasen einer Fortbildung in dieser Reihenfolge.
pub const PHASES: &[&str] = &["Planung", "Ausschreibung", "Durchführung", "Nachbereitung", "Abgeschlossen"];

/// Angaben der Planung, ohne die sich die Fortbildung nicht ausschreiben lässt: (Feld, Bezeichnung).
const PLANNING: &[(&str, &str)] = &[
    ("referent", "Referent"),
    ("inhalt", "Inhalt"),
    ("beginn", "Datum und Uhrzeit (Beginn)"),
    ("ort", "Ort"),
    ("teilnehmer_geplant", "Anzahl Teilnehmer (geplant)"),
];

pub fn phase_index(phase: &str) -> Option<usize> {
    PHASES.iter().position(|p| *p == phase)
}

fn filled(fields: &Map<String, Value>, name: &str) -> bool {
    match fields.get(name) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(_) => true,
    }
}

/// Anzahl offener und aller Aufgaben (`- [ ]` und `- [x]`) in einem Markdown-Text. Aufgaben in Codeblöcken zählen
/// nicht.
pub fn task_counts(markdown: &str) -> (usize, usize) {
    let (mut open, mut total) = (0, 0);
    let mut in_code = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let rest = trimmed.strip_prefix(['-', '*', '+']).map(str::trim_start);
        if let Some(rest) = rest {
            if rest.starts_with("[ ]") {
                open += 1;
                total += 1;
            } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
                total += 1;
            }
        }
    }
    (open, total)
}

/// Was noch fehlt, damit die Fortbildung die Phase `to` verlassen darf, in der Reihenfolge der Phasen. `fields`
/// sind die Angaben nach der Änderung. Eine Rückwärtsbewegung oder gleiche Phase hat nie Hindernisse.
pub fn blockers(fields: &Map<String, Value>, from: &str, to: &str) -> Vec<String> {
    let (Some(from_i), Some(to_i)) = (phase_index(from), phase_index(to)) else { return Vec::new() };
    let mut out = Vec::new();
    // Jede verlassene Phase muss ihre Angaben haben.
    for leaving in from_i..to_i {
        match PHASES[leaving] {
            "Planung" => {
                let missing: Vec<&str> = PLANNING.iter().filter(|(f, _)| !filled(fields, f)).map(|(_, l)| *l).collect();
                if !missing.is_empty() {
                    out.push(format!("Für die Ausschreibung fehlt aus der Planung: {}", missing.join(", ")));
                }
            }
            "Ausschreibung" => {
                if !filled(fields, "fibs_nummer") {
                    out.push("Für die Durchführung fehlt die Veranstaltungsnummer aus FIBS (Ausschreibung)".to_string());
                }
            }
            "Nachbereitung" => {
                let checklist = fields.get("checkliste_nachbereitung").and_then(Value::as_str).unwrap_or("");
                let (open, _) = task_counts(checklist);
                if open > 0 {
                    let word = if open == 1 { "Aufgabe" } else { "Aufgaben" };
                    out.push(format!("Die Checkliste der Nachbereitung hat noch {open} offene {word}"));
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn planned() -> Map<String, Value> {
        fields(json!({"referent": "Dr. Beispiel", "inhalt": "KI im Unterricht", "beginn": "2027-02-10T09:00", "ort": "Ulm", "teilnehmer_geplant": 25}))
    }

    #[test]
    fn phases_have_an_order() {
        assert_eq!(phase_index("Planung"), Some(0));
        assert_eq!(phase_index("Abgeschlossen"), Some(4));
        assert_eq!(phase_index("Unsinn"), None);
        assert!(PHASES.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn planning_must_be_complete_before_tendering() {
        let missing = blockers(&fields(json!({"referent": "X"})), "Planung", "Ausschreibung");
        assert_eq!(missing.len(), 1);
        for wanted in ["Inhalt", "Datum und Uhrzeit (Beginn)", "Ort", "Anzahl Teilnehmer (geplant)"] {
            assert!(missing[0].contains(wanted), "{missing:?}");
        }
        assert!(!missing[0].contains("Referent"), "ausgefüllte Angaben stehen nicht in der Liste");
        assert!(blockers(&planned(), "Planung", "Ausschreibung").is_empty());
        // Leerer Text zählt als fehlend, die Zahl 0 nicht.
        let mut f = planned();
        f.insert("ort".into(), json!("  "));
        assert_eq!(blockers(&f, "Planung", "Ausschreibung").len(), 1);
        let mut f = planned();
        f.insert("teilnehmer_geplant".into(), json!(0));
        assert!(blockers(&f, "Planung", "Ausschreibung").is_empty());
    }

    #[test]
    fn execution_needs_the_fibs_number() {
        assert_eq!(blockers(&planned(), "Ausschreibung", "Durchführung").len(), 1);
        let mut f = planned();
        f.insert("fibs_nummer".into(), json!("2027-12345"));
        assert!(blockers(&f, "Ausschreibung", "Durchführung").is_empty());
    }

    #[test]
    fn closing_needs_the_follow_up_checklist_done() {
        let mut f = planned();
        f.insert("fibs_nummer".into(), json!("1"));
        f.insert("checkliste_nachbereitung".into(), json!("- [x] Bescheinigungen\n- [ ] Abrechnung\n- [ ] Evaluation"));
        let b = blockers(&f, "Nachbereitung", "Abgeschlossen");
        assert_eq!(b, ["Die Checkliste der Nachbereitung hat noch 2 offene Aufgaben"]);
        f.insert("checkliste_nachbereitung".into(), json!("- [x] a\n- [ ] b"));
        assert_eq!(blockers(&f, "Nachbereitung", "Abgeschlossen"), ["Die Checkliste der Nachbereitung hat noch 1 offene Aufgabe"]);
        f.insert("checkliste_nachbereitung".into(), json!("- [x] a\n- [X] b"));
        assert!(blockers(&f, "Nachbereitung", "Abgeschlossen").is_empty());
        // Ohne Checkliste gibt es nichts abzuhaken.
        f.remove("checkliste_nachbereitung");
        assert!(blockers(&f, "Nachbereitung", "Abgeschlossen").is_empty());
    }

    #[test]
    fn skipping_phases_checks_every_phase_on_the_way() {
        let all = blockers(&fields(json!({})), "Planung", "Abgeschlossen");
        assert_eq!(all.len(), 2, "Planung und Ausschreibung fehlen: {all:?}");
        // Die Nachbereitung wird beim Überspringen nicht geprüft, denn sie wurde nie betreten. Die Checkliste
        // beginnt ja erst dort.
        let planned_only = blockers(&planned(), "Planung", "Durchführung");
        assert_eq!(planned_only.len(), 1);
        assert!(planned_only[0].contains("FIBS"));
    }

    #[test]
    fn going_back_or_staying_never_blocks() {
        let empty = fields(json!({}));
        assert!(blockers(&empty, "Durchführung", "Planung").is_empty());
        assert!(blockers(&empty, "Abgeschlossen", "Nachbereitung").is_empty());
        assert!(blockers(&empty, "Planung", "Planung").is_empty());
        assert!(blockers(&empty, "Unsinn", "Planung").is_empty());
    }

    #[test]
    fn counts_tasks_in_markdown() {
        assert_eq!(task_counts(""), (0, 0));
        assert_eq!(task_counts("- [ ] a\n* [x] b\n  + [ ] c\n- normal\n[ ] ohne Strich"), (2, 3));
        assert_eq!(task_counts("```\n- [ ] in Code\n```\n- [ ] echt"), (1, 1));
        assert_eq!(task_counts("-  [ ] zwei Leerzeichen"), (1, 1));
    }
}
