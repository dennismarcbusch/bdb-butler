from django.db import models


class Einstellungen(models.Model):
    """Stammdaten, Empfänger, SMTP und Unterschrift. Es gibt genau einen Datensatz."""

    name = models.CharField("Name", max_length=100, blank=True)
    vorname = models.CharField("Vorname", max_length=100, blank=True)
    plz = models.CharField("PLZ", max_length=10, blank=True)
    ort = models.CharField("Ort", max_length=100, blank=True)
    strasse = models.CharField("Straße", max_length=150, blank=True)
    telefon = models.CharField("Telefon", max_length=50, blank=True)
    email = models.EmailField("E-Mail im Formular", blank=True)

    dienststelle = models.TextField("Beschäftigungsbehörde (Name, Adresse)", blank=True)
    dienststelle_kurz = models.CharField("Beschäftigungsbehörde (Kurzname)", max_length=150, blank=True)
    abteilung = models.CharField("Sachgebiet/Referat/Abteilung", max_length=100, blank=True)
    besoldungsgruppe = models.CharField("Bes.-/Entgeltgruppe", max_length=20, blank=True)
    org_nr = models.CharField("Organisationsnummer", max_length=20, blank=True)
    personal_nr = models.CharField("Personalnummer (VIVA)", max_length=20, blank=True)

    iban = models.CharField("IBAN", max_length=34, blank=True)
    bic = models.CharField("BIC", max_length=11, blank=True)
    kreditinstitut = models.CharField("Kreditinstitut", max_length=100, blank=True)

    unterschrift_ort = models.CharField("Ort für Unterschrift", max_length=100, blank=True)
    unterschrift_svg = models.FileField("Unterschrift (SVG)", upload_to="unterschrift/", blank=True)

    empfaenger_antrag = models.EmailField("Empfänger Dienstreiseantrag (Verwaltung)", blank=True)
    empfaenger_abrechnung = models.EmailField("Empfänger Abrechnung (LfF)", blank=True)
    absender = models.EmailField("Absenderadresse", blank=True)

    smtp_host = models.CharField("SMTP-Server", max_length=200, blank=True)
    smtp_port = models.PositiveIntegerField("SMTP-Port", default=587)
    smtp_user = models.CharField("SMTP-Benutzer", max_length=200, blank=True)
    smtp_passwort = models.CharField("SMTP-Passwort", max_length=200, blank=True)
    smtp_sicherheit = models.CharField(
        "Verschlüsselung",
        max_length=8,
        choices=[("starttls", "STARTTLS"), ("ssl", "SSL/TLS"), ("keine", "keine")],
        default="starttls",
    )

    class Meta:
        verbose_name = "Einstellungen"
        verbose_name_plural = "Einstellungen"

    def __str__(self):
        return "Einstellungen"

    def save(self, *args, **kwargs):
        self.pk = 1
        super().save(*args, **kwargs)

    @classmethod
    def laden(cls):
        return cls.objects.get_or_create(pk=1)[0]


class Reise(models.Model):
    class Status(models.TextChoices):
        ENTWURF = "entwurf", "Entwurf"
        BEANTRAGT = "beantragt", "Beantragt"
        GENEHMIGT = "genehmigt", "Genehmigt"
        ABGERECHNET = "abgerechnet", "Abgerechnet"
        BEZAHLT = "bezahlt", "Bezahlt"

    class Art(models.TextChoices):
        DIENSTREISE = "dienstreise", "Dienstreise"
        FORTBILDUNG = "fortbildung", "Fortbildungsreise"

    class Ort(models.TextChoices):
        WOHNORT = "wohnort", "Wohnort"
        DIENSTSTELLE = "dienststelle", "Dienststelle"
        ANDERER = "anderer", "anderer Ort"

    status = models.CharField(max_length=12, choices=Status.choices, default=Status.ENTWURF)
    art = models.CharField("Art der Reise", max_length=12, choices=Art.choices, default=Art.DIENSTREISE)
    zweck = models.CharField("Zweck / Titel", max_length=300)
    geschaeftsort = models.CharField("Geschäftsort", max_length=300)
    taegliche_rueckkehr = models.BooleanField("Tägliche Rückkehr möglich", default=False)

    reise_beginn = models.DateTimeField("Reiseantritt")
    dienstgeschaeft_beginn = models.DateTimeField("Beginn Dienstgeschäft")
    dienstgeschaeft_ende = models.DateTimeField("Ende Dienstgeschäft")
    reise_ende = models.DateTimeField("Beendigung der Reise")
    abfahrt_von = models.CharField(max_length=12, choices=Ort.choices, default=Ort.WOHNORT)
    abfahrt_von_text = models.CharField("anderer Abfahrtsort", max_length=150, blank=True)
    rueckkehr_nach = models.CharField(max_length=12, choices=Ort.choices, default=Ort.WOHNORT)
    rueckkehr_nach_text = models.CharField("anderer Rückkehrort", max_length=150, blank=True)

    uebernachtungskosten = models.CharField("Voraussichtliche Übernachtungskosten", max_length=150, blank=True)
    sonstige_kosten = models.CharField("Voraussichtliche sonstige Kosten", max_length=150, blank=True)
    kostentraeger_dritte = models.CharField("Ersatz der Reisekosten durch Dritte", max_length=300, blank=True)

    oepnv = models.BooleanField("Öffentlicher Nahverkehr", default=False)
    bahn = models.BooleanField("Deutsche Bahn", default=True)
    bahncard = models.BooleanField("BahnCard vorhanden", default=False)
    fahrkartenpreis = models.DecimalField("Fahrkartenpreis (€)", max_digits=8, decimal_places=2, null=True, blank=True)

    verpflegung_gestellt = models.BooleanField("Verpflegung unentgeltlich gestellt", default=False)
    verpflegung_von = models.DateField("Verpflegung ab", null=True, blank=True)
    verpflegung_von_mahlzeit = models.CharField(max_length=12, blank=True)
    verpflegung_bis = models.DateField("Verpflegung bis", null=True, blank=True)
    verpflegung_bis_mahlzeit = models.CharField(max_length=12, blank=True)
    hotel_selbst_bezahlt = models.BooleanField("Unterkunft selbst bezahlt", default=False)
    hinweise = models.TextField("Ergänzende Hinweise", blank=True)

    aktenzeichen = models.CharField(max_length=100, blank=True)
    veranstalter_formular = models.FileField(
        "Vorausgefülltes Veranstalter-Formular", upload_to="veranstalter/", blank=True
    )
    antrag_pdf = models.FileField(upload_to="antraege/", blank=True)
    genehmigung_pdf = models.FileField(upload_to="genehmigungen/", blank=True)
    abrechnung_pdf = models.FileField(upload_to="abrechnungen/", blank=True)

    antrag_versendet_am = models.DateTimeField(null=True, blank=True)
    abrechnung_versendet_am = models.DateTimeField(null=True, blank=True)
    bezahlt_am = models.DateField("Bezahlt am", null=True, blank=True)
    erstellt_am = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["-reise_beginn"]
        verbose_name_plural = "Reisen"

    def __str__(self):
        return f"{self.reise_beginn:%d.%m.%Y} {self.zweck}"


class Anhang(models.Model):
    class Art(models.TextChoices):
        FAHRKARTE = "fahrkarte", "Fahrkarte"
        TEILNAHME = "teilnahme", "Teilnahmebescheinigung"
        HOTEL = "hotel", "Hotelrechnung"
        SONSTIGES = "sonstiges", "Sonstiges"

    reise = models.ForeignKey(Reise, on_delete=models.CASCADE, related_name="anhaenge")
    art = models.CharField(max_length=12, choices=Art.choices, default=Art.SONSTIGES)
    datei = models.FileField(upload_to="anhaenge/")
    beilegen = models.BooleanField("Der Abrechnung beilegen", default=True)

    class Meta:
        verbose_name_plural = "Anhänge"

    def __str__(self):
        return f"{self.get_art_display()}: {self.datei.name}"
