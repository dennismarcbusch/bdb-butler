"""Erzeugt die Referenz für die PDF-Tests aus der Python-Fassung des Dienstreisebusters.

Aufruf (nur Entwicklung, braucht pypdf, reportlab und svglib):
    python3 -m venv /tmp/pyref && /tmp/pyref/bin/pip install pypdf reportlab svglib
    /tmp/pyref/bin/python make_reference.py <Ordner für Referenz-PDFs>

Schreibt `reference.json` (Eingaben und erwartete Feldwerte) neben dieses Skript. Die Rust-Tests vergleichen ihre
Zuordnung damit. Die Referenz-PDFs dienen dem optischen Vergleich.
"""
import datetime as dt
import decimal
import json
import sys
from pathlib import Path
from types import SimpleNamespace as NS

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE.parent.parent.parent / "dienstreisebuster"))

from reisen.pdf import abrechnung, antrag  # noqa: E402

TZ = dt.timezone.utc

STAMMDATEN = {
    "name": "Mustermann", "vorname": "Max", "plz": "89073", "ort": "Ulm", "strasse": "Hauptstraße 1",
    "telefon": "0731 123456", "email": "max.mustermann@example.org",
    "dienststelle": "Musterschule\nSchulstraße 2\n89073 Ulm", "dienststelle_kurz": "Musterschule Ulm",
    "abteilung": "Fachbereich Digitale Bildung", "besoldungsgruppe": "A13", "org_nr": "12345", "personal_nr": "9876543",
    "iban": "DE89 3704 0044 0532 0130 00", "bic": "COBADEFFXXX", "kreditinstitut": "Commerzbank",
    "unterschrift_ort": "Ulm",
}

CASES = [
    {
        "name": "dienstreise_bahn_mehrtaegig",
        "reise": {
            "art": "dienstreise", "zweck": "Beratung Medienkonzept", "geschaeftsort": "Dillingen a. d. Donau",
            "taegliche_rueckkehr": False,
            "reise_beginn": "2026-11-16T06:45", "dienstgeschaeft_beginn": "2026-11-16T10:00",
            "dienstgeschaeft_ende": "2026-11-17T15:30", "reise_ende": "2026-11-17T18:20",
            "abfahrt_von": "wohnort", "abfahrt_von_text": "", "rueckkehr_nach": "wohnort", "rueckkehr_nach_text": "",
            "uebernachtungskosten": "", "sonstige_kosten": "", "kostentraeger_dritte": "",
            "oepnv": False, "bahn": True, "bahncard": True, "fahrkartenpreis": "123.45",
            "verpflegung_gestellt": True, "verpflegung_von": "2026-11-16", "verpflegung_von_mahlzeit": "mittagessen",
            "verpflegung_bis": "2026-11-17", "verpflegung_bis_mahlzeit": "fruehstueck",
            "hotel_selbst_bezahlt": True, "hinweise": "Zeile eins\nZeile zwei",
        },
    },
    {
        "name": "fortbildung_oepnv_dienststelle",
        "reise": {
            "art": "fortbildung", "zweck": "Fortbildung Künstliche Intelligenz", "geschaeftsort": "München",
            "taegliche_rueckkehr": True,
            "reise_beginn": "2027-01-12T07:05", "dienstgeschaeft_beginn": "2027-01-12T09:00",
            "dienstgeschaeft_ende": "2027-01-12T16:00", "reise_ende": "2027-01-12T17:40",
            "abfahrt_von": "dienststelle", "abfahrt_von_text": "", "rueckkehr_nach": "anderer",
            "rueckkehr_nach_text": "Augsburg, Hauptbahnhof",
            "uebernachtungskosten": "80 €", "sonstige_kosten": "12,50 € Parken", "kostentraeger_dritte": "Akademie Dillingen",
            "oepnv": True, "bahn": False, "bahncard": False, "fahrkartenpreis": None,
            "verpflegung_gestellt": False, "verpflegung_von": None, "verpflegung_von_mahlzeit": "",
            "verpflegung_bis": None, "verpflegung_bis_mahlzeit": "",
            "hotel_selbst_bezahlt": False, "hinweise": "",
        },
    },
]
TODAY = dt.date(2026, 10, 31)


def parse_dt(text):
    return dt.datetime.strptime(text, "%Y-%m-%dT%H:%M").replace(tzinfo=TZ) if text else None


def parse_date(text):
    return dt.date.fromisoformat(text) if text else None


def python_reise(r):
    obj = NS(**r)
    for f in ("reise_beginn", "dienstgeschaeft_beginn", "dienstgeschaeft_ende", "reise_ende"):
        setattr(obj, f, parse_dt(r[f]))
    obj.verpflegung_von = parse_date(r["verpflegung_von"])
    obj.verpflegung_bis = parse_date(r["verpflegung_bis"])
    obj.fahrkartenpreis = decimal.Decimal(r["fahrkartenpreis"]) if r["fahrkartenpreis"] else None
    obj.erstellt_am = dt.datetime.combine(TODAY, dt.time(12, 0), tzinfo=TZ)
    return obj


def main(out_dir):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    svg = (HERE / "unterschrift.svg").read_bytes()
    einst = NS(**STAMMDATEN, unterschrift_svg=NS(read=lambda: svg))
    cases = []
    for case in CASES:
        reise = python_reise(case["reise"])
        antrag_werte = antrag.werte(reise, einst)
        abrechnung_werte = abrechnung.werte(reise, einst)
        abrechnung_werte["Unterschrift_Datum"] = TODAY.strftime("%d.%m.%Y")
        cases.append({
            "name": case["name"], "reise": case["reise"], "stammdaten": STAMMDATEN, "today": TODAY.isoformat(),
            "antrag": antrag_werte, "abrechnung": abrechnung_werte,
        })
        (out / f"antrag_{case['name']}.pdf").write_bytes(antrag.erzeugen(reise, einst))
    (HERE / "reference.json").write_text(json.dumps({"cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{len(cases)} Fälle, Referenz-PDFs in {out}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "/tmp/ref-pdfs")
