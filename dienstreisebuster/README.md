# Dienstreisebuster

Bestandstool im Repo `bdb-butler`, das durch das Dienstreisemodul der BdB-Butler-App abgelöst wird (Meilenstein 3,
siehe `../docs/ANFORDERUNGEN.md` und `../docs/ARCHITEKTUR.md`). Der Name „Dienstreisebuster“ entfällt dann.
Bis dahin läuft es unverändert als eigenständige Django-App. Es gehört nicht zur verschlüsselten Architektur der App:
IBAN, Adresse und SMTP-Passwort liegen hier unverschlüsselt in der Datenbank. Neue Funktionen kommen nicht mehr
hierher, sondern ins Dienstreisemodul der App.

Web-App (Django, SQLite), die Dienstreiseanträge und Reisekostenabrechnungen als PDF erzeugt,
per Mail verschickt und den Zahlungsstatus pro Reise verfolgt.

## Stand

- Datenmodell: Einstellungen (Stammdaten, Empfänger, SMTP, Unterschrift als SVG), Reise, Anhänge
- Browser-Oberfläche: Reiseliste, Reise anlegen/bearbeiten, Detailseite, Einstellungen
- PDFs mit Unterschrift: Antrag (R001) und Abrechnung im FIBS-Format
  (`reisen/pdf/antrag.py`, `reisen/pdf/abrechnung.py`)
- Mailversand mit Vorschau: Antrag an die Verwaltung, Abrechnung an das LfF mit auswählbaren Anhängen
  (`reisen/mail.py`); nach dem Versand wechselt der Status
- Offen: Ticketpreis-Erkennung, R002-Abrechnung (Vorlage liegt in `reisen/pdf/templates/`),
  Zuordnung unbekannter Veranstalter-Formulare

## Entwicklung

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
DEBUG=1 .venv/bin/python manage.py migrate
DEBUG=1 .venv/bin/python manage.py createsuperuser
DEBUG=1 .venv/bin/python manage.py runserver
```

## Betrieb (Docker Compose im LXC)

1. LXC mit aktivierten Optionen `nesting` und `keyctl` anlegen, Docker installieren.
2. `cp .env.example .env` und `SECRET_KEY`, `ALLOWED_HOSTS`, `CSRF_TRUSTED_ORIGINS` setzen.
3. `docker compose up -d --build`
4. Ersten Benutzer anlegen: `docker compose exec web python manage.py createsuperuser`
5. Caddy auf dem separaten LXC: `reverse_proxy <lxc-ip>:8000`

Alle veränderlichen Daten (SQLite, Uploads, erzeugte PDFs) liegen in `./data` und müssen gesichert werden.
Das SMTP-Passwort liegt in der Datenbank, nicht in der `.env`.
