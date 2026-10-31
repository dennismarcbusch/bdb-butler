"""Erzeugt `fibs_form.pdf`: einen Nachbau eines Abrechnungsformulars im FIBS-Format für die Tests.

Es ist NICHT das echte Formular der Akademie, sondern ein zweiseitiges PDF mit denselben Feldnamen, die
`dienstreise::abrechnung_values` füllt, und einem vom „Veranstalter“ vorausgefüllten Feld (Aktenzeichen). Das echte
Formular liegt nicht im Repository.

Aufruf: /tmp/pyref/bin/python make_form_fixture.py
"""
from pathlib import Path

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

TEXT = [
    "PLZ", "Ort", "Telefon", "E-Mail", "BIC", "Beginn_der_Reise", "Ende_der_Reise", "Fahrkartenpreis", "Unterschrift_Ort",
    "Verpflegung_Begin_Datum", "Verpflegung_Ende_Datum", "Unterschrift_Datum",
    "IBAN_1", "IBAN_2", "IBAN_3", "IBAN_4", "IBAN_5", "IBAN_6",
]
CHECKS = [
    "Bahncard_Ja", "Bahncard_Nein", "Verpflegung_Ja", "Verpflegung_Nein", "Hotel_Selbstzahlung_Ja", "Hotel_Selbstzahlung_Nein",
] + [f"Verpflegung_{p}_{m}" for p in ("Begin", "Ende") for m in ("Fruehstueck", "Mittagessen", "Abendessen")]

out = Path(__file__).parent / "fibs_form.pdf"
c = canvas.Canvas(str(out), pagesize=A4)
form = c.acroForm
c.setFont("Helvetica-Bold", 14)
c.drawString(50, 800, "Nachbau: Reisekostenabrechnung (nur für Tests)")
c.setFont("Helvetica", 9)

y = 770
# Vom Veranstalter vorausgefüllt: bleibt unangetastet.
c.drawString(50, y + 12, "Aktenzeichen (vom Veranstalter)")
form.textfield(name="Aktenzeichen", value="AZ-2026/1234", x=50, y=y - 8, width=200, height=18, borderStyle="inset", forceBorder=True)
y -= 40
for name in TEXT[:9]:
    c.drawString(50, y + 12, name)
    form.textfield(name=name, x=50, y=y - 8, width=250, height=18, borderStyle="inset", forceBorder=True)
    y -= 36
c.drawString(320, 790, "Hinweise")
form.textfield(name="Abrechnungshinweise_Teilnehmer_multiline", x=320, y=640, width=220, height=140,
               fieldFlags="multiline", borderStyle="inset", forceBorder=True)
c.showPage()

c.setFont("Helvetica", 9)
y = 790
for name in TEXT[9:]:
    c.drawString(50, y + 12, name)
    form.textfield(name=name, x=50, y=y - 8, width=160, height=18, borderStyle="inset", forceBorder=True)
    y -= 34
x = 320
yy = 780
for name in CHECKS:
    c.drawString(x + 20, yy + 3, name)
    form.checkbox(name=name, x=x, y=yy, size=14, buttonStyle="check", borderStyle="solid", forceBorder=True)
    yy -= 24
c.showPage()
c.save()

# ReportLab nennt den angekreuzten Zustand „Yes“, das echte Formular „On“: umbenennen.
from pypdf import PdfReader, PdfWriter
from pypdf.generic import NameObject

writer = PdfWriter(clone_from=PdfReader(str(out)))
for page in writer.pages:
    for annot in page.get("/Annots", []):
        annot = annot.get_object()
        if annot.get("/FT") != "/Btn":
            continue
        for key in ("/N", "/D"):
            states = annot.get("/AP", {}).get(key)
            if states is not None and "/Yes" in states:
                states[NameObject("/On")] = states.pop("/Yes")
        if annot.get("/AS") == "/Yes":
            annot[NameObject("/AS")] = NameObject("/On")
        if annot.get("/V") == "/Yes":
            annot[NameObject("/V")] = NameObject("/On")
with out.open("wb") as f:
    writer.write(f)
print(out, out.stat().st_size)
