//! Ausfüllen von AcroForm-PDFs und Aufsetzen einer Unterschrift (SVG).
//!
//! Portierung von `reisen/pdf/fuellen.py` des alten Dienstreisebusters (pypdf, reportlab, svglib) nach Rust.
//!
//! - Textfelder bekommen `/V` und, wenn möglich, ein einfaches Erscheinungsbild (einzeilig oder mit
//!   expliziten Zeilenumbrüchen, linksbündig). Das Formular setzt außerdem `NeedAppearances`, damit
//!   Betrachter alle Felder neu zeichnen. Felder, für die kein Erscheinungsbild erzeugt wird (zentriert,
//!   rechtsbündig), verlieren ihr altes und werden vom Betrachter gezeichnet.
//! - Checkboxen und Radiogruppen: `/V` am Feld und `/AS` an jedem Widget. Ein Widget zeigt einen Zustand
//!   nur, wenn er in seinem Erscheinungsbild (`/AP /N`) vorkommt, sonst `/Off`.
//! - Die Unterschrift wird als Vektorpfade (aus dem SVG über `usvg`) auf die Seite gezeichnet und in ein
//!   Rechteck eingepasst. Unterstützt werden Pfade mit einfarbiger Füllung und Kontur. Verläufe, Bilder,
//!   Masken und Transparenz werden nicht ausgewertet.

use std::collections::{BTreeMap, HashSet};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("PDF lässt sich nicht lesen: {0}")]
    Read(String),
    #[error("PDF lässt sich nicht schreiben: {0}")]
    Write(String),
    #[error("Das Formular enthält kein Feld „{0}“")]
    NoSuchField(String),
    #[error("Das Formular hat keine Seite {0}")]
    NoSuchPage(usize),
    #[error("Die Unterschrift (SVG) ist ungültig: {0}")]
    Signature(String),
    #[error("Das PDF hat keine Formularfelder")]
    NoForm,
}

pub type Result<T> = std::result::Result<T, PdfError>;

/// Wert für ein Formularfeld.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// Text für Textfelder und Auswahllisten.
    Text(String),
    /// Zustand für Checkboxen und Radiogruppen, zum Beispiel `Ja` oder `Off` (ohne Schrägstrich).
    State(String),
}

impl FieldValue {
    pub fn text(s: impl Into<String>) -> Self {
        FieldValue::Text(s.into())
    }

    /// Checkbox: `on` ist der Name des angekreuzten Zustands, sonst `Off`.
    pub fn check(checked: bool, on: &str) -> Self {
        FieldValue::State(if checked { on.to_string() } else { "Off".to_string() })
    }
}

/// Unterschrift, die in ein Rechteck der Seite eingepasst wird (PDF-Koordinaten, Ursprung unten links).
#[derive(Debug, Clone)]
pub struct Signature<'a> {
    pub svg: &'a str,
    /// Seite, 0-basiert.
    pub page: usize,
    /// `[x0, y0, x1, y1]`
    pub rect: [f32; 4],
}

// ----- Hilfen für PDF-Objekte ---------------------------------------------------------------------------

fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        // PDFDocEncoding stimmt für die hier vorkommenden Zeichen mit Latin-1 überein.
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn encode_pdf_string(text: &str) -> Vec<u8> {
    if text.chars().all(|c| (c as u32) < 256) {
        text.chars().map(|c| c as u8).collect()
    } else {
        let mut out = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_be_bytes());
        }
        out
    }
}

fn dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        _ => None,
    }
}

fn name_of(dict: &Dictionary) -> Option<String> {
    match dict.get(b"T").ok()? {
        Object::String(bytes, _) => Some(decode_pdf_string(bytes)),
        _ => None,
    }
}

fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn rect_of(doc: &Document, dict: &Dictionary) -> Option<[f32; 4]> {
    let array = match dict.get(b"Rect").ok()? {
        Object::Array(a) => a.clone(),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Array(a) => a.clone(),
            _ => return None,
        },
        _ => return None,
    };
    let values: Vec<f32> = array.iter().filter_map(number).collect();
    if values.len() != 4 {
        return None;
    }
    Some([values[0].min(values[2]), values[1].min(values[3]), values[0].max(values[2]), values[1].max(values[3])])
}

/// Ein Formularfeld: das Objekt mit dem Namen, seine Widgets und der vollständige Name.
struct Field {
    id: ObjectId,
    name: String,
    widgets: Vec<ObjectId>,
}

fn acroform_fields(doc: &Document) -> Result<Vec<ObjectId>> {
    let catalog = doc.catalog().map_err(|e| PdfError::Read(e.to_string()))?;
    let acro = catalog.get(b"AcroForm").map_err(|_| PdfError::NoForm)?;
    let acro = dict(doc, acro).ok_or(PdfError::NoForm)?;
    match acro.get(b"Fields").map_err(|_| PdfError::NoForm)? {
        Object::Array(items) => Ok(items.iter().filter_map(|o| o.as_reference().ok()).collect()),
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => Ok(items.iter().filter_map(|o| o.as_reference().ok()).collect()),
            _ => Err(PdfError::NoForm),
        },
        _ => Err(PdfError::NoForm),
    }
}

fn is_widget(dict: &Dictionary) -> bool {
    dict.get(b"Subtype").and_then(|o| o.as_name()).is_ok_and(|n| n == b"Widget")
}

fn collect_fields(doc: &Document, id: ObjectId, prefix: &str, seen: &mut HashSet<ObjectId>, out: &mut Vec<Field>) {
    if !seen.insert(id) {
        return;
    }
    let Ok(d) = doc.get_dictionary(id) else { return };
    let own = name_of(d);
    let full = match (&own, prefix.is_empty()) {
        (Some(n), true) => n.clone(),
        (Some(n), false) => format!("{prefix}.{n}"),
        (None, _) => prefix.to_string(),
    };
    let kids: Vec<ObjectId> = match d.get(b"Kids") {
        Ok(Object::Array(items)) => items.iter().filter_map(|o| o.as_reference().ok()).collect(),
        _ => Vec::new(),
    };
    // Kinder mit eigenem Namen sind Unterfelder, Kinder ohne Namen sind Widgets dieses Feldes.
    let (named, widgets): (Vec<_>, Vec<_>) = kids
        .into_iter()
        .partition(|k| doc.get_dictionary(*k).ok().is_some_and(|kd| name_of(kd).is_some()));
    if own.is_some() && (is_widget(d) || !widgets.is_empty() || named.is_empty()) {
        let mut w = widgets;
        if is_widget(d) {
            w.push(id);
        }
        out.push(Field { id, name: full.clone(), widgets: w });
    }
    for child in named {
        collect_fields(doc, child, &full, seen, out);
    }
}

fn all_fields(doc: &Document) -> Result<Vec<Field>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in acroform_fields(doc)? {
        collect_fields(doc, id, "", &mut seen, &mut out);
    }
    Ok(out)
}

/// Eigenschaft eines Feldes, auch von Vorfahren geerbt (`/FT`, `/Ff`, `/DA`, `/Q`).
fn inherited<'a>(doc: &'a Document, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..16 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(value) = d.get(key) {
            return Some(value);
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn field_type(doc: &Document, id: ObjectId) -> Option<Vec<u8>> {
    inherited(doc, id, b"FT").and_then(|o| o.as_name().ok()).map(<[u8]>::to_vec)
}

fn field_flags(doc: &Document, id: ObjectId) -> i64 {
    inherited(doc, id, b"Ff").and_then(|o| o.as_i64().ok()).unwrap_or(0)
}

const FLAG_MULTILINE: i64 = 1 << 12;
const FLAG_RADIO: i64 = 1 << 15;
const FLAG_PUSHBUTTON: i64 = 1 << 16;

fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}

/// Rechteck des ersten Widgets mit diesem Feldnamen auf der Seite (0-basiert): `[x0, y0, x1, y1]`.
pub fn field_rect(template: &[u8], name: &str, page: usize) -> Result<[f32; 4]> {
    let doc = Document::load_mem(template).map_err(|e| PdfError::Read(e.to_string()))?;
    let pages = page_ids(&doc);
    let page_id = *pages.get(page).ok_or(PdfError::NoSuchPage(page))?;
    for field in all_fields(&doc)? {
        if field.name != name {
            continue;
        }
        for widget in &field.widgets {
            let Ok(wd) = doc.get_dictionary(*widget) else { continue };
            let on_page = wd.get(b"P").ok().and_then(|p| p.as_reference().ok()).is_none_or(|p| p == page_id)
                && page_has_annot(&doc, page_id, *widget);
            if on_page {
                if let Some(rect) = rect_of(&doc, wd) {
                    return Ok(rect);
                }
            }
        }
    }
    Err(PdfError::NoSuchField(name.to_string()))
}

fn page_has_annot(doc: &Document, page: ObjectId, widget: ObjectId) -> bool {
    let Ok(pd) = doc.get_dictionary(page) else { return false };
    match pd.get(b"Annots") {
        Ok(Object::Array(items)) => items.iter().any(|o| o.as_reference().is_ok_and(|r| r == widget)),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => items.iter().any(|o| o.as_reference().is_ok_and(|r| r == widget)),
            _ => false,
        },
        _ => false,
    }
}

/// Namen aller Felder des Formulars (vollständige Namen).
pub fn field_names(template: &[u8]) -> Result<Vec<String>> {
    let doc = Document::load_mem(template).map_err(|e| PdfError::Read(e.to_string()))?;
    Ok(all_fields(&doc)?.into_iter().map(|f| f.name).collect())
}

/// Text des PDFs (alle Seiten). Das klappt nur bei PDFs mit Textinhalt und gängigen Schriften. Bei Bildern oder
/// ungewöhnlicher Kodierung kommt wenig oder nichts heraus.
pub fn extract_text(pdf: &[u8]) -> Result<String> {
    let doc = Document::load_mem(pdf).map_err(|e| PdfError::Read(e.to_string()))?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    doc.extract_text(&pages).map_err(|e| PdfError::Read(e.to_string()))
}

/// Aktuelle Werte aller Felder: Text als `Text`, Zustände als `State` (ohne Schrägstrich).
pub fn read_values(template: &[u8]) -> Result<BTreeMap<String, FieldValue>> {
    let doc = Document::load_mem(template).map_err(|e| PdfError::Read(e.to_string()))?;
    let mut out = BTreeMap::new();
    for field in all_fields(&doc)? {
        let value = inherited(&doc, field.id, b"V");
        match value {
            Some(Object::String(bytes, _)) => {
                out.insert(field.name, FieldValue::Text(decode_pdf_string(bytes)));
            }
            Some(Object::Name(n)) => {
                out.insert(field.name, FieldValue::State(String::from_utf8_lossy(n).to_string()));
            }
            _ => {}
        }
    }
    Ok(out)
}

// ----- Erscheinungsbild für Textfelder ------------------------------------------------------------------

/// Zeichen für die Standardschrift (WinAnsi, entspricht Windows-1252). Nicht darstellbares wird zu `?`.
fn winansi(c: char) -> u8 {
    match c {
        '\u{20}'..='\u{7E}' | '\u{A0}'..='\u{FF}' => c as u8,
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ => b'?',
    }
}

fn pdf_literal(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        match winansi(c) {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b => out.push(b),
        }
    }
    out.push(b')');
    out
}

/// Schriftname und -größe aus einer Standardangabe wie `/Helv 0 Tf 0 g`. Größe 0 bedeutet automatisch.
fn parse_da(da: &str) -> (String, f32) {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if *t == "Tf" && i >= 2 {
            return (tokens[i - 2].trim_start_matches('/').to_string(), tokens[i - 1].parse().unwrap_or(0.0));
        }
    }
    ("Helv".to_string(), 0.0)
}

/// Einfaches Erscheinungsbild für ein Textfeld, oder `None`, wenn es sich so nicht erzeugen lässt
/// (zentriert oder rechtsbündig braucht Schriftmaße). Dann zeichnet der Betrachter das Feld neu.
fn text_appearance(doc: &Document, field: &Field, widget: ObjectId, text: &str) -> Option<Stream> {
    let wd = doc.get_dictionary(widget).ok()?;
    let rect = rect_of(doc, wd)?;
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    if w < 4.0 || h < 4.0 {
        return None;
    }
    let quadding = inherited(doc, field.id, b"Q").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    if quadding != 0 {
        return None;
    }
    let da = inherited(doc, field.id, b"DA")
        .and_then(|o| o.as_str().ok())
        .map(decode_pdf_string)
        .or_else(|| {
            let acro = doc.catalog().ok()?.get(b"AcroForm").ok()?;
            let acro = dict(doc, acro)?;
            acro.get(b"DA").ok()?.as_str().ok().map(decode_pdf_string)
        })
        .unwrap_or_else(|| "/Helv 0 Tf 0 g".to_string());
    let (font, mut size) = parse_da(&da);
    let multiline = field_flags(doc, field.id) & FLAG_MULTILINE != 0;
    let lines: Vec<&str> = if multiline { text.split('\n').collect() } else { vec![text.lines().next().unwrap_or("")] };
    if size <= 0.0 {
        size = if multiline { 10.0 } else { (h - 4.0).clamp(6.0, 11.0) };
    }
    let leading = size * 1.15;
    let mut content = Vec::new();
    content.extend_from_slice(format!("/Tx BMC q 1 1 {} {} re W n BT {da} /{font} {size} Tf\n", w - 2.0, h - 2.0).as_bytes());
    // Der Dateiname der Schrift steht in `da`. Die Farbangabe daraus gilt weiter, die Schrift wird neu gesetzt.
    let first_baseline = if multiline { h - 2.0 - size } else { (h - size) / 2.0 + size * 0.22 };
    content.extend_from_slice(format!("2 {first_baseline:.2} Td {leading:.2} TL\n").as_bytes());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            content.extend_from_slice(b"T*\n");
        }
        content.extend_from_slice(&pdf_literal(line));
        content.extend_from_slice(b" Tj\n");
    }
    content.extend_from_slice(b"ET Q EMC");

    let mut resources = Dictionary::new();
    let acro_dr = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|a| dict(doc, a))
        .and_then(|a| a.get(b"DR").ok())
        .and_then(|d| dict(doc, d));
    if let Some(dr) = acro_dr {
        if let Ok(fonts) = dr.get(b"Font") {
            resources.set("Font", fonts.clone());
        }
    }
    let mut stream_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)],
    };
    stream_dict.set("Resources", Object::Dictionary(resources));
    Some(Stream::new(stream_dict, content))
}

// ----- Felder setzen ------------------------------------------------------------------------------------

fn set_text_value(doc: &mut Document, field: &Field, text: &str) {
    let appearances: Vec<(ObjectId, Option<Stream>)> = field
        .widgets
        .iter()
        .map(|w| (*w, text_appearance(doc, field, *w, text)))
        .collect();
    if let Ok(fd) = doc.get_dictionary_mut(field.id) {
        fd.set("V", Object::String(encode_pdf_string(text), StringFormat::Literal));
    }
    for (widget, stream) in appearances {
        match stream {
            Some(stream) => {
                let ap_id = doc.add_object(stream);
                if let Ok(wd) = doc.get_dictionary_mut(widget) {
                    wd.set("AP", dictionary! { "N" => Object::Reference(ap_id) });
                }
            }
            None => {
                // Ohne eigenes Erscheinungsbild: altes entfernen, der Betrachter zeichnet neu.
                if let Ok(wd) = doc.get_dictionary_mut(widget) {
                    wd.remove(b"AP");
                }
            }
        }
    }
}

fn set_state_value(doc: &mut Document, field: &Field, state: &str) {
    // Ein Widget zeigt den Zustand nur, wenn sein Erscheinungsbild ihn kennt.
    let known: Vec<(ObjectId, bool)> = field
        .widgets
        .iter()
        .map(|w| {
            let has = doc
                .get_dictionary(*w)
                .ok()
                .and_then(|wd| wd.get(b"AP").ok())
                .and_then(|ap| dict(doc, ap))
                .and_then(|ap| ap.get(b"N").ok())
                .and_then(|n| dict(doc, n))
                .is_some_and(|n| n.has(state.as_bytes()));
            (*w, has)
        })
        .collect();
    if let Ok(fd) = doc.get_dictionary_mut(field.id) {
        fd.set("V", Object::Name(state.as_bytes().to_vec()));
    }
    for (widget, has) in known {
        if let Ok(wd) = doc.get_dictionary_mut(widget) {
            let shown = if has { state } else { "Off" };
            wd.set("AS", Object::Name(shown.as_bytes().to_vec()));
        }
    }
}

/// Füllt ein Formular aus. Felder, die im Formular nicht vorkommen, werden ignoriert. Pushbuttons bleiben
/// unberührt. Gibt die PDF-Bytes zurück.
pub fn fill(template: &[u8], values: &BTreeMap<String, FieldValue>, signature: Option<&Signature<'_>>) -> Result<Vec<u8>> {
    let mut doc = Document::load_mem(template).map_err(|e| PdfError::Read(e.to_string()))?;
    let fields = all_fields(&doc)?;

    for field in &fields {
        let Some(value) = values.get(&field.name) else { continue };
        let ft = field_type(&doc, field.id);
        let flags = field_flags(&doc, field.id);
        match (ft.as_deref(), value) {
            (Some(b"Tx") | Some(b"Ch"), FieldValue::Text(text)) => set_text_value(&mut doc, field, text),
            (Some(b"Btn"), FieldValue::State(state)) if flags & FLAG_PUSHBUTTON == 0 => {
                // Radiogruppen und Checkboxen unterscheiden sich nur im Aufbau, nicht im Setzen.
                let _ = flags & FLAG_RADIO;
                set_state_value(&mut doc, field, state);
            }
            _ => {}
        }
    }

    // NeedAppearances: Betrachter zeichnen alle Felder neu.
    let acro_id = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|o| o.as_reference().ok());
    match acro_id {
        Some(id) => {
            if let Ok(d) = doc.get_dictionary_mut(id) {
                d.set("NeedAppearances", true);
            }
        }
        None => {
            let root = doc.trailer.get(b"Root").and_then(|r| r.as_reference()).map_err(|e| PdfError::Read(e.to_string()))?;
            if let Ok(catalog) = doc.get_dictionary_mut(root) {
                if let Ok(Object::Dictionary(d)) = catalog.get_mut(b"AcroForm") {
                    d.set("NeedAppearances", true);
                }
            }
        }
    }

    if let Some(sig) = signature {
        draw_signature(&mut doc, sig)?;
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| PdfError::Write(e.to_string()))?;
    Ok(out)
}

// ----- Unterschrift -------------------------------------------------------------------------------------

fn fmt(v: f32) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn paint_color(paint: &usvg::Paint) -> (f32, f32, f32) {
    match paint {
        usvg::Paint::Color(c) => (c.red as f32 / 255.0, c.green as f32 / 255.0, c.blue as f32 / 255.0),
        // Verläufe und Muster werden nicht ausgewertet: dunkles Grau als Ersatz.
        _ => (0.1, 0.1, 0.1),
    }
}

fn emit_group(group: &usvg::Group, out: &mut String) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => emit_group(g, out),
            usvg::Node::Path(path) => emit_path(path, out),
            _ => {}
        }
    }
}

fn emit_path(path: &usvg::Path, out: &mut String) {
    if !path.is_visible() {
        return;
    }
    let t = path.abs_transform();
    let map = |x: f32, y: f32| (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty);
    let mut data = String::new();
    // Aktueller Punkt in Seitenkoordinaten, für die Umrechnung quadratischer in kubische Kurven.
    let mut current = (0.0f32, 0.0f32);
    for segment in path.data().segments() {
        use usvg::tiny_skia_path::PathSegment as S;
        match segment {
            S::MoveTo(p) => {
                current = map(p.x, p.y);
                data.push_str(&format!("{} {} m\n", fmt(current.0), fmt(current.1)));
            }
            S::LineTo(p) => {
                current = map(p.x, p.y);
                data.push_str(&format!("{} {} l\n", fmt(current.0), fmt(current.1)));
            }
            S::QuadTo(c, p) => {
                let control = map(c.x, c.y);
                let end = map(p.x, p.y);
                let c1 = (current.0 + 2.0 / 3.0 * (control.0 - current.0), current.1 + 2.0 / 3.0 * (control.1 - current.1));
                let c2 = (end.0 + 2.0 / 3.0 * (control.0 - end.0), end.1 + 2.0 / 3.0 * (control.1 - end.1));
                data.push_str(&format!("{} {} {} {} {} {} c\n", fmt(c1.0), fmt(c1.1), fmt(c2.0), fmt(c2.1), fmt(end.0), fmt(end.1)));
                current = end;
            }
            S::CubicTo(c1, c2, p) => {
                let (x1, y1) = map(c1.x, c1.y);
                let (x2, y2) = map(c2.x, c2.y);
                current = map(p.x, p.y);
                data.push_str(&format!("{} {} {} {} {} {} c\n", fmt(x1), fmt(y1), fmt(x2), fmt(y2), fmt(current.0), fmt(current.1)));
            }
            S::Close => data.push_str("h\n"),
        }
    }
    let fill = path.fill();
    let stroke = path.stroke();
    if fill.is_none() && stroke.is_none() {
        return;
    }
    out.push_str("q\n");
    if let Some(f) = fill {
        let (r, g, b) = paint_color(f.paint());
        out.push_str(&format!("{} {} {} rg\n", fmt(r), fmt(g), fmt(b)));
    }
    if let Some(s) = stroke {
        let (r, g, b) = paint_color(s.paint());
        let scale = ((t.sx * t.sx + t.ky * t.ky).sqrt() + (t.kx * t.kx + t.sy * t.sy).sqrt()) / 2.0;
        out.push_str(&format!("{} {} {} RG {} w\n", fmt(r), fmt(g), fmt(b), fmt((s.width().get() * scale).max(0.01))));
        let cap = match s.linecap() {
            usvg::LineCap::Butt => 0,
            usvg::LineCap::Round => 1,
            usvg::LineCap::Square => 2,
        };
        let join = match s.linejoin() {
            usvg::LineJoin::Round => 1,
            usvg::LineJoin::Bevel => 2,
            _ => 0,
        };
        out.push_str(&format!("{cap} J {join} j\n"));
    }
    out.push_str(&data);
    let even_odd = fill.is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd);
    let op = match (fill.is_some(), stroke.is_some(), even_odd) {
        (true, true, false) => "B",
        (true, true, true) => "B*",
        (true, false, false) => "f",
        (true, false, true) => "f*",
        _ => "S",
    };
    out.push_str(op);
    out.push_str("\nQ\n");
}

fn draw_signature(doc: &mut Document, sig: &Signature<'_>) -> Result<()> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(sig.svg, &options).map_err(|e| PdfError::Signature(e.to_string()))?;
    let (svg_w, svg_h) = (tree.size().width(), tree.size().height());
    if svg_w <= 0.0 || svg_h <= 0.0 {
        return Err(PdfError::Signature("leere Zeichenfläche".into()));
    }
    let [x0, y0, x1, y1] = sig.rect;
    let (box_w, box_h) = (x1 - x0, y1 - y0);
    if box_w <= 0.0 || box_h <= 0.0 {
        return Err(PdfError::Signature("Zielrechteck ist leer".into()));
    }
    let scale = (box_w / svg_w).min(box_h / svg_h);
    // SVG hat den Ursprung oben links und y nach unten, PDF unten links und y nach oben.
    let tx = x0 + (box_w - svg_w * scale) / 2.0;
    let ty = y0 + (box_h - svg_h * scale) / 2.0 + svg_h * scale;

    let mut body = String::new();
    emit_group(tree.root(), &mut body);
    if body.is_empty() {
        return Err(PdfError::Signature("enthält keine Pfade".into()));
    }
    let content = format!("q\n{} 0 0 {} {} {} cm\n{body}Q\n", fmt(scale), fmt(-scale), fmt(tx), fmt(ty));

    let pages = page_ids(doc);
    let page_id = *pages.get(sig.page).ok_or(PdfError::NoSuchPage(sig.page))?;
    let stream_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
    let page = doc.get_dictionary_mut(page_id).map_err(|e| PdfError::Write(e.to_string()))?;
    let mut contents = match page.get(b"Contents") {
        Ok(Object::Array(items)) => items.clone(),
        Ok(other) => vec![other.clone()],
        Err(_) => Vec::new(),
    };
    contents.push(Object::Reference(stream_id));
    page.set("Contents", Object::Array(contents));
    Ok(())
}
