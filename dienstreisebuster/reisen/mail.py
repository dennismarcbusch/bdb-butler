"""Mailversand über den in den Einstellungen hinterlegten SMTP-Server."""
from django.core.mail import EmailMessage, get_connection

MAX_BYTES = 20 * 1024 * 1024


class VersandFehler(Exception):
    pass


def _verbindung(einst):
    return get_connection(
        host=einst.smtp_host,
        port=einst.smtp_port,
        username=einst.smtp_user or None,
        password=einst.smtp_passwort or None,
        use_tls=einst.smtp_sicherheit == "starttls",
        use_ssl=einst.smtp_sicherheit == "ssl",
        timeout=30,
    )


def pruefen(einst, empfaenger):
    fehlt = []
    if not empfaenger:
        fehlt.append("Empfänger")
    if not einst.absender:
        fehlt.append("Absenderadresse")
    if not einst.smtp_host:
        fehlt.append("SMTP-Server")
    if fehlt:
        raise VersandFehler("In den Einstellungen fehlt: " + ", ".join(fehlt) + ".")


def senden(einst, empfaenger, betreff, text, dateien):
    """dateien: Liste von (Dateiname, Bytes). Wirft VersandFehler bei Problemen."""
    pruefen(einst, empfaenger)
    if sum(len(inhalt) for _, inhalt in dateien) > MAX_BYTES:
        raise VersandFehler("Die Anhänge sind zusammen größer als 20 MB.")
    mail = EmailMessage(betreff, text, einst.absender, [empfaenger], connection=_verbindung(einst))
    for name, inhalt in dateien:
        mail.attach(name, inhalt)
    try:
        mail.send()
    except Exception as exc:  # smtplib, Socket- und TLS-Fehler
        raise VersandFehler(f"Versand fehlgeschlagen: {exc}") from exc
