"""Feldzuordnung für den Dienstreiseantrag (R001, Stand 04/2019)."""
from .fuellen import VORLAGEN, feldrechteck, fuellen, schalter
from pypdf import PdfReader

VORLAGE = VORLAGEN / "r001_antrag.pdf"


def _datum(dt):
    return dt.strftime("%d.%m.%Y")


def _zeit(dt):
    return dt.strftime("%H:%M")


def _ort_radio(wahl):
    return {"wohnort": "/WOrt", "dienststelle": "/DOrt"}.get(wahl, "/Off")


def werte(reise, einst):
    """Feldwerte für den Antrag aus Reise und Einstellungen."""
    lokal = lambda dt: dt.astimezone()  # noqa: E731
    beginn, dg_beginn = lokal(reise.reise_beginn), lokal(reise.dienstgeschaeft_beginn)
    dg_ende, ende = lokal(reise.dienstgeschaeft_ende), lokal(reise.reise_ende)
    return {
        "S1 Antrag": "/Fortbildung" if reise.art == "fortbildung" else "/Dienstreise",
        "S1 Name": einst.name,
        "S1 Vorname": einst.vorname,
        "S1 PLZ": einst.plz,
        "S1 Ort": einst.ort,
        "S1 Str": einst.strasse,
        "S1 Beschäftigungsbehörde": einst.dienststelle_kurz,
        "S1 Abt": einst.abteilung,
        "S1 Entgeld": einst.besoldungsgruppe,
        "S1 Telefon": einst.telefon,
        "S1 E-Mail": einst.email,
        "S1 OrgNr": einst.org_nr,
        "S1 PersonalNr": einst.personal_nr,
        "S1 Zweck": reise.zweck,
        "S1 GeschOrt": reise.geschaeftsort,
        "S1 Rück": "/Ja" if reise.taegliche_rueckkehr else "/Nein",
        "S1Hin Ort": _ort_radio(reise.abfahrt_von),
        "S1 Hin andOrt": reise.abfahrt_von_text,
        "S1 Hin Antritt": _datum(beginn),
        "S1 Hin Uhrzeit": _zeit(beginn),
        "S1 Hin Beginn": _datum(dg_beginn),
        "S1 Hin Beginn Uhrzeit2": _zeit(dg_beginn),
        "S1R Ort": _ort_radio(reise.rueckkehr_nach),
        "S1 Rü andOrt": reise.rueckkehr_nach_text,
        "S1 Rü Antritt": _datum(ende),
        "S1 Rü Uhrzeit": _zeit(ende),
        "S1 Rü Beginn": _datum(dg_ende),
        # Das Feld heißt im Formular irreführend "Hin Beginn Uhrzeit", gehört aber zum Ende des Dienstgeschäfts.
        "S1 Hin Beginn Uhrzeit": _zeit(dg_ende),
        "S1 VorsÜKosten": reise.uebernachtungskosten or "- keine -",
        "S1 VorausSonst": reise.sonstige_kosten or "- keine -",
        "S1 Ersatz": reise.kostentraeger_dritte,
        "S1 Beförd 1": schalter(reise.oepnv or reise.bahn),  # "regelmäßig verkehrende Beförderungsmittel"
        "S1 Beförd 2": schalter(reise.oepnv),
        "S1 Beförd 4": schalter(reise.bahn),
        "S1 Beförd 5a": schalter(reise.bahn and reise.bahncard),
        "S1 Ort U1": einst.unterschrift_ort,
        "S1 Datum U1": _datum(reise.erstellt_am.astimezone()) if reise.erstellt_am else "",
    }


def erzeugen(reise, einst):
    svg = einst.unterschrift_svg.read() if einst.unterschrift_svg else None
    box = None
    if svg:
        x0, y0, x1, y1 = feldrechteck(PdfReader(VORLAGE), "S1 Unter U1")
        box = (x0, y0 - 6, x1, y1 + 14)
    return fuellen(VORLAGE, werte(reise, einst), svg, box)
