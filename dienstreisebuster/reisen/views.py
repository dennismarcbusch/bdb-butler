import datetime as dt
import os

from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.core.files.base import ContentFile
from django.http import FileResponse, Http404
from django.shortcuts import get_object_or_404, redirect, render
from django.utils import timezone
from django.views.decorators.http import require_POST

from .forms import AbrechnungForm, AnhangForm, EinstellungenForm, GenehmigungForm, ReiseForm, StatusForm
from .mail import MAX_BYTES, VersandFehler, senden
from .models import Anhang, Einstellungen, Reise
from .pdf import abrechnung as abrechnung_pdf
from .pdf import antrag as antrag_pdf

EINSTELLUNGS_GRUPPEN = [
    ("Persönliche Daten", ["name", "vorname", "plz", "ort", "strasse", "telefon", "email"]),
    ("Dienststelle", ["dienststelle", "dienststelle_kurz", "abteilung", "besoldungsgruppe", "org_nr", "personal_nr"]),
    ("Bankverbindung", ["iban", "bic", "kreditinstitut"]),
    ("Unterschrift", ["unterschrift_ort", "unterschrift_svg"]),
    ("Mailversand", ["empfaenger_antrag", "empfaenger_abrechnung", "absender", "smtp_host", "smtp_port",
                     "smtp_user", "smtp_passwort", "smtp_sicherheit"]),
]
PFLICHT_STAMMDATEN = ["name", "vorname", "plz", "ort", "strasse", "dienststelle_kurz", "personal_nr"]


@login_required
def liste(request):
    reisen = Reise.objects.all()
    status = request.GET.get("status")
    if status in Reise.Status.values:
        reisen = reisen.filter(status=status)
    return render(request, "reisen/liste.html", {
        "reisen": reisen,
        "status": status,
        "stati": Reise.Status.choices,
        "ohne_stammdaten": not _stammdaten_vollstaendig(),
    })


def _stammdaten_vollstaendig():
    einst = Einstellungen.laden()
    return all(getattr(einst, f) for f in PFLICHT_STAMMDATEN)


@login_required
def neu(request):
    form = ReiseForm(request.POST or None)
    if request.method == "POST" and form.is_valid():
        reise = form.save()
        messages.success(request, "Reise angelegt.")
        return redirect("reisen:detail", pk=reise.pk)
    return render(request, "reisen/reise_form.html", {"form": form, "titel": "Neue Reise"})


@login_required
def bearbeiten(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    form = ReiseForm(request.POST or None, instance=reise)
    if request.method == "POST" and form.is_valid():
        form.save()
        messages.success(request, "Gespeichert. Ein bereits erzeugter Antrag ist dadurch veraltet.")
        return redirect("reisen:detail", pk=reise.pk)
    return render(request, "reisen/reise_form.html", {"form": form, "titel": "Reise bearbeiten", "reise": reise})


@login_required
def detail(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    return render(request, "reisen/detail.html", {
        "reise": reise,
        "abrechnung_form": AbrechnungForm(instance=reise),
        "status_form": StatusForm(instance=reise),
        "genehmigung_form": GenehmigungForm(),
        "anhang_form": AnhangForm(),
        "ohne_stammdaten": not _stammdaten_vollstaendig(),
    })


@login_required
@require_POST
def abrechnung_speichern(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    form = AbrechnungForm(request.POST, request.FILES, instance=reise)
    if form.is_valid():
        form.save()
        messages.success(request, "Abrechnungsdaten gespeichert.")
    else:
        messages.error(request, "Abrechnungsdaten nicht gespeichert: " + _fehler(form))
    return redirect("reisen:detail", pk=pk)


@login_required
@require_POST
def status_speichern(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    form = StatusForm(request.POST, instance=reise)
    if form.is_valid():
        form.save()
        messages.success(request, "Status gespeichert.")
    else:
        messages.error(request, "Status nicht gespeichert: " + _fehler(form))
    return redirect("reisen:detail", pk=pk)


@login_required
@require_POST
def antrag_erzeugen(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    if not _stammdaten_vollstaendig():
        messages.error(request, "Bitte zuerst die Stammdaten in den Einstellungen vervollständigen.")
        return redirect("reisen:einstellungen")
    einst = Einstellungen.laden()
    inhalt = antrag_pdf.erzeugen(reise, einst)
    if reise.antrag_pdf:
        reise.antrag_pdf.delete(save=False)
    reise.antrag_pdf.save(f"antrag-{reise.reise_beginn:%Y-%m-%d}.pdf", ContentFile(inhalt), save=False)
    reise.save(update_fields=["antrag_pdf"])
    messages.success(request, "Antrag erzeugt.")
    return redirect("reisen:detail", pk=pk)


@login_required
@require_POST
def genehmigung_hochladen(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    form = GenehmigungForm(request.POST, request.FILES, instance=reise)
    if form.is_valid():
        reise = form.save(commit=False)
        if reise.status in (Reise.Status.ENTWURF, Reise.Status.BEANTRAGT):
            reise.status = Reise.Status.GENEHMIGT
        reise.save()
        messages.success(request, "Genehmigung gespeichert.")
    else:
        messages.error(request, "Genehmigung nicht gespeichert: " + _fehler(form))
    return redirect("reisen:detail", pk=pk)


@login_required
@require_POST
def anhang_hinzufuegen(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    form = AnhangForm(request.POST, request.FILES)
    if form.is_valid():
        anhang = form.save(commit=False)
        anhang.reise = reise
        anhang.save()
        messages.success(request, "Anhang gespeichert.")
    else:
        messages.error(request, "Anhang nicht gespeichert: " + _fehler(form))
    return redirect("reisen:detail", pk=pk)


@login_required
@require_POST
def anhang_loeschen(request, pk):
    anhang = get_object_or_404(Anhang, pk=pk)
    reise_pk = anhang.reise_id
    anhang.datei.delete(save=False)
    anhang.delete()
    messages.success(request, "Anhang gelöscht.")
    return redirect("reisen:detail", pk=reise_pk)


@login_required
def datei(request, pk, art):
    """Liefert hochgeladene oder erzeugte Dateien nur an angemeldete Nutzer aus."""
    if art == "anhang":
        feld = get_object_or_404(Anhang, pk=pk).datei
    elif art in ("antrag", "genehmigung", "abrechnung", "veranstalter"):
        feld = getattr(get_object_or_404(Reise, pk=pk), {
            "antrag": "antrag_pdf", "genehmigung": "genehmigung_pdf",
            "abrechnung": "abrechnung_pdf", "veranstalter": "veranstalter_formular",
        }[art])
    else:
        raise Http404
    if not feld:
        raise Http404
    return FileResponse(feld.open("rb"), as_attachment=False, filename=feld.name.rsplit("/", 1)[-1])


@login_required
def einstellungen(request):
    einst = Einstellungen.laden()
    form = EinstellungenForm(request.POST or None, request.FILES or None, instance=einst)
    if request.method == "POST" and form.is_valid():
        form.save()
        messages.success(request, "Einstellungen gespeichert.")
        return redirect("reisen:einstellungen")
    gruppen = [
        (titel, [form[name] for name in namen]) for titel, namen in EINSTELLUNGS_GRUPPEN
    ]
    return render(request, "reisen/einstellungen.html", {"form": form, "gruppen": gruppen})


def _fehler(form):
    return "; ".join(f"{form.fields[f].label if f in form.fields else f}: {' '.join(e)}" for f, e in form.errors.items())


@login_required
@require_POST
def abrechnung_erzeugen(request, pk):
    reise = get_object_or_404(Reise, pk=pk)
    if not _stammdaten_vollstaendig():
        messages.error(request, "Bitte zuerst die Stammdaten in den Einstellungen vervollständigen.")
        return redirect("reisen:einstellungen")
    if not reise.veranstalter_formular:
        messages.error(request, "Bitte zuerst das Abrechnungsformular des Veranstalters hochladen "
                                "(Abrechnungsdaten). Andere Formulare werden noch nicht unterstützt.")
        return redirect("reisen:detail", pk=pk)
    try:
        inhalt = abrechnung_pdf.erzeugen(reise.veranstalter_formular.path, reise, Einstellungen.laden(),
                                         timezone.localdate())
    except Exception as exc:  # unbekanntes oder beschädigtes Formular
        messages.error(request, f"Das Formular konnte nicht ausgefüllt werden: {exc}")
        return redirect("reisen:detail", pk=pk)
    if reise.abrechnung_pdf:
        reise.abrechnung_pdf.delete(save=False)
    reise.abrechnung_pdf.save(f"abrechnung-{reise.reise_beginn:%Y-%m-%d}.pdf", ContentFile(inhalt), save=False)
    reise.save(update_fields=["abrechnung_pdf"])
    messages.success(request, "Abrechnung erzeugt.")
    return redirect("reisen:detail", pk=pk)


def _mail_vorgabe(reise, einst, art):
    unterschrift = f"{einst.vorname} {einst.name}".strip()
    zeitraum = f"{reise.reise_beginn:%d.%m.%Y}"
    if reise.reise_ende.date() != reise.reise_beginn.date():
        zeitraum += f" bis {reise.reise_ende:%d.%m.%Y}"
    if art == "antrag":
        betreff = f"Antrag auf Genehmigung einer {reise.get_art_display()} am {zeitraum}"
        text = (f"Sehr geehrte Damen und Herren,\n\nanbei übersende ich meinen Antrag auf Genehmigung einer "
                f"{reise.get_art_display()} nach {reise.geschaeftsort} ({zeitraum}).\n\n"
                f"Mit freundlichen Grüßen\n{unterschrift}")
    else:
        betreff = f"Reisekostenabrechnung {reise.zweck} ({zeitraum})"
        text = (f"Sehr geehrte Damen und Herren,\n\nanbei übersende ich meinen Erstattungsantrag für die "
                f"Reise nach {reise.geschaeftsort} ({zeitraum}) mit den zugehörigen Unterlagen.\n\n"
                f"Mit freundlichen Grüßen\n{unterschrift}")
    return betreff, text


def _versand_dateien(reise, art):
    """Mögliche Anhänge als Liste von (schluessel, bezeichnung, feld, vorausgewaehlt)."""
    if art == "antrag":
        return [("antrag", "Dienstreiseantrag", reise.antrag_pdf, True)]
    dateien = [("abrechnung", "Reisekostenabrechnung", reise.abrechnung_pdf, True),
               ("genehmigung", "Genehmigung", reise.genehmigung_pdf, True)]
    dateien += [(f"anhang{a.pk}", f"{a.get_art_display()}: {os.path.basename(a.datei.name)}", a.datei, a.beilegen)
                for a in reise.anhaenge.all()]
    return [d for d in dateien if d[2]]


@login_required
def versand(request, pk, art):
    """Vorschau und Versand: art ist "antrag" (an die Verwaltung) oder "abrechnung" (an das LfF)."""
    if art not in ("antrag", "abrechnung"):
        raise Http404
    reise = get_object_or_404(Reise, pk=pk)
    einst = Einstellungen.laden()
    empfaenger = einst.empfaenger_antrag if art == "antrag" else einst.empfaenger_abrechnung
    pflichtdatei = reise.antrag_pdf if art == "antrag" else reise.abrechnung_pdf
    if not pflichtdatei:
        messages.error(request, "Das Dokument wurde noch nicht erzeugt.")
        return redirect("reisen:detail", pk=pk)

    dateien = _versand_dateien(reise, art)
    betreff, text = _mail_vorgabe(reise, einst, art)
    gewaehlt = {k for k, *_, vorab in dateien if vorab}
    if request.method == "POST":
        betreff = request.POST.get("betreff", betreff).strip()
        text = request.POST.get("text", text)
        gewaehlt = set(request.POST.getlist("anhang"))
        pflicht_schluessel = dateien[0][0]
        gewaehlt.add(pflicht_schluessel)
        if "senden" in request.POST:
            anhaenge = []
            for schluessel, _, feld, _ in dateien:
                if schluessel in gewaehlt:
                    with feld.open("rb") as f:
                        anhaenge.append((os.path.basename(feld.name), f.read()))
            try:
                senden(einst, empfaenger, betreff, text, anhaenge)
            except VersandFehler as exc:
                messages.error(request, str(exc))
            else:
                jetzt = timezone.now()
                if art == "antrag":
                    reise.antrag_versendet_am = jetzt
                    if reise.status == Reise.Status.ENTWURF:
                        reise.status = Reise.Status.BEANTRAGT
                else:
                    reise.abrechnung_versendet_am = jetzt
                    if reise.status in (Reise.Status.ENTWURF, Reise.Status.BEANTRAGT, Reise.Status.GENEHMIGT):
                        reise.status = Reise.Status.ABGERECHNET
                reise.save()
                messages.success(request, f"Mail an {empfaenger} gesendet.")
                return redirect("reisen:detail", pk=pk)

    liste = [{"schluessel": k, "name": n, "groesse": f.size, "gewaehlt": k in gewaehlt, "pflicht": i == 0}
             for i, (k, n, f, _) in enumerate(dateien)]
    summe = sum(d["groesse"] for d in liste if d["gewaehlt"])
    return render(request, "reisen/versand.html", {
        "reise": reise, "art": art, "empfaenger": empfaenger, "absender": einst.absender,
        "betreff": betreff, "text": text, "dateien": liste, "summe_mb": summe / 1024 / 1024,
        "zu_gross": summe > MAX_BYTES,
        "bereits": reise.antrag_versendet_am if art == "antrag" else reise.abrechnung_versendet_am,
    })
