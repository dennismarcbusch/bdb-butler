# Installation und Betrieb

Diese Anleitung führt durch die Einrichtung des BdB-Butlers: Sync-Server, App, erster Start, weitere Rechner, Mail, KI-Zugriff, Sicherung und Updates. Hintergründe zu Verschlüsselung und Protokoll stehen in [ARCHITEKTUR.md](ARCHITEKTUR.md).

## Inhalt

1. [Überblick: Was du brauchst](#1-überblick-was-du-brauchst)
2. [Sync-Server einrichten](#2-sync-server-einrichten)
3. [App aus dem Quelltext bauen](#3-app-aus-dem-quelltext-bauen)
4. [App installieren](#4-app-installieren)
5. [Erster Start: Tresor anlegen](#5-erster-start-tresor-anlegen)
6. [Weitere Rechner verbinden](#6-weitere-rechner-verbinden)
7. [Ordner für PDFs und Anhänge](#7-ordner-für-pdfs-und-anhänge)
8. [Dienstreisen: Stammdaten und Mail](#8-dienstreisen-stammdaten-und-mail)
9. [Daten aus dem alten Dienstreisebuster übernehmen](#9-daten-aus-dem-alten-dienstreisebuster-übernehmen)
10. [KI-Zugriff (MCP) einrichten](#10-ki-zugriff-mcp-einrichten)
11. [Sicherung und Wiederherstellung](#11-sicherung-und-wiederherstellung)
12. [Updates](#12-updates)
13. [Fehlersuche](#13-fehlersuche)

## 1. Überblick: Was du brauchst

| Teil | Wo | Wozu |
|---|---|---|
| Sync-Server | ein Linux-Rechner, zum Beispiel ein LXC-Container auf Proxmox | verteilt die verschlüsselten Daten zwischen deinen Rechnern |
| Reverse-Proxy mit TLS | zum Beispiel ein vorhandener Caddy | HTTPS nach außen. Der Sync-Server selbst spricht nur HTTP |
| App | jeder Rechner, auf dem du arbeitest (Windows oder Linux) | die eigentliche Anwendung |
| Synchronisierter Ordner | Nextcloud-Client (oder ein anderes Werkzeug) auf jedem Rechner | PDFs und Anhänge der Dienstreisen und Berichte |

Der Server ist **optional**. Ohne Server arbeitet die App auf einem einzelnen Rechner, und du kannst ihn später nachtragen. Für mehrere Rechner ist er nötig.

Die Rechner, auf denen die App läuft, sollten eine Festplattenverschlüsselung haben (BitLocker, LUKS). Der Ordner mit PDFs und Anhängen ist bewusst nicht zusätzlich verschlüsselt.

## 2. Sync-Server einrichten

Der Server ist ein einzelnes Programm ohne Abhängigkeiten. Er speichert in einer SQLite-Datei nur Chiffretext und kennt keine Inhalte.

### 2.1 Token erzeugen

Das Token schützt den Zugriff auf den Chiffretext. Es muss mindestens 32 Zeichen lang sein:

```bash
openssl rand -base64 36
```

Bewahre das Token in deinem Passwortmanager auf. Du gibst es auf jedem Rechner einmal in der App ein.

### 2.2 Server bauen

Auf einem Rechner mit Go (Version laut `server/go.mod`, Go lädt sie bei Bedarf selbst). Das Ergebnis ist eine statische Datei, die auf jedem 64-Bit-Linux läuft:

```bash
cd server
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -trimpath -ldflags="-s -w" -o bdbb-server .
```

Kurz ausprobieren:

```bash
BDBB_TOKEN="<Token>" BDBB_DB=/tmp/bdbb-test.db ./bdbb-server
```

```bash
curl http://127.0.0.1:8080/healthz
```

Die Antwort ist `{"status":"ok"}`.

### 2.3 Im LXC-Container als Dienst einrichten

Die folgenden Schritte gelten für einen Debian- oder Ubuntu-Container. Lege einen eigenen Systemnutzer und die Ordner an:

```bash
useradd --system --home /var/lib/bdbb --shell /usr/sbin/nologin bdbb
```

```bash
install -d -o bdbb -g bdbb -m 0700 /var/lib/bdbb
```

```bash
install -d -m 0750 /etc/bdbb
```

Kopiere `bdbb-server` nach `/usr/local/bin/` (zum Beispiel per `scp`) und mach die Datei ausführbar:

```bash
install -m 0755 bdbb-server /usr/local/bin/bdbb-server
```

Lege die Konfiguration in `/etc/bdbb/bdbb.env` an (Rechte 0600, nur root lesbar):

```bash
install -m 0600 /dev/null /etc/bdbb/bdbb.env
```

Inhalt von `/etc/bdbb/bdbb.env`:

```
BDBB_TOKEN=<Token aus 2.1>
BDBB_ADDR=:8080
BDBB_DB=/var/lib/bdbb/bdbb.db
```

| Variable | Bedeutung | Standard |
|---|---|---|
| `BDBB_TOKEN` | Bearer-Token, Pflicht, mindestens 32 Zeichen | |
| `BDBB_ADDR` | Adresse und Port | `:8080` |
| `BDBB_DB` | Pfad der SQLite-Datei | `./data/bdbb.db` |

Die systemd-Unit `/etc/systemd/system/bdbb.service`:

```ini
[Unit]
Description=BdB-Butler Sync-Server
After=network-online.target
Wants=network-online.target

[Service]
User=bdbb
Group=bdbb
EnvironmentFile=/etc/bdbb/bdbb.env
ExecStart=/usr/local/bin/bdbb-server
Restart=on-failure
# Härtung
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/bdbb

[Install]
WantedBy=multi-user.target
```

Starten und prüfen:

```bash
systemctl daemon-reload
```

```bash
systemctl enable --now bdbb
```

```bash
systemctl status bdbb
```

```bash
curl http://127.0.0.1:8080/healthz
```

Der Server protokolliert nur Adresse, Start und interne Fehler, nie Inhalte oder das Token (`journalctl -u bdbb`).

### 2.4 Caddy davor setzen

Auf dem Caddy-Container, in der `Caddyfile`:

```
butler.example.de {
	reverse_proxy <IP des LXC>:8080
}
```

Danach `systemctl reload caddy`. Caddy besorgt das TLS-Zertifikat selbst. Prüfen:

```bash
curl https://butler.example.de/healthz
```

Empfehlungen:

- Der Port 8080 des Server-Containers sollte nur vom Caddy-Container aus erreichbar sein (Proxmox-Firewall oder `nftables`).
- Die Anfragen der App tragen das Token im Kopf `Authorization`. Caddy schwärzt diesen Kopf in Zugriffsprotokollen von sich aus. Schalte die globale Option `log_credentials` nicht ein.
- Einträge sind höchstens 1 MiB groß, eine Anfrage höchstens 2 MiB. Ein Limit im Proxy darf nicht darunter liegen.

## 3. App aus dem Quelltext bauen

Fertige Installationspakete gibt es noch nicht, die App wird auf dem Zielsystem gebaut (Linux-Pakete unter Linux, Windows-Installer unter Windows).

### 3.1 Voraussetzungen

Für alle Systeme:

- **Rust** (stabil, getestet mit 1.97) über <https://rustup.rs>
- **Node.js 22** mit npm
- **Git**

**Linux (Debian, Ubuntu)**, die Bibliotheken für Tauri 2:

```bash
sudo apt install build-essential curl wget file libssl-dev libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
```

Für Fedora und andere Systeme siehe die [Tauri-Voraussetzungen](https://tauri.app/start/prerequisites/).

**Windows 10/11:**

- „Microsoft C++ Build Tools“ mit der Auswahl „Desktopentwicklung mit C++“
- WebView2 (in Windows 10 ab 1803 und Windows 11 bereits enthalten)
- Rust mit der MSVC-Toolchain (Standard bei `rustup` unter Windows)

### 3.2 Bauen

```bash
git clone https://github.com/dennismarcbusch/bdb-butler.git
```

```bash
cd bdb-butler/app
```

```bash
npm ci
```

```bash
npm run tauri build
```

Der erste Bau dauert einige Minuten. Die Pakete liegen danach unter `target/release/bundle/` im Hauptordner des Repositorys:

| System | Pakete |
|---|---|
| Linux | `deb/*.deb`, `rpm/*.rpm`, `appimage/*.AppImage` |
| Windows | `msi/*.msi`, `nsis/*-setup.exe` |

Vor dem Bauen kannst du prüfen, ob alles in Ordnung ist (siehe [README](../README.md#tests)):

```bash
cargo test -p bdb-core
```

## 4. App installieren

**Linux (Debian, Ubuntu):**

```bash
sudo apt install ./target/release/bundle/deb/*.deb
```

Alternativ das AppImage ausführbar machen und starten (`chmod +x`, keine Installation nötig).

**Windows:** Den Installer aus `target/release/bundle/nsis/` oder `msi/` ausführen. Da die Pakete nicht signiert sind, warnt Windows SmartScreen beim ersten Start („Weitere Informationen“, dann „Trotzdem ausführen“).

Die App legt ihre Daten in einem eigenen Ordner ab:

| System | Datenordner |
|---|---|
| Linux | `~/.local/share/de.dennisbusch.bdbbutler/` |
| Windows | `%APPDATA%\de.dennisbusch.bdbbutler\` |

Darin liegen:

| Datei | Inhalt |
|---|---|
| `bdb.sqlite` | lokale Datenbank: alle Einträge als Chiffretext, Keyring, verschlüsselte Token und SMTP-Passwort |
| `settings.json` | Server-Adresse und Ordner für Anhänge (kein Token) |
| `mcp.json` | Einstellungen des KI-Zugriffs (kein Token) |
| `mcp-audit.log` | Protokoll des KI-Zugriffs (ohne Inhalte) |

Dieser Ordner gehört **nicht** in den Nextcloud-Ordner. Die Datenbank wird über den Sync-Server abgeglichen, nicht über Dateien.

## 5. Erster Start: Tresor anlegen

Beim ersten Start zeigt die App die Einrichtung.

1. Wähle **„Neu einrichten“**.
2. Trage **Server-Adresse** (`https://butler.example.de`) und **Token** ein. Beides kannst du auch später in den Einstellungen nachtragen.
3. Wähle ein **Master-Passwort**: mindestens 10 Zeichen, besser eine Passphrase aus sechs bis acht zufälligen Wörtern. Wer den Serverbestand erbeutet, kann es offline durchprobieren.
4. **„Tresor anlegen“**. Das dauert etwa eine Sekunde (die Schlüsselableitung ist absichtlich teuer).
5. Die App zeigt den **Recovery-Key** (Vierergruppen aus Buchstaben und Ziffern) **genau einmal** an. Schreib ihn ab oder druck ihn aus und bewahre ihn offline auf, zum Beispiel bei wichtigen Unterlagen. Mit ihm entsperrst du die App, wenn du das Passwort vergessen hast. **Ohne Passwort und Recovery-Key sind die Daten nicht wiederherstellbar**, auch nicht vom Server.

Danach ist die App entsperrt. Nach 15 Minuten ohne Eingabe sperrt sie sich selbst, oder du sperrst sie über „Sperren“.

Hast du ohne Server begonnen, trägst du ihn später in **Einstellungen, Server und Ordner** ein. Beim nächsten Abgleich lädt die App den Tresor hoch.

## 6. Weitere Rechner verbinden

1. App auf dem weiteren Rechner installieren (Abschnitte 3 und 4).
2. In der Einrichtung **„Mit vorhandenem Tresor verbinden“** wählen.
3. Dieselbe **Server-Adresse**, dasselbe **Token** und dein **Master-Passwort** eingeben, dann **„Verbinden“**.
4. Die App holt die Daten vom Server. Danach gleicht sie beim Entsperren, kurz nach jeder Änderung und auf Knopfdruck („Jetzt abgleichen“ in den Einstellungen) ab.

Gut zu wissen:

- Ändert ein Rechner das Master-Passwort (**Einstellungen, Master-Passwort ändern**), gilt auf den anderen Rechnern nach deren nächstem Abgleich ebenfalls nur noch das neue Passwort. Der Recovery-Key bleibt gültig.
- Ändern zwei Rechner dasselbe Feld, ohne sich zwischendurch abzugleichen, gewinnt die jüngere Änderung. Die andere erscheint unter **„Konflikte“** und lässt sich übernehmen.
- Der Ordner für PDFs und Anhänge wird je Rechner eingetragen (Abschnitt 7).

## 7. Ordner für PDFs und Anhänge

Antrag, Abrechnung, Genehmigung, Veranstalter-Formulare, Anhänge und gespeicherte Tätigkeitsberichte legt die App als Dateien in einem Ordner ab:

```
<Ordner>/
├── Dienstreisen/
│   └── 2026-11-16-beratung-medienkonzept-1a2b3c4d/
│       ├── antrag-2026-11-16.pdf
│       ├── genehmigung-….pdf
│       └── anhang-….pdf
└── Berichte/
    └── Tätigkeitsbericht Schuljahr 2025-26.md
```

1. Lege auf jedem Rechner einen Ordner an, den der **Nextcloud-Client** synchronisiert, zum Beispiel `~/Nextcloud/BdB-Butler` beziehungsweise `C:\Users\<Name>\Nextcloud\BdB-Butler`.
2. Trage ihn in der App unter **Einstellungen, Server und Ordner** als Ordner für PDFs und Anhänge ein.

Die Einträge speichern nur Pfade relativ zu diesem Ordner. Der Pfad darf deshalb auf jedem Rechner anders lauten. Fehlt eine Datei noch (Nextcloud hat sie noch nicht geholt), meldet die App das, statt abzubrechen.

## 8. Dienstreisen: Stammdaten und Mail

Unter **Einstellungen, Stammdaten für Dienstreisen** trägst du einmal ein, was in Antrag und Abrechnung gehört. Die Stammdaten werden zwischen deinen Rechnern abgeglichen.

- **Persönliche Daten und Dienststelle:** Name, Vorname, Adresse, Telefon, E-Mail für das Formular, Beschäftigungsbehörde, Sachgebiet, Besoldungsgruppe, Organisations- und Personalnummer. Name, Vorname, PLZ, Ort, Straße, Kurzname der Beschäftigungsbehörde und Personalnummer sind nötig, um PDFs zu erzeugen.
- **Bankverbindung:** IBAN, BIC, Kreditinstitut (für FIBS und R002).
- **Unterschrift:** Ort für die Unterschrift und die Unterschrift als SVG-Datei. Sie wird als Vektorgrafik in Antrag und FIBS-Abrechnung eingesetzt (nicht in R002).
- **Mailversand:** Mailadresse der Verwaltung (Antrag) und des LfF (Abrechnung), deine Absenderadresse sowie SMTP-Server, Port, Benutzer und Verschlüsselung (STARTTLS, meist Port 587; SSL/TLS, meist Port 465). Die Werte bekommst du von deinem Mailanbieter oder der Dienststelle.
- **SMTP-Passwort:** Es wird **nicht** abgeglichen und liegt verschlüsselt in der lokalen Datenbank des Rechners. Trag es deshalb auf jedem Rechner ein, von dem aus du Mails senden willst.

Beim Versand zeigt die App Betreff, Text und Anhänge zur Kontrolle. Die Mail darf mit Anhängen höchstens 20 MB groß werden. Weil Anhänge in der Mail etwa ein Drittel größer sind als die Dateien, passen etwa 14,6 MB an Dateien hinein. Die Ansicht zeigt beide Größen.

## 9. Daten aus dem alten Dienstreisebuster übernehmen

Der Import übernimmt Stammdaten (mit Unterschrift und SMTP-Passwort), Reisen, Dateien und Anhänge aus dem alten Django-Tool. Das alte Tool bleibt dabei unverändert, Dateien werden kopiert.

1. Kopiere vom Server des alten Tools die Datei `data/db.sqlite3` und den Ordner `data/media` auf den Rechner, zum Beispiel per `scp -r`. Am besten hältst du den Docker-Container vorher an (`docker compose stop`), damit die Datenbank in einem festen Zustand ist.
2. Trag in der App zuerst den Ordner für PDFs und Anhänge ein (Abschnitt 7).
3. **Einstellungen, Import aus dem alten Dienstreisebuster:** die Pfade zu `db.sqlite3` und zum Ordner `media` eintragen, dann **„Importieren“**.
4. Der Bericht zeigt, was übernommen, übersprungen oder nicht gefunden wurde. Zeiten werden von UTC in deutsche Zeit umgerechnet.

Ein erneuter Import überspringt bereits Übernommenes und überschreibt nichts. Bereits ausgefüllte Stammdaten bleiben unangetastet. Führe den Import auf **einem** Rechner aus und gleiche dann ab. Die anderen Rechner bekommen die Einträge über den Server und die Dateien über Nextcloud.

Anschließend die kopierten Dateien des alten Tools löschen, denn sie enthalten das SMTP-Passwort und die Bankverbindung im Klartext.

## 10. KI-Zugriff (MCP) einrichten

Die App enthält einen lokalen MCP-Server, über den eine KI auf deine Einträge zugreifen kann. Er läuft nur, solange die App läuft **und entsperrt ist**, und ist nur vom eigenen Rechner aus erreichbar (`127.0.0.1`).

### 10.1 In der App einschalten

**Einstellungen, KI-Zugriff (MCP)** (nur bei entsperrter App):

1. **„KI-Zugriff einschalten“** ankreuzen. Standardport ist 8765.
2. Optional den **Schreibzugriff** einschalten: Dann darf die KI Einträge anlegen und ändern. Löschen und Mailversand gibt es über die KI nie.
3. **Typen mit Personenbezug** (Ansprechpartner, beratene Personen, Protokolle) sind gesperrt. Gib sie nur frei, wenn du das bewusst willst. Die Datenschutzfreigabe deckt den Zugriff ab, du achtest selbst darauf, dass über die KI keine personenbezogenen Daten verarbeitet werden.
4. Die App zeigt Adresse, Token und einen fertigen Befehl für Claude Code. Über **„Neues Token erzeugen“** machst du alte Zugänge ungültig.

### 10.2 Claude Code verbinden

Den angezeigten Befehl ausführen, etwa:

```bash
claude mcp add --transport http bdb-butler http://127.0.0.1:8765/mcp --header "Authorization: Bearer <Token>"
```

Andere MCP-Clients, die Streamable HTTP mit eigenem Kopf `Authorization` beherrschen, verbindest du mit derselben Adresse und demselben Token.

### 10.3 Was die KI sieht

- Werkzeuge: `list_kinds`, `list_records`, `get_record`, `search`; mit Schreibzugriff `create_record` und `update_record`.
- Gesperrte Typen bleiben auch per ID und Suche unsichtbar. Verweise (IDs) auf sie können in anderen Einträgen stehen, ihre Inhalte nicht.
- Dateipfade und Versandzeitpunkte der Dienstreisen verwaltet die App selbst. Die KI kann sie nicht ändern.
- Jeder Aufruf steht im Protokoll (Einstellungen, KI-Zugriff), ohne Inhalte.
- Ist die App gesperrt, antwortet der Server mit „Die App ist gesperrt. Bitte in der App entsperren.“

## 11. Sicherung und Wiederherstellung

| Was | Wie sichern | Wiederherstellen |
|---|---|---|
| **Recovery-Key** | offline, ausgedruckt oder abgeschrieben | Entsperren mit dem Recovery-Key auf dem Sperrbildschirm |
| **Server-Datenbank** | regelmäßig `/var/lib/bdbb/bdbb.db` sichern (Chiffretext, darf auch auf weniger vertrauenswürdige Ziele) | siehe Hinweis unten |
| **Ordner für PDFs und Anhänge** | über Nextcloud (Versionen, Papierkorb) und die übliche Nextcloud-Sicherung | aus Nextcloud |
| Lokaler Datenordner eines Rechners | nicht nötig, solange der Server läuft | App neu installieren und mit dem Tresor verbinden (Abschnitt 6) |

Eine konsistente Kopie der Server-Datenbank im laufenden Betrieb erstellst du mit SQLite selbst (Paket `sqlite3`):

```bash
sqlite3 /var/lib/bdbb/bdbb.db ".backup '/var/backups/bdbb-$(date +%F).db'"
```

Alternativ den Dienst kurz anhalten (`systemctl stop bdbb`), die Datei kopieren und wieder starten.

**Wichtig, Stand heute:** Jeder Rechner hat den vollständigen Stand, und solange einer davon intakt ist, gehen keine Daten verloren. Einen fertigen Weg, den Server nach einem Verlust wieder in Betrieb zu nehmen, gibt es aber noch nicht. Die Rechner merken sich, bis zu welcher Folgenummer sie den Server kennen. Ein leerer Server oder eine ältere Sicherung passen nicht dazu, und der Abgleich läuft dann nicht mehr sauber: Er bricht ab oder übersieht neue Einträge anderer Rechner. Bis die App das selbst kann:

- Den Server-Container mit Proxmox-Sicherungen absichern und die Datenbank wie oben sichern, als Notfallkopie des Chiffretexts.
- Fällt der Server aus, die Rechner offline weiterbenutzen (das geht ohne Einschränkung) und den Server nicht mit einer älteren Sicherung zurückspielen, ohne die Rechner neu zu verbinden.

## 12. Updates

**Server:** neu bauen (Abschnitt 2.2), Datei ersetzen und den Dienst neu starten:

```bash
install -m 0755 bdbb-server /usr/local/bin/bdbb-server
```

```bash
systemctl restart bdbb
```

**App:** im Repository den neuen Stand holen und neu bauen (Abschnitt 3.2), dann das neue Paket installieren. Die Daten im Datenordner bleiben erhalten. Aktualisiere am besten alle Rechner zeitnah: Ein Rechner mit älterer App überspringt Einträge im neueren Format und meldet sie, bis er aktualisiert ist.

## 13. Fehlersuche

| Meldung oder Verhalten | Ursache und Abhilfe |
|---|---|
| „Der Server hat das Token abgelehnt“ | Token in der App stimmt nicht mit `BDBB_TOKEN` überein. In Einstellungen, Server und Ordner neu eintragen |
| „Server nicht erreichbar“ | Adresse prüfen (mit `https://`), `curl https://butler.example.de/healthz`, Caddy und `systemctl status bdbb` prüfen |
| „Auf dem Server liegt ein anderer Tresor als lokal“ | Der Rechner hat offline einen eigenen Tresor angelegt, auf dem Server liegt schon einer. Den lokalen Datenordner entfernen (Abschnitt 4) und mit „Mit vorhandenem Tresor verbinden“ neu einrichten. Achtung: Was nur lokal existierte, geht dabei verloren |
| Hinweis „neuerer Keyring, dessen Prüfsumme nicht passt“ | Der Keyring auf dem Server stammt nicht von diesem Tresor oder von einer älteren App-Fassung. Alle Rechner aktualisieren und das Passwort auf einem davon neu setzen. Tritt das ohne Passwortwechsel auf, den Server prüfen |
| Hinweis „Einträge ließen sich nicht einspielen“ | Ein Eintrag ist beschädigt oder stammt von einer neueren App-Fassung. Diese App aktualisieren. Der Abgleich läuft für alle anderen Einträge weiter |
| „Bitte zuerst den Ordner für PDFs und Anhänge … eintragen“ | Abschnitt 7 |
| „Bitte zuerst die Stammdaten vervollständigen“ | Abschnitt 8, die genannten Felder ausfüllen |
| Mailversand schlägt fehl | Host, Port und Verschlüsselung prüfen (STARTTLS/587 oder SSL/TLS/465), SMTP-Passwort auf diesem Rechner eintragen, Größe der Anhänge prüfen |
| KI-Client bekommt 401 | Token in der App neu kopieren, gegebenenfalls „Neues Token erzeugen“ und Client neu verbinden |
| KI-Client bekommt 503 | KI-Zugriff ist aus oder die App ist gesperrt |
| KI-Zugriff startet nicht („Port …“) | Ein anderes Programm belegt den Port. In den Einstellungen einen anderen Port (ab 1024) wählen und den Client anpassen |
| Master-Passwort vergessen | Auf dem Sperrbildschirm mit dem Recovery-Key entsperren, dann unter Einstellungen ein neues Passwort setzen |
