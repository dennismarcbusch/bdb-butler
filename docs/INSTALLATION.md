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
9. [KI-Zugriff (MCP) einrichten](#9-ki-zugriff-mcp-einrichten)
10. [Sicherung und Wiederherstellung](#10-sicherung-und-wiederherstellung)
11. [Updates](#11-updates)
12. [Fehlersuche](#12-fehlersuche)

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

**Alpine Linux:** Ein Skript erledigt alles auf einmal (bauen, einrichten, starten), siehe [2.5](#25-alpine-linux-alles-per-skript). Die Abschnitte 2.1 bis 2.4 beschreiben den Weg von Hand, zum Beispiel für Debian.

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

### 2.5 Alpine Linux: alles per Skript

`server/deploy/install-alpine.sh` richtet den Server in einem Alpine-Container (LXC) vollständig ein: Server-Datei holen, Systemnutzer, Einstellungen mit neuem Token, OpenRC-Dienst, tägliche Sicherung, Start und Prüfung. Der Reverse-Proxy muss bereits auf den Container zeigen (Port 8080, sofern du nichts anderes einstellst).

Die Server-Datei lädt das Skript fertig gebaut aus dem **neuesten veröffentlichten Release** auf GitHub und prüft ihre Prüfsumme (Releases entstehen über den Workflow „Bauen“, siehe [3.0](#30-bauen-auf-github)). Gibt es noch keinen Release, baut es den Server im Container aus dem Quelltext. Das dauert einige Minuten und braucht mehr Speicher (siehe Hinweise).

Als root im Container:

```bash
wget -O install-alpine.sh https://raw.githubusercontent.com/dennismarcbusch/bdb-butler/main/server/deploy/install-alpine.sh
```

```bash
sh install-alpine.sh https://butler.example.de
```

Die Adresse ist die, unter der dein Reverse-Proxy den Server erreichbar macht. Sie ist optional: Mit ihr prüft das Skript am Ende auch den Weg über den Proxy und zeigt sie als Server-Adresse an.

Am Ende stehen **Server-Adresse und Token** auf dem Bildschirm. Übernimm das Token in deinen Passwortmanager und trag beides in der App ein (Abschnitt 5). Das Token steht außerdem in `/etc/bdbb/bdbb.env`.

Anpassen lässt sich das Skript über Umgebungsvariablen, zum Beispiel ein anderer Port:

```bash
BDBB_PORT=9000 sh install-alpine.sh https://butler.example.de
```

| Variable | Bedeutung | Standard |
|---|---|---|
| `BDBB_PORT` | Port des Servers (nur bei der Ersteinrichtung, danach in `/etc/bdbb/bdbb.env`) | `8080` |
| `BDBB_PUBLIC_URL` | wie das erste Argument: öffentliche Adresse für Prüfung und Ausgabe | keiner |
| `BDBB_SOURCE` | `auto` = Release, sonst Quelltext; `release` = nur Release; `source` = immer aus dem Quelltext bauen | `auto` |
| `BDBB_VERSION` | Release, zum Beispiel `v0.1.0` | `latest` |
| `BDBB_REF` | Branch oder Tag beim Bauen aus dem Quelltext | `main` |
| `BDBB_BACKUP` | `0` = keine tägliche Sicherung einrichten | `1` |
| `BDBB_KEEP_BUILD` | `1` = Go-Caches nach dem Bauen behalten (schnellere Updates, etwa 1 GB Platz) | `0` |

Was wo landet:

| Pfad | Inhalt |
|---|---|
| `/usr/local/bin/bdbb-server` | der Server |
| `/etc/bdbb/bdbb.env` | Token, Adresse, Datenbankpfad (nur für root und die Gruppe `bdbb` lesbar) |
| `/etc/init.d/bdbb` | OpenRC-Dienst, startet beim Booten (`rc-service bdbb status`, `restart`, `stop`) |
| `/var/lib/bdbb/bdbb.db` | Datenbank (nur Chiffretext) |
| `/var/log/bdbb/bdbb.log` | Protokoll (ohne Inhalte und ohne Token) |
| `/etc/periodic/daily/bdbb-backup` | tägliche Sicherung nach `/var/backups/bdbb/`, 14 Tage aufbewahrt (über `crond`) |
| `/opt/bdb-butler/src` | Quelltext (nur beim Bauen aus dem Quelltext) |

Hinweise:

- Der Server läuft als eigener Nutzer `bdbb` ohne Anmelderecht. Das Token wird aus der Datei gelesen und steht nicht in der Prozessliste.
- Mit fertiger Datei aus dem Release braucht der Container weder Go noch viel Speicher. Fertige Dateien gibt es für `x86_64` und `aarch64`.
- Nur beim Bauen aus dem Quelltext: Der Container braucht kurzzeitig etwa 1,5 GB RAM und 1 GB freien Platz, das Skript warnt bei weniger Speicher und räumt danach die Go-Caches wieder weg. Alpines Go-Paket ist oft älter als die Version in `server/go.mod`, dann lädt das Skript die passende Go-Version automatisch von `proxy.golang.org` nach.
- Stimmt die Prüfsumme einer heruntergeladenen Datei nicht, bricht das Skript ab, ohne etwas zu ersetzen.
- **Update:** das Skript einfach erneut ausführen. Es holt den neuesten Release (oder baut neu), ersetzt die Server-Datei und startet den Dienst neu. Token, Einstellungen und Daten bleiben erhalten.
- Prüft das Skript am Ende die öffentliche Adresse vergeblich, liegt das oft nur daran, dass der Container sich selbst nicht über den Proxy erreicht. Dann von einem anderen Rechner aus `curl https://butler.example.de/healthz` aufrufen (mit deiner Adresse).

## 3. App aus dem Quelltext bauen

Am einfachsten baut GitHub die App für dich (3.0). Selbst bauen (3.1 und 3.2) geht ebenfalls, dann auf dem Zielsystem: Linux-Pakete unter Linux, den Windows-Installer unter Windows. WSL erzeugt Linux-Pakete, keinen Windows-Installer.

### 3.0 Bauen auf GitHub

Der Workflow `.github/workflows/build.yml` baut die App auf Rechnern von GitHub für Windows und Ubuntu und den Sync-Server für Linux, nachdem die Tests durchgelaufen sind. Für ein öffentliches Repository ist das kostenlos, ein Lauf dauert etwa 15 bis 20 Minuten.

**Direkte Links:**

- Fertige Releases: <https://github.com/dennismarcbusch/bdb-butler/releases> (ein Release-Entwurf ist nur für den Repository-Besitzer sichtbar, bis er veröffentlicht ist)
- Läufe des Workflows „Bauen“ mit den Artifacts: <https://github.com/dennismarcbusch/bdb-butler/actions/workflows/build.yml>

**Einzelner Bau:**

1. Auf GitHub im Repository **Actions** öffnen und links den Workflow **„Bauen“** wählen.
2. **„Run workflow“**, Branch `main`, dann wieder **„Run workflow“**.
3. Ist der Lauf fertig (grüner Haken), unten auf seiner Seite unter **Artifacts** die Datei `BdB-Butler-Windows` oder `BdB-Butler-Ubuntu` herunterladen und entpacken. Darin liegen die Installer (Abschnitt 4).

Die Dateien unter Artifacts hält GitHub 90 Tage vor.

**Release mit Versionsnummer:**

1. Version in `app/src-tauri/tauri.conf.json` (`"version"`) erhöhen und auf `main` bringen.
2. Ein passendes Tag setzen und pushen:

   ```bash
   git tag v0.1.0
   ```

   ```bash
   git push origin v0.1.0
   ```

3. Der Workflow baut und legt unter [**Releases**](https://github.com/dennismarcbusch/bdb-butler/releases) einen **Entwurf** mit allen Installern und der Server-Datei (`bdbb-server-linux-amd64`, `-arm64`, Prüfsummen in `SHA256SUMS-server.txt`) an. Prüfen und auf **„Publish release“** klicken. Danach steht er dauerhaft auf der Repository-Seite zum Herunterladen, und das Installationsskript für Alpine (2.5) nimmt die Server-Datei von dort.

Bei jedem Push auf `main` und bei Pull Requests laufen nur die Tests, ohne zu bauen.

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

Im Ordner mit dem heruntergeladenen Paket (beim eigenen Bau `target/release/bundle/deb/`):

```bash
sudo apt install ./*_amd64.deb
```

Alternativ das AppImage ausführbar machen und starten (`chmod +x`, keine Installation nötig).

**Windows:** Den Installer ausführen: `BdB-Butler_…_x64-setup.exe` (empfohlen) oder `…_x64_en-US.msi`, aus dem Download von GitHub oder beim eigenen Bau aus `target/release/bundle/nsis/` beziehungsweise `msi/`. Da die Pakete nicht signiert sind, warnt Windows SmartScreen beim ersten Start („Weitere Informationen“, dann „Trotzdem ausführen“).

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

## 9. KI-Zugriff (MCP) einrichten

Die App enthält einen lokalen MCP-Server, über den eine KI auf deine Einträge zugreifen kann. Er läuft nur, solange die App läuft **und entsperrt ist**, und ist nur vom eigenen Rechner aus erreichbar (`127.0.0.1`).

### 9.1 In der App einschalten

**Einstellungen, KI-Zugriff (MCP)** (nur bei entsperrter App):

1. **„KI-Zugriff einschalten“** ankreuzen. Standardport ist 8765.
2. Optional den **Schreibzugriff** einschalten: Dann darf die KI Einträge anlegen und ändern. Löschen und Mailversand gibt es über die KI nie.
3. **Typen mit Personenbezug** (Ansprechpartner, beratene Personen, Protokolle) sind gesperrt. Gib sie nur frei, wenn du das bewusst willst. Die Datenschutzfreigabe deckt den Zugriff ab, du achtest selbst darauf, dass über die KI keine personenbezogenen Daten verarbeitet werden.
4. Die App zeigt Adresse, Token und einen fertigen Befehl für Claude Code. Über **„Neues Token erzeugen“** machst du alte Zugänge ungültig.

### 9.2 Claude Code verbinden

Den angezeigten Befehl ausführen, etwa:

```bash
claude mcp add --transport http bdb-butler http://127.0.0.1:8765/mcp --header "Authorization: Bearer <Token>"
```

Andere MCP-Clients, die Streamable HTTP mit eigenem Kopf `Authorization` beherrschen, verbindest du mit derselben Adresse und demselben Token.

### 9.3 Was die KI sieht

- Werkzeuge: `list_kinds`, `list_records`, `get_record`, `search`; mit Schreibzugriff `create_record` und `update_record`.
- Gesperrte Typen bleiben auch per ID und Suche unsichtbar. Verweise (IDs) auf sie können in anderen Einträgen stehen, ihre Inhalte nicht.
- Dateipfade und Versandzeitpunkte der Dienstreisen verwaltet die App selbst. Die KI kann sie nicht ändern.
- Jeder Aufruf steht im Protokoll (Einstellungen, KI-Zugriff), ohne Inhalte.
- Ist die App gesperrt, antwortet der Server mit „Die App ist gesperrt. Bitte in der App entsperren.“

## 10. Sicherung und Wiederherstellung

| Was | Wie sichern | Wiederherstellen |
|---|---|---|
| **Recovery-Key** | offline, ausgedruckt oder abgeschrieben | Entsperren mit dem Recovery-Key auf dem Sperrbildschirm |
| **Server-Datenbank** | regelmäßig `/var/lib/bdbb/bdbb.db` sichern (Chiffretext, darf auch auf weniger vertrauenswürdige Ziele) | zurückspielen und neu befüllen (Abschnitt 10.1) |
| **Ordner für PDFs und Anhänge** | über Nextcloud (Versionen, Papierkorb) und die übliche Nextcloud-Sicherung | aus Nextcloud |
| Lokaler Datenordner eines Rechners | nicht nötig, solange der Server läuft | App neu installieren und mit dem Tresor verbinden (Abschnitt 6) |

Eine konsistente Kopie der Server-Datenbank im laufenden Betrieb erstellst du mit SQLite selbst (Paket `sqlite3`):

```bash
sqlite3 /var/lib/bdbb/bdbb.db ".backup '/var/backups/bdbb-$(date +%F).db'"
```

Alternativ den Dienst kurz anhalten (`systemctl stop bdbb`), die Datei kopieren und wieder starten.

### 10.1 Server nach einem Verlust wieder in Betrieb nehmen

Jeder Rechner hat den vollständigen Stand. Solange einer davon intakt ist, gehen keine Daten verloren, auch wenn der Server leer ist oder nur eine ältere Sicherung hat. Die Rechner merken sich aber, bis zu welcher Folgenummer sie den Server kennen. Passt der Server nicht mehr dazu, bricht der Abgleich ab, bevor er etwas einspielt oder sendet, und meldet zum Beispiel „Der Server ist leer …“ oder „Der Server hat einen älteren Stand als dieser Rechner …“.

So nimmst du den Server wieder in Betrieb:

1. Server neu aufsetzen (Abschnitt 2) oder die Sicherung zurückspielen: Dienst anhalten, Datei ersetzen, Dienst starten.

   ```bash
   systemctl stop bdbb
   ```

   ```bash
   install -o bdbb -g bdbb -m 0600 /var/backups/bdbb-2026-10-01.db /var/lib/bdbb/bdbb.db
   ```

   ```bash
   rm -f /var/lib/bdbb/bdbb.db-wal /var/lib/bdbb/bdbb.db-shm
   ```

   ```bash
   systemctl start bdbb
   ```

   Das Token bleibt dasselbe (`BDBB_TOKEN`), sonst in jeder App neu eintragen.
2. Auf **einem** Rechner, am besten dem, auf dem du zuletzt gearbeitet hast: Einstellungen, Abgleich, „Jetzt abgleichen“. Die App erkennt den Verlust und zeigt „Server neu befüllen“. Nach der Bestätigung sendet sie ihren ganzen Stand. Liegt eine ältere Sicherung auf dem Server, wird sie je Feld zusammengeführt: Die jüngere Änderung gewinnt, also der Stand der Rechner. Der Keyring dieses Rechners ersetzt den der Sicherung, damit gilt sein Master-Passwort.
3. Auf **jedem weiteren** Rechner: abgleichen. Die App meldet „Ein anderer Rechner hat den Server neu befüllt“ und bietet ebenfalls „Server neu befüllen“ an. Nach der Bestätigung übernimmt sie den Keyring des Servers und sendet ihre Einträge. Was dort offline geändert wurde, kommt dazu. Gleiche Werte ergeben keine Konflikte.

Bis alle Rechner das getan haben, können sie offline ohne Einschränkung weiterarbeiten. Ein Rechner mit älterer App-Fassung erkennt den neu befüllten Server nicht sicher. Aktualisiere deshalb alle Rechner, bevor du den Server neu befüllst (Abschnitt 11).

Die Sicherung der Server-Datenbank ist die Notfallkopie für den Fall, dass kein Rechner mehr intakt ist: Mit ihr und dem Recovery-Key lässt sich ein neuer Rechner einrichten (Abschnitt 6). Solange ein Rechner intakt ist, reicht dagegen auch ein leerer Server. Proxmox-Sicherungen des Containers schaden nicht.

## 11. Updates

**Server unter Alpine mit Skript (Abschnitt 2.5):** `sh install-alpine.sh https://butler.example.de` erneut ausführen.

**Server von Hand:** neu bauen (Abschnitt 2.2), Datei ersetzen und den Dienst neu starten:

```bash
install -m 0755 bdbb-server /usr/local/bin/bdbb-server
```

```bash
systemctl restart bdbb
```

**App:** im Repository den neuen Stand holen und neu bauen (Abschnitt 3.2), dann das neue Paket installieren. Die Daten im Datenordner bleiben erhalten. Aktualisiere am besten alle Rechner zeitnah: Ein Rechner mit älterer App überspringt Einträge im neueren Format und meldet sie, bis er aktualisiert ist.

## 12. Fehlersuche

| Meldung oder Verhalten | Ursache und Abhilfe |
|---|---|
| „Der Server hat das Token abgelehnt“ | Token in der App stimmt nicht mit `BDBB_TOKEN` überein. In Einstellungen, Server und Ordner neu eintragen |
| „Server nicht erreichbar“ | Adresse prüfen (mit `https://`), `curl https://butler.example.de/healthz`, Caddy und `systemctl status bdbb` prüfen (Alpine: `rc-service bdbb status`, Protokoll in `/var/log/bdbb/bdbb.log`) |
| „Auf dem Server liegt ein anderer Tresor als lokal“ | Der Rechner hat offline einen eigenen Tresor angelegt, auf dem Server liegt schon einer. Den lokalen Datenordner entfernen (Abschnitt 4) und mit „Mit vorhandenem Tresor verbinden“ neu einrichten. Achtung: Was nur lokal existierte, geht dabei verloren |
| Hinweis „neuerer Keyring, dessen Prüfsumme nicht passt“ | Der Keyring auf dem Server stammt nicht von diesem Tresor oder von einer älteren App-Fassung. Alle Rechner aktualisieren und das Passwort auf einem davon neu setzen. Tritt das ohne Passwortwechsel auf, den Server prüfen |
| „Der Server ist leer, er hat seinen Bestand verloren“ | Der Server wurde neu aufgesetzt oder seine Datenbank ist weg. In den Einstellungen unter Abgleich „Server neu befüllen“ wählen (Abschnitt 10.1) |
| „Der Server hat einen älteren Stand als dieser Rechner“ | Eine ältere Sicherung wurde zurückgespielt. „Server neu befüllen“ wählen, die neueren Stände der Rechner gewinnen (Abschnitt 10.1). Tritt das ohne Zurückspielen auf, den Server prüfen (richtige Datenbank in `BDBB_DB`?) |
| „Ein anderer Rechner hat den Server neu befüllt“ | Auch auf diesem Rechner „Server neu befüllen“ wählen. Danach gilt das Master-Passwort des Rechners, der zuerst neu befüllt hat |
| „Ein anderer Rechner hat den Server gerade neu befüllt. Bitte … noch einmal wählen“ | Zwei Rechner haben gleichzeitig neu befüllt. Einfach noch einmal „Server neu befüllen“ wählen |
| Hinweis „Einträge ließen sich nicht einspielen“ | Ein Eintrag ist beschädigt oder stammt von einer neueren App-Fassung. Diese App aktualisieren. Der Abgleich läuft für alle anderen Einträge weiter |
| „Bitte zuerst den Ordner für PDFs und Anhänge … eintragen“ | Abschnitt 7 |
| „Bitte zuerst die Stammdaten vervollständigen“ | Abschnitt 8, die genannten Felder ausfüllen |
| Mailversand schlägt fehl | Host, Port und Verschlüsselung prüfen (STARTTLS/587 oder SSL/TLS/465), SMTP-Passwort auf diesem Rechner eintragen, Größe der Anhänge prüfen |
| KI-Client bekommt 401 | Token in der App neu kopieren, gegebenenfalls „Neues Token erzeugen“ und Client neu verbinden |
| KI-Client bekommt 503 | KI-Zugriff ist aus oder die App ist gesperrt |
| KI-Zugriff startet nicht („Port …“) | Ein anderes Programm belegt den Port. In den Einstellungen einen anderen Port (ab 1024) wählen und den Client anpassen |
| Master-Passwort vergessen | Auf dem Sperrbildschirm mit dem Recovery-Key entsperren, dann unter Einstellungen ein neues Passwort setzen |
