from django import forms

from .models import Anhang, Einstellungen, Reise

DATETIME = "%Y-%m-%dT%H:%M"
MAHLZEITEN = [("", "–"), ("fruehstueck", "Frühstück"), ("mittagessen", "Mittagessen"), ("abendessen", "Abendessen")]


def _datetime_feld(label):
    return forms.DateTimeField(
        label=label,
        input_formats=[DATETIME],
        widget=forms.DateTimeInput(attrs={"type": "datetime-local"}, format=DATETIME),
    )


class ReiseForm(forms.ModelForm):
    reise_beginn = _datetime_feld("Reiseantritt")
    dienstgeschaeft_beginn = _datetime_feld("Beginn Dienstgeschäft")
    dienstgeschaeft_ende = _datetime_feld("Ende Dienstgeschäft")
    reise_ende = _datetime_feld("Beendigung der Reise")

    class Meta:
        model = Reise
        fields = [
            "art", "zweck", "geschaeftsort", "taegliche_rueckkehr",
            "reise_beginn", "dienstgeschaeft_beginn", "dienstgeschaeft_ende", "reise_ende",
            "abfahrt_von", "abfahrt_von_text", "rueckkehr_nach", "rueckkehr_nach_text",
            "oepnv", "bahn", "bahncard",
            "uebernachtungskosten", "sonstige_kosten", "kostentraeger_dritte",
        ]

    def clean(self):
        daten = super().clean()
        reihenfolge = ["reise_beginn", "dienstgeschaeft_beginn", "dienstgeschaeft_ende", "reise_ende"]
        zeiten = [daten.get(k) for k in reihenfolge]
        if all(zeiten) and zeiten != sorted(zeiten):
            raise forms.ValidationError(
                "Die Zeiten müssen in der Reihenfolge Reiseantritt, Beginn, Ende Dienstgeschäft, "
                "Beendigung der Reise liegen."
            )
        return daten


class AbrechnungForm(forms.ModelForm):
    verpflegung_von_mahlzeit = forms.ChoiceField(label="erste Mahlzeit", choices=MAHLZEITEN, required=False)
    verpflegung_bis_mahlzeit = forms.ChoiceField(label="letzte Mahlzeit", choices=MAHLZEITEN, required=False)

    class Meta:
        model = Reise
        fields = [
            "fahrkartenpreis", "bahncard",
            "verpflegung_gestellt", "verpflegung_von", "verpflegung_von_mahlzeit",
            "verpflegung_bis", "verpflegung_bis_mahlzeit",
            "hotel_selbst_bezahlt", "hinweise", "aktenzeichen", "veranstalter_formular",
        ]
        localized_fields = ["fahrkartenpreis"]  # Eingabe mit Dezimalkomma
        widgets = {
            "verpflegung_von": forms.DateInput(attrs={"type": "date"}, format="%Y-%m-%d"),
            "verpflegung_bis": forms.DateInput(attrs={"type": "date"}, format="%Y-%m-%d"),
            "hinweise": forms.Textarea(attrs={"rows": 4}),
        }

    def clean_veranstalter_formular(self):
        datei = self.cleaned_data.get("veranstalter_formular")
        if datei and hasattr(datei, "content_type") and datei.content_type != "application/pdf":
            raise forms.ValidationError("Bitte eine PDF-Datei hochladen.")
        return datei


class StatusForm(forms.ModelForm):
    class Meta:
        model = Reise
        fields = ["status", "bezahlt_am"]
        widgets = {"bezahlt_am": forms.DateInput(attrs={"type": "date"}, format="%Y-%m-%d")}


class GenehmigungForm(forms.ModelForm):
    class Meta:
        model = Reise
        fields = ["genehmigung_pdf"]

    def clean_genehmigung_pdf(self):
        datei = self.cleaned_data.get("genehmigung_pdf")
        if not datei:
            raise forms.ValidationError("Bitte eine Datei auswählen.")
        return datei


class AnhangForm(forms.ModelForm):
    class Meta:
        model = Anhang
        fields = ["art", "datei", "beilegen"]


class EinstellungenForm(forms.ModelForm):
    smtp_passwort = forms.CharField(
        label="SMTP-Passwort", required=False, widget=forms.PasswordInput(render_value=False),
        help_text="Leer lassen, um das gespeicherte Passwort zu behalten.",
    )

    class Meta:
        model = Einstellungen
        fields = "__all__"
        widgets = {"dienststelle": forms.Textarea(attrs={"rows": 3})}

    def clean_unterschrift_svg(self):
        datei = self.cleaned_data.get("unterschrift_svg")
        if datei and hasattr(datei, "name") and not datei.name.lower().endswith(".svg"):
            raise forms.ValidationError("Bitte eine SVG-Datei hochladen.")
        return datei

    def save(self, commit=True):
        obj = super().save(commit=False)
        if not self.cleaned_data.get("smtp_passwort"):
            obj.smtp_passwort = Einstellungen.laden().smtp_passwort
        if commit:
            obj.save()
        return obj
