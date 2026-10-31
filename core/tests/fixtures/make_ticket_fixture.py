"""Erzeugt `ticket_sample.pdf`: ein ERFUNDENES Beispiel einer Fahrkarte als PDF mit Text, für die Tests der
Preiserkennung. Es ist keine echte Fahrkarte und bildet das Layout eines Anbieters nicht nach.

Aufruf: /tmp/pyref/bin/python make_ticket_fixture.py
"""
from pathlib import Path

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

out = Path(__file__).parent / "ticket_sample.pdf"
c = canvas.Canvas(str(out), pagesize=A4)
c.setFont("Helvetica-Bold", 14)
c.drawString(50, 790, "Beispiel-Fahrkarte (nur für Tests)")
c.setFont("Helvetica", 11)
lines = [
    "Hinfahrt: Ulm Hbf - Muenchen Hbf, 16.11.2026, 07:12",
    "Rueckfahrt: Muenchen Hbf - Ulm Hbf, 17.11.2026, 18:20",
    "2. Klasse, 1 Erwachsener",
    "Preis Hinfahrt: 19,90 EUR",
    "Preis Rueckfahrt: 19,90 EUR",
    "BahnCard 25 Rabatt: -9,95 EUR",
    "Gesamtpreis: 29,85 EUR",
    "inkl. 19% MwSt. 4,77 EUR",
]
y = 750
for line in lines:
    c.drawString(50, y, line)
    y -= 20
c.showPage()
c.save()
print(out, out.stat().st_size)
