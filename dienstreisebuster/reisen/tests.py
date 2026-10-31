import datetime as dt
import shutil
import tempfile

from django.contrib.auth.models import User
from django.core.files.uploadedfile import SimpleUploadedFile
from django.test import TestCase, override_settings
from django.urls import reverse

from .models import Einstellungen, Reise

MEDIA = tempfile.mkdtemp()
SVG = b'<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><path d="M0 20 L100 20" stroke="black"/></svg>'
REISE = {
    "art": "dienstreise", "zweck": "Vortrag", "geschaeftsort": "Rohr", "taegliche_rueckkehr": "on",
    "reise_beginn": "2026-10-27T08:00", "dienstgeschaeft_beginn": "2026-10-27T13:00",
    "dienstgeschaeft_ende": "2026-10-27T16:00", "reise_ende": "2026-10-27T22:30",
    "abfahrt_von": "wohnort", "rueckkehr_nach": "wohnort", "bahn": "on",
}


@override_settings(MEDIA_ROOT=MEDIA)
class OberflaechenTest(TestCase):
    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(MEDIA, ignore_errors=True)
        super().tearDownClass()

    def setUp(self):
        User.objects.create_user("u", password="pw-123456")
        self.client.login(username="u", password="pw-123456")

    def _stammdaten(self):
        Einstellungen.objects.update_or_create(pk=1, defaults=dict(
            name="Muster", vorname="Max", plz="12345", ort="Stadt", strasse="Weg 1",
            dienststelle_kurz="Schule", personal_nr="1"))

    def test_anmeldung_noetig(self):
        self.client.logout()
        r = self.client.get(reverse("reisen:liste"))
        self.assertRedirects(r, "/konto/login/?next=/")

    def test_reise_anlegen_und_listen(self):
        r = self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        self.assertRedirects(r, reverse("reisen:detail", args=[reise.pk]))
        self.assertContains(self.client.get(reverse("reisen:liste")), "Vortrag")

    def test_zeitreihenfolge_wird_geprueft(self):
        falsch = {**REISE, "reise_ende": "2026-10-27T07:00"}
        r = self.client.post(reverse("reisen:neu"), falsch)
        self.assertEqual(r.status_code, 200)
        self.assertFalse(Reise.objects.exists())

    def test_antrag_erzeugen_braucht_stammdaten(self):
        self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        r = self.client.post(reverse("reisen:antrag_erzeugen", args=[reise.pk]))
        self.assertRedirects(r, reverse("reisen:einstellungen"))

    def test_antrag_erzeugen_und_abrufen(self):
        self._stammdaten()
        einst = Einstellungen.laden()
        einst.unterschrift_svg.save("u.svg", SimpleUploadedFile("u.svg", SVG))
        self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        self.client.post(reverse("reisen:antrag_erzeugen", args=[reise.pk]))
        reise.refresh_from_db()
        self.assertTrue(reise.antrag_pdf)
        r = self.client.get(reverse("reisen:datei", args=[reise.pk, "antrag"]))
        self.assertEqual(b"".join(r.streaming_content)[:5], b"%PDF-")

    def test_genehmigung_setzt_status(self):
        self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        pdf = SimpleUploadedFile("g.pdf", b"%PDF-1.4 test", content_type="application/pdf")
        self.client.post(reverse("reisen:genehmigung", args=[reise.pk]), {"genehmigung_pdf": pdf})
        reise.refresh_from_db()
        self.assertEqual(reise.status, Reise.Status.GENEHMIGT)

    def test_anhang_hinzufuegen_und_loeschen(self):
        self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        datei = SimpleUploadedFile("ticket.pdf", b"%PDF-1.4 t", content_type="application/pdf")
        self.client.post(reverse("reisen:anhang_neu", args=[reise.pk]),
                         {"art": "fahrkarte", "datei": datei, "beilegen": "on"})
        anhang = reise.anhaenge.get()
        self.client.post(reverse("reisen:anhang_loeschen", args=[anhang.pk]))
        self.assertFalse(reise.anhaenge.exists())

    def test_abrechnungsdaten_und_status(self):
        self.client.post(reverse("reisen:neu"), REISE)
        reise = Reise.objects.get()
        self.client.post(reverse("reisen:abrechnung", args=[reise.pk]),
                         {"fahrkartenpreis": "41,25", "verpflegung_von_mahlzeit": "", "verpflegung_bis_mahlzeit": ""})
        self.client.post(reverse("reisen:status", args=[reise.pk]), {"status": "bezahlt", "bezahlt_am": "2026-11-20"})
        reise.refresh_from_db()
        self.assertEqual(str(reise.fahrkartenpreis), "41.25")
        self.assertEqual(reise.bezahlt_am, dt.date(2026, 11, 20))

    def test_einstellungen_behalten_smtp_passwort(self):
        Einstellungen.objects.update_or_create(pk=1, defaults={"smtp_passwort": "geheim"})
        r = self.client.get(reverse("reisen:einstellungen"))
        self.assertNotContains(r, "geheim")
        data = {f: "" for f in ("name", "vorname")}
        data["smtp_port"] = "587"
        data["smtp_sicherheit"] = "starttls"
        self.client.post(reverse("reisen:einstellungen"), data)
        self.assertEqual(Einstellungen.laden().smtp_passwort, "geheim")


@override_settings(MEDIA_ROOT=MEDIA)
class VersandTest(TestCase):
    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(MEDIA, ignore_errors=True)
        super().tearDownClass()

    def setUp(self):
        User.objects.create_user("u", password="pw-123456")
        self.client.login(username="u", password="pw-123456")
        Einstellungen.objects.update_or_create(pk=1, defaults=dict(
            name="Muster", vorname="Max", plz="12345", ort="Stadt", strasse="Weg 1",
            dienststelle_kurz="Schule", personal_nr="1", iban="DE89370400440532013000",
            absender="max@example.org", empfaenger_antrag="verwaltung@example.org",
            empfaenger_abrechnung="lff@example.org", smtp_host="smtp.example.org"))
        self.client.post(reverse("reisen:neu"), REISE)
        self.reise = Reise.objects.get()

    def _antrag(self):
        self.client.post(reverse("reisen:antrag_erzeugen", args=[self.reise.pk]))

    def test_antrag_senden(self):
        from django.core import mail
        self._antrag()
        url = reverse("reisen:versand", args=[self.reise.pk, "antrag"])
        self.assertContains(self.client.get(url), "verwaltung@example.org")
        self.client.post(url, {"betreff": "Antrag", "text": "Hallo", "senden": "1"})
        self.assertEqual(len(mail.outbox), 1)
        nachricht = mail.outbox[0]
        self.assertEqual(nachricht.to, ["verwaltung@example.org"])
        self.assertEqual(nachricht.from_email, "max@example.org")
        self.assertEqual(len(nachricht.attachments), 1)
        self.reise.refresh_from_db()
        self.assertEqual(self.reise.status, Reise.Status.BEANTRAGT)
        self.assertIsNotNone(self.reise.antrag_versendet_am)

    def test_ohne_dokument_kein_versand(self):
        from django.core import mail
        r = self.client.post(reverse("reisen:versand", args=[self.reise.pk, "antrag"]), {"senden": "1"})
        self.assertRedirects(r, reverse("reisen:detail", args=[self.reise.pk]))
        self.assertEqual(len(mail.outbox), 0)

    def test_fehlende_smtp_einstellung(self):
        from django.core import mail
        Einstellungen.objects.filter(pk=1).update(smtp_host="")
        self._antrag()
        self.client.post(reverse("reisen:versand", args=[self.reise.pk, "antrag"]),
                         {"betreff": "A", "text": "B", "senden": "1"})
        self.assertEqual(len(mail.outbox), 0)
        self.reise.refresh_from_db()
        self.assertEqual(self.reise.status, Reise.Status.ENTWURF)
        self.assertIsNone(self.reise.antrag_versendet_am)

    def test_abrechnung_mit_anhaengen(self):
        from pathlib import Path
        from django.core import mail
        formular = Path(__file__).parent / "pdf" / "templates" / "r002_lff.pdf"
        self.client.post(reverse("reisen:abrechnung", args=[self.reise.pk]), {
            "fahrkartenpreis": "41,25", "verpflegung_von_mahlzeit": "", "verpflegung_bis_mahlzeit": "",
            "veranstalter_formular": SimpleUploadedFile("f.pdf", formular.read_bytes(), content_type="application/pdf"),
        })
        for name in ("ticket.pdf", "bescheinigung.pdf"):
            self.client.post(reverse("reisen:anhang_neu", args=[self.reise.pk]), {
                "art": "sonstiges", "datei": SimpleUploadedFile(name, b"%PDF-1.4 x", content_type="application/pdf"),
                "beilegen": "on"})
        self.client.post(reverse("reisen:abrechnung_erzeugen", args=[self.reise.pk]))
        self.reise.refresh_from_db()
        self.assertTrue(self.reise.abrechnung_pdf)
        url = reverse("reisen:versand", args=[self.reise.pk, "abrechnung"])
        anhang_ids = [f"anhang{a.pk}" for a in self.reise.anhaenge.all()]
        # Nur den ersten Anhang mitschicken
        self.client.post(url, {"betreff": "Abr", "text": "T", "anhang": anhang_ids[:1], "senden": "1"})
        nachricht = mail.outbox[0]
        self.assertEqual(nachricht.to, ["lff@example.org"])
        self.assertEqual(len(nachricht.attachments), 2)  # Abrechnung + ein Anhang
        self.reise.refresh_from_db()
        self.assertEqual(self.reise.status, Reise.Status.ABGERECHNET)

    def test_abrechnung_ohne_veranstalterformular(self):
        r = self.client.post(reverse("reisen:abrechnung_erzeugen", args=[self.reise.pk]), follow=True)
        self.assertContains(r, "Abrechnungsformular des Veranstalters")
        self.reise.refresh_from_db()
        self.assertFalse(self.reise.abrechnung_pdf)
