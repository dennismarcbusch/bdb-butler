//! Erkennung des Preises in einer Fahrkarte (PDF) als Vorschlag.
//!
//! Das ist eine Heuristik auf dem Text des PDFs. Sie funktioniert nur bei PDFs mit Textinhalt (nicht bei
//! eingescannten Bildern) und ist **nicht an echten Fahrkarten verschiedener Anbieter geprüft**. Das Ergebnis ist
//! deshalb immer nur ein Vorschlag, den der Nutzer bestätigt.
//!
//! Vorgehen: Zeilen mit Schlüsselwörtern wie „Gesamtpreis“ oder „Summe“ haben den Vorrang, danach „Preis“ oder
//! „Betrag“. Zeilen über Steuer, Rabatt oder Ersparnis zählen nie. Steht der Betrag in der Zeile darunter, wird er
//! von dort genommen. Ohne Schlüsselwort gilt ein einziger Betrag mit Währung im ganzen Text.

/// Betrag nach deutscher Schreibweise (`1.234,50`) an der Stelle `start`: Wert und Länge in Bytes.
fn amount_at(text: &str, start: usize) -> Option<(f64, usize)> {
    let bytes = text.as_bytes();
    let mut i = start;
    let digits_from = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())) {
        i += 1;
    }
    if i == digits_from {
        return None;
    }
    if bytes.get(i) != Some(&b',') || !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) || !bytes.get(i + 2).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    // Eine dritte Nachkommastelle gehört nicht zu einem Preis (zum Beispiel 1,234).
    if bytes.get(i + 3).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let whole: String = text[digits_from..i].chars().filter(|c| *c != '.').collect();
    let value: f64 = format!("{whole}.{}", &text[i + 1..i + 3]).parse().ok()?;
    Some((value, i + 3 - start))
}

/// Alle Beträge einer Zeile mit Angabe, ob eine Währung (EUR, €) unmittelbar dabei steht.
fn amounts_in_line(line: &str) -> Vec<(f64, bool)> {
    let mut out = Vec::new();
    let mut i = 0;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    while i < chars.len() {
        let (pos, c) = chars[i];
        let previous_is_digit = i > 0 && (chars[i - 1].1.is_ascii_digit() || chars[i - 1].1 == ',' || chars[i - 1].1 == '.');
        if c.is_ascii_digit() && !previous_is_digit {
            if let Some((value, len)) = amount_at(line, pos) {
                let before = line[..pos].trim_end().to_uppercase();
                let after = line[pos + len..].trim_start().to_uppercase();
                let currency = before.ends_with("EUR") || before.ends_with('€') || after.starts_with("EUR") || after.starts_with('€');
                out.push((value, currency));
                // Bis hinter den Betrag springen.
                while i < chars.len() && chars[i].0 < pos + len {
                    i += 1;
                }
                continue;
            }
        }
        i += 1;
    }
    out
}

const EXCLUDED: &[&str] = &["mwst", "mehrwertsteuer", "ust", "umsatzsteuer", "steuer", "enthalten", "davon", "rabatt", "ersparnis", "nachlass", "gutschrift", "erstattung", "pfand", "bahncard"];
const STRONG: &[&str] = &["gesamtpreis", "gesamtbetrag", "zahlbetrag", "endbetrag", "gesamtsumme", "summe", "total", "gesamt", "zu zahlen", "rechnungsbetrag"];
const WEAK: &[&str] = &["fahrpreis", "ticketpreis", "preis", "betrag", "kosten"];

fn has_any(line: &str, words: &[&str]) -> bool {
    let lower = line.to_lowercase();
    words.iter().any(|w| lower.contains(w))
}

/// Preis in Euro, wenn der Text einen eindeutigen Vorschlag hergibt.
pub fn detect_price(text: &str) -> Option<f64> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let candidates = |keywords: &[&str]| -> Vec<f64> {
        let mut found = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !has_any(line, keywords) || has_any(line, EXCLUDED) {
                continue;
            }
            let mut amounts = amounts_in_line(line);
            if amounts.is_empty() {
                // Der Betrag steht in der Zeile darunter, solange diese nicht von der Steuer handelt.
                if let Some(next) = lines.get(i + 1) {
                    if !has_any(next, EXCLUDED) {
                        amounts = amounts_in_line(next);
                    }
                }
            }
            found.extend(amounts.into_iter().map(|(v, _)| v).filter(|v| *v > 0.0));
        }
        found
    };
    for keywords in [STRONG, WEAK] {
        let found = candidates(keywords);
        if let Some(max) = found.iter().copied().reduce(f64::max) {
            return Some(max);
        }
    }
    // Ohne Schlüsselwort: genau ein Betrag mit Währung im ganzen Text.
    let with_currency: Vec<f64> = lines
        .iter()
        .filter(|l| !has_any(l, EXCLUDED))
        .flat_map(|l| amounts_in_line(l))
        .filter(|(v, currency)| *currency && *v > 0.0)
        .map(|(v, _)| v)
        .collect();
    match with_currency.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_total_and_ignores_tax_and_discount_lines() {
        assert_eq!(detect_price("Ihr Ticket\nGesamtpreis: 59,90 EUR\ninkl. 19% MwSt. 9,56 EUR"), Some(59.90));
        assert_eq!(detect_price("Preis 29,90 €"), Some(29.90));
        assert_eq!(detect_price("Rabatt: -10,00 EUR\nGesamtpreis 49,90 EUR\nBahnCard 25: 62,00 EUR"), Some(49.90));
        assert_eq!(detect_price("Summe\n1.234,50 EUR\nenthaltene Steuer 197,10 EUR"), Some(1234.50));
        assert_eq!(detect_price("Zahlbetrag: EUR 118,00"), Some(118.0));
        assert_eq!(detect_price("Fahrpreis 12,40 EUR"), Some(12.40));
    }

    #[test]
    fn strong_keywords_win_over_weak_ones() {
        assert_eq!(detect_price("Preis Hinfahrt 30,00 EUR\nPreis Rückfahrt 30,00 EUR\nGesamtpreis 60,00 EUR"), Some(60.0));
        // Mehrere Gesamtbeträge: der größte.
        assert_eq!(detect_price("Gesamtpreis 20,00 EUR\nGesamtpreis 45,50 EUR"), Some(45.5));
    }

    #[test]
    fn a_single_amount_with_currency_is_a_fallback() {
        assert_eq!(detect_price("Hamburg Hbf nach München Hbf\n89,00 EUR\nGültig am 16.11.2026"), Some(89.0));
        // Zwei Beträge ohne Schlüsselwort: kein Vorschlag, statt zu raten.
        assert_eq!(detect_price("Hinfahrt 30,00 EUR\nRückfahrt 31,00 EUR"), None);
        // Betrag ohne Währung zählt nicht.
        assert_eq!(detect_price("Wagen 12,50"), None);
    }

    #[test]
    fn rejects_text_without_a_usable_price() {
        assert_eq!(detect_price(""), None);
        assert_eq!(detect_price("Kein Preis hier, nur Text 2026"), None);
        assert_eq!(detect_price("Gesamtpreis: 0,00 EUR"), None, "ein Preis von null ist keiner");
        assert_eq!(detect_price("Preis: 1,234 EUR"), None, "drei Nachkommastellen sind kein Preis");
        assert_eq!(detect_price("MwSt. 9,56 EUR\nRabatt 5,00 EUR"), None, "nur Steuer und Rabatt");
        assert_eq!(detect_price("Preis:"), None);
    }

    #[test]
    fn parses_german_amounts() {
        assert_eq!(amounts_in_line("Summe 1.234,56 EUR und 7,00 €"), vec![(1234.56, true), (7.0, true)]);
        assert_eq!(amounts_in_line("Datum 16.11.2026 Uhrzeit 06,45"), vec![(6.45, false)], "ein Datum ist kein Betrag, 06,45 schon");
        assert_eq!(amounts_in_line("ohne Zahlen"), vec![]);
        assert_eq!(amounts_in_line("EUR 59,90"), vec![(59.9, true)]);
    }
}
