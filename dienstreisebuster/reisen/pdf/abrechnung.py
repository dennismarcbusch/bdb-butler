"""Feldzuordnung für die Abrechnung im FIBS-Format (Akademie für Lehrerfortbildung, Stand 04/2025).

Das Formular kommt vorausgefüllt vom Veranstalter (Aktenzeichen, Titel, Zeitraum, Buchungsstelle).
Diese Felder bleiben unangetastet; ergänzt werden nur Angaben aus Reise und Einstellungen.
"""
from pypdf import PdfReader

from .fuellen import feldrechteck, fuellen, schalter


def _dezimal(wert):
    return f"{wert:.2f}".replace(".", ",")


def _iban_teile(iban):
    iban = "".join(iban.split())
    teile = [iban[:4], iban[4:8], iban[8:12], iban[12:16], iban[16:20], iban[20:]]
    return {f"IBAN_{i}": t for i, t in enumerate(teile, start=1)}


def werte(reise, einst):
    ab = reise.reise_beginn.astimezone()
    bis = reise.reise_ende.astimezone()
    w = {
        "PLZ": einst.plz,
        "Ort": einst.ort,
        "Telefon": einst.telefon,
        "E-Mail": einst.email,
        "BIC": einst.bic,
        "Beginn_der_Reise": ab.strftime("%d.%m.%Y %H:%M"),
        "Ende_der_Reise": bis.strftime("%d.%m.%Y %H:%M"),
        "Bahncard_Ja": schalter(reise.bahncard, "/On"),
        "Bahncard_Nein": schalter(not reise.bahncard, "/On"),
        "Verpflegung_Ja": schalter(reise.verpflegung_gestellt, "/On"),
        "Verpflegung_Nein": schalter(not reise.verpflegung_gestellt, "/On"),
        "Hotel_Selbstzahlung_Ja": schalter(reise.hotel_selbst_bezahlt, "/On"),
        "Hotel_Selbstzahlung_Nein": schalter(not reise.hotel_selbst_bezahlt, "/On"),
        "Abrechnungshinweise_Teilnehmer_multiline": reise.hinweise,
        "Unterschrift_Ort": einst.unterschrift_ort,
    }
    w.update(_iban_teile(einst.iban))
    if reise.fahrkartenpreis is not None:
        w["Fahrkartenpreis"] = _dezimal(reise.fahrkartenpreis)
    if reise.verpflegung_gestellt and reise.verpflegung_von and reise.verpflegung_bis:
        w["Verpflegung_Begin_Datum"] = reise.verpflegung_von.strftime("%d.%m.%Y")
        w["Verpflegung_Ende_Datum"] = reise.verpflegung_bis.strftime("%d.%m.%Y")
        for praefix, mahlzeit in (("Begin", reise.verpflegung_von_mahlzeit), ("Ende", reise.verpflegung_bis_mahlzeit)):
            for name in ("Fruehstueck", "Mittagessen", "Abendessen"):
                w[f"Verpflegung_{praefix}_{name}"] = schalter(name.lower().startswith(mahlzeit[:4]), "/On")
    return w


def erzeugen(formular, reise, einst, datum):
    """formular: Datei oder Pfad des vom Veranstalter vorausgefüllten PDFs."""
    w = werte(reise, einst)
    w["Unterschrift_Datum"] = datum.strftime("%d.%m.%Y")
    svg = einst.unterschrift_svg.read() if einst.unterschrift_svg else None
    box = None
    if svg:
        reader = PdfReader(formular)
        x0, y0, x1, y1 = feldrechteck(reader, "Unterschrift_Datum", seite=1)
        # Unterschriftsbereich liegt rechts neben dem Datumsfeld.
        box = (x1 + 20, y0 - 8, x1 + 20 + 180, y1 + 16)
    return fuellen(formular, w, svg, box, unterschrift_seite=1)
