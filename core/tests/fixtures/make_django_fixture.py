"""Erzeugt `django/` (Datenbank und Dateien) mit dem ALTEN Dienstreisebuster als Testdaten für den Import.

Die Daten sind erfunden. Aufruf (nur Entwicklung):
    python3 -m venv /tmp/djvenv && /tmp/djvenv/bin/pip install -r ../../../dienstreisebuster/requirements.txt
    /tmp/djvenv/bin/python make_django_fixture.py

Der Code des alten Tools steht nicht mehr im Arbeitsbaum. Zum Neuerzeugen vorher auschecken:
    git worktree add /tmp/dienstreisebuster-alt 9e561ca && ln -s /tmp/dienstreisebuster-alt/dienstreisebuster ../../../dienstreisebuster
"""
import datetime as dt
import decimal
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).parent
TARGET = HERE / "django"
shutil.rmtree(TARGET, ignore_errors=True)
TARGET.mkdir()
os.environ["DATA_DIR"] = str(TARGET)
os.environ["DEBUG"] = "1"
os.environ.setdefault("DJANGO_SETTINGS_MODULE", "config.settings")
sys.path.insert(0, str(HERE.parent.parent.parent / "dienstreisebuster"))

import django  # noqa: E402

django.setup()
from django.core.files.base import ContentFile  # noqa: E402
from django.core.management import call_command  # noqa: E402

call_command("migrate", verbosity=0)

from reisen.models import Anhang, Einstellungen, Reise  # noqa: E402

UTC = dt.timezone.utc
einst = Einstellungen.laden()
for name, value in dict(
    name="Mustermann", vorname="Max", plz="89073", ort="Ulm", strasse="Hauptstraße 1", telefon="0731 123456",
    email="max.mustermann@example.org", dienststelle="Musterschule\nSchulstraße 2\n89073 Ulm",
    dienststelle_kurz="Musterschule Ulm", abteilung="Fachbereich Digitale Bildung", besoldungsgruppe="A13",
    org_nr="12345", personal_nr="9876543", iban="DE89 3704 0044 0532 0130 00", bic="COBADEFFXXX",
    kreditinstitut="Commerzbank", unterschrift_ort="Ulm", empfaenger_antrag="verwaltung@example.org",
    empfaenger_abrechnung="lff@example.org", absender="max.mustermann@example.org", smtp_host="smtp.example.org",
    smtp_port=465, smtp_user="max", smtp_passwort="altes-passwort", smtp_sicherheit="ssl",
).items():
    setattr(einst, name, value)
einst.unterschrift_svg.save("unterschrift.svg", ContentFile((HERE / "unterschrift.svg").read_bytes()), save=False)
einst.save()

winter = Reise.objects.create(
    status="bezahlt", art="fortbildung", zweck="Fortbildung Künstliche Intelligenz", geschaeftsort="München",
    taegliche_rueckkehr=False,
    reise_beginn=dt.datetime(2026, 11, 16, 5, 45, tzinfo=UTC),  # 06:45 Ortszeit (Winterzeit, UTC+1)
    dienstgeschaeft_beginn=dt.datetime(2026, 11, 16, 9, 0, tzinfo=UTC),
    dienstgeschaeft_ende=dt.datetime(2026, 11, 17, 14, 30, tzinfo=UTC),
    reise_ende=dt.datetime(2026, 11, 17, 17, 20, tzinfo=UTC),
    abfahrt_von="dienststelle", rueckkehr_nach="anderer", rueckkehr_nach_text="Augsburg, Hauptbahnhof",
    uebernachtungskosten="80 €", sonstige_kosten="12,50 € Parken", kostentraeger_dritte="Akademie Dillingen",
    oepnv=False, bahn=True, bahncard=True, fahrkartenpreis=decimal.Decimal("123.45"),
    verpflegung_gestellt=True, verpflegung_von=dt.date(2026, 11, 16), verpflegung_von_mahlzeit="mittagessen",
    verpflegung_bis=dt.date(2026, 11, 17), verpflegung_bis_mahlzeit="fruehstueck", hotel_selbst_bezahlt=True,
    hinweise="Zeile eins\nZeile zwei", aktenzeichen="AZ-2026/1234",
    antrag_versendet_am=dt.datetime(2026, 10, 1, 7, 30, tzinfo=UTC),
    abrechnung_versendet_am=dt.datetime(2026, 11, 20, 12, 0, tzinfo=UTC), bezahlt_am=dt.date(2026, 12, 1),
)
winter.antrag_pdf.save("antrag-2026-11-16.pdf", ContentFile(b"%PDF-1.4 antrag"), save=False)
winter.genehmigung_pdf.save("genehmigung.pdf", ContentFile(b"%PDF-1.4 genehmigung"), save=False)
winter.abrechnung_pdf.save("abrechnung-2026-11-16.pdf", ContentFile(b"%PDF-1.4 abrechnung"), save=False)
winter.veranstalter_formular.save("fibs_form.pdf", ContentFile((HERE / "fibs_form.pdf").read_bytes()), save=False)
winter.save()
Anhang.objects.create(reise=winter, art="fahrkarte", beilegen=True).datei.save("ticket.pdf", ContentFile(b"%PDF-1.4 ticket"))
Anhang.objects.create(reise=winter, art="hotel", beilegen=False).datei.save("hotel rechnung.pdf", ContentFile(b"%PDF-1.4 hotel"))

# Sommerzeit (UTC+2) und ein Entwurf ohne Dateien.
Reise.objects.create(
    zweck="Beratung vor Ort", geschaeftsort="Neu-Ulm", taegliche_rueckkehr=True,
    reise_beginn=dt.datetime(2026, 7, 14, 6, 0, tzinfo=UTC),  # 08:00 Ortszeit
    dienstgeschaeft_beginn=dt.datetime(2026, 7, 14, 7, 0, tzinfo=UTC),
    dienstgeschaeft_ende=dt.datetime(2026, 7, 14, 13, 0, tzinfo=UTC),
    reise_ende=dt.datetime(2026, 7, 14, 14, 0, tzinfo=UTC),
    oepnv=True, bahn=False,
)
print("Reisen:", Reise.objects.count(), "Anhänge:", Anhang.objects.count())
