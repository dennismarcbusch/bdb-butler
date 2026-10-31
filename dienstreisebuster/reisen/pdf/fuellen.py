"""Ausfüllen der AcroForm-PDFs und Aufsetzen der Unterschrift."""
import io
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import NameObject, TextStringObject
from reportlab.graphics import renderPDF
from reportlab.pdfgen import canvas
from svglib.svglib import svg2rlg

VORLAGEN = Path(__file__).parent / "templates"


def feldrechteck(reader, name, seite=0):
    """Rechteck (x0, y0, x1, y1) des ersten Widgets mit diesem Feldnamen."""
    for annot in reader.pages[seite].get("/Annots", []):
        annot = annot.get_object()
        if annot.get("/T") == name:
            return [float(v) for v in annot["/Rect"]]
    raise KeyError(name)


def unterschrift_overlay(svg_bytes, seitengroesse, box):
    """Einseitiges PDF mit der Unterschrift, eingepasst in box=(x0, y0, x1, y1)."""
    zeichnung = svg2rlg(io.BytesIO(svg_bytes))
    x0, y0, x1, y1 = box
    faktor = min((x1 - x0) / zeichnung.width, (y1 - y0) / zeichnung.height)
    zeichnung.scale(faktor, faktor)
    zeichnung.width *= faktor
    zeichnung.height *= faktor
    puffer = io.BytesIO()
    c = canvas.Canvas(puffer, pagesize=seitengroesse)
    renderPDF.draw(zeichnung, c, x0 + ((x1 - x0) - zeichnung.width) / 2, y0 + ((y1 - y0) - zeichnung.height) / 2)
    c.save()
    puffer.seek(0)
    return PdfReader(puffer).pages[0]


def _knoepfe_setzen(seite, werte):
    """Checkboxen und Radiogruppen: /V am Feld und /AS an jedem Widget setzen.

    Gruppenname steht am Parent, Einzelfelder tragen /T selbst. Ein Widget zeigt den Zustand nur,
    wenn dieser in seinem Appearance-Dictionary (/AP /N) vorkommt, sonst /Off.
    """
    for annot in seite.get("/Annots", []):
        annot = annot.get_object()
        feld = annot.get("/Parent").get_object() if annot.get("/Parent") and "/T" not in annot else annot
        name = feld.get("/T")
        if name not in werte or str(werte[name])[:1] != "/":
            continue
        zustand = werte[name]
        stile = annot.get("/AP", {}).get("/N", {})
        annot[NameObject("/AS")] = NameObject(zustand if zustand in stile else "/Off")
        feld[NameObject("/V")] = NameObject(zustand)


def _felder_setzen(writer, seite, werte):
    """Setzt die Feldwerte einer Seite. Widgets ohne Appearance-Stream (kommt bei Veranstalter-
    Formularen vor) bekommen den Wert direkt gesetzt und werden vom Viewer neu gezeichnet."""
    _knoepfe_setzen(seite, werte)
    texte = {k: v for k, v in werte.items() if str(v)[:1] != "/"}
    try:
        writer.update_page_form_field_values(seite, texte, auto_regenerate=False)
        return
    except KeyError:
        pass
    for name, wert in texte.items():
        try:
            writer.update_page_form_field_values(seite, {name: wert}, auto_regenerate=False)
        except KeyError:
            for annot in seite.get("/Annots", []):
                annot = annot.get_object()
                if annot.get("/T") == name:
                    annot[NameObject("/V")] = TextStringObject(wert)


def fuellen(vorlage, werte, unterschrift_svg=None, unterschrift_box=None, unterschrift_seite=0):
    """Füllt ein Formular aus und gibt die PDF-Bytes zurück.

    werte: {Feldname: Text | "/Zustand" für Radiogruppen | True/False für Checkboxen (in
    "/Zustand" umzusetzen)}. Felder, die nicht im Formular vorkommen, werden ignoriert.
    """
    reader = PdfReader(vorlage)
    writer = PdfWriter(clone_from=reader)
    for seite in writer.pages:
        _felder_setzen(writer, seite, werte)
    writer.set_need_appearances_writer(True)

    if unterschrift_svg and unterschrift_box:
        seite = writer.pages[unterschrift_seite]
        overlay = unterschrift_overlay(
            unterschrift_svg, (float(seite.mediabox.width), float(seite.mediabox.height)), unterschrift_box
        )
        seite.merge_page(overlay)

    ausgabe = io.BytesIO()
    writer.write(ausgabe)
    return ausgabe.getvalue()


def schalter(wert, an="/Ja"):
    """Checkbox-Zustand: an oder /Off."""
    return an if wert else "/Off"
