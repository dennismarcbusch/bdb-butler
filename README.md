# BdB-Butler

Desktop-App für die Arbeit als mBdB: Dienstreisen, Schulen und Ansprechpartner, Beratungen und Protokolle, Auftragsbuch, Fortbildungen und Tätigkeitsbericht an einem Ort. Gedacht für genau einen Nutzer auf mehreren Rechnern.

- **Local-first:** Jeder Rechner hält alle Daten in einer eigenen Datenbank und funktioniert auch offline.
- **Ende-zu-Ende verschlüsselt:** Ein kleiner Sync-Server verteilt die Daten zwischen den Rechnern, sieht aber nur Chiffretext.
- **KI-Zugriff:** Ein lokaler MCP-Server in der App lässt eine KI (zum Beispiel Claude) Einträge suchen, anlegen und ändern. Personenbezogene Typen sind dabei gesperrt, bis du sie freigibst.

Installation und Betrieb: **[docs/INSTALLATION.md](docs/INSTALLATION.md)**. Das deb-Paket für Ubuntu und Debian baut GitHub Actions (Workflow „Bauen“, siehe [Abschnitt 3.0](docs/INSTALLATION.md#30-bauen-auf-github)) und stellt es als apt-Paketquelle bereit ([Abschnitt 4](docs/INSTALLATION.md#4-app-installieren)). Windows-Installer, rpm und AppImage werden bis auf Weiteres nicht gebaut.

**Download:** [Releases](https://github.com/dennismarcbusch/bdb-butler/releases) (deb-Paket und Server-Datei, nach dem Veröffentlichen des Entwurfs) oder [Läufe des Workflows „Bauen“](https://github.com/dennismarcbusch/bdb-butler/actions/workflows/build.yml) (unten auf der Seite eines Laufs unter „Artifacts“).

## Inhalt

- [Funktionen](#funktionen)
- [Datenschutz und Sicherheit](#datenschutz-und-sicherheit)
- [Aufbau](#aufbau)
- [Schnellstart für die Entwicklung](#schnellstart-für-die-entwicklung)
- [Tests](#tests)
- [Stand und offene Punkte](#stand-und-offene-punkte)
- [Weitere Dokumentation](#weitere-dokumentation)

## Funktionen

### Schulen, Ansprechpartner und Graph

- Schulen (Schulart, Schulnummer, Adresse, Kontakt, Notizen) und ihre Ansprechpartner mit Rolle und Verknüpfungen untereinander.
- Eine **Graph-Ansicht** zeigt alle Einträge und ihre Verknüpfungen, etwa Ansprechpartner zu Schule oder Protokoll zu Beratung. Sie lässt sich zoomen und verschieben, filtern und durchsuchen. Neue Typen mit Verweisen erscheinen dort automatisch.
- Jeder Eintrag listet seine verknüpften Einträge auf.

### Beratungen und Protokolle

- Beratungen mit Verweis auf beratene Personen, Art (vor Ort, online, Telefon, schriftlich) und Dauer.
- Protokolle (Beratung, Jour fixe, Dienstbesprechung, Sonstiges) im **Markdown**-Format mit Überschriften, Listen, Aufgaben (`- [ ]`), Zitaten, Code und Links.
- **Wikilinks** `[[Titel]]` verweisen auf andere Einträge und erscheinen im Graphen.
- Aufgaben lassen sich in der Vorschau abhaken.

### Auftragsbuch

- Aufträge von Ministerium, Sachgebietsleitung oder Schulleitung mit Frist, Status und Verweis auf Schule und Protokoll.
- Liste nach Frist sortiert, mit Fälligkeit in Worten („überfällig seit 3 Tagen“) und einem Zähler für überfällige Aufträge in der Navigation.
- Aus jeder Aufgabe in einem Protokoll lässt sich mit einem Klick ein Auftrag anlegen, der auf das Protokoll verweist.

### Dienstreisen

- Reisen mit allen Angaben für Antrag und Abrechnung. Der Status läuft Entwurf, Beantragt, Genehmigt, Abgerechnet, Bezahlt.
- **Dienstreiseantrag R001** als ausgefülltes PDF mit eingesetzter Unterschrift (SVG aus den Stammdaten).
- **Abrechnung:** Das vom Veranstalter vorausgefüllte FIBS-Formular wird ergänzt. Ohne solches Formular füllt die App den Erstattungsantrag **R002** des LfF mit den Grunddaten vor.
- Der **Fahrkartenpreis** wird aus PDF-Fahrkarten vorgeschlagen (eine Heuristik, du übernimmst ihn selbst).
- **Mailversand** direkt vom Rechner per SMTP: der Antrag an die Verwaltung, die Abrechnung an das LfF mit auswählbaren Anhängen. Nach dem Versand wechselt der Status.
- Alle PDFs und Anhänge liegen in einem Ordner, den Nextcloud (oder ein anderes Werkzeug) zwischen den Rechnern synchronisiert.

### Fortbildungen

- Bedarfsanmeldungen von Schulen sammeln und einer Fortbildung zuordnen.
- Prozess Planung, Ausschreibung (FIBS), Durchführung, Nachbereitung, Abgeschlossen. Ein Phasenwechsel ist nur mit vollständigen Angaben möglich, und die App sagt, was fehlt.
- Checklisten für jede Phase mit Standardvorschlägen.

### Tätigkeitsbericht

- Auswertung über alle Module für einen frei wählbaren Zeitraum, Kalenderjahr oder Schuljahr: Beratungen, Fortbildungen, Dienstreisen, Protokolle, Aufträge und Bedarfsanmeldungen.
- Enthält Zahlen, Titel und Schulnamen, aber keine Personen.
- Lässt sich als Markdown speichern, kopieren oder drucken.

### Abgleich und Konflikte

- Abgeglichen wird beim Entsperren, kurz nach jeder Änderung und auf Knopfdruck („Jetzt abgleichen“ in den Einstellungen). Einen regelmäßigen Abgleich im Hintergrund gibt es nicht. Änderungen eines anderen Rechners kommen also spätestens beim nächsten dieser Anlässe an.
- Ändern zwei Rechner verschiedene Felder desselben Eintrags, werden beide Änderungen übernommen.
- Ändern sie dasselbe Feld, gewinnt die jüngere Änderung. Die andere Fassung bleibt als **Konflikt** erhalten und lässt sich mit einem Klick übernehmen. So geht kein Text stillschweigend verloren.
- Verliert der Sync-Server seinen Bestand (neu aufgesetzt oder aus einer älteren Sicherung zurückgespielt), merkt die App das und bietet „Server neu befüllen“ an. Jeder Rechner hat den vollständigen Stand, neuere Stände der Rechner gewinnen (siehe [Sicherung](docs/INSTALLATION.md#10-sicherung-und-wiederherstellung)).

### KI-Zugriff (MCP)

- Lokaler MCP-Server (Streamable HTTP auf `127.0.0.1`, Bearer-Token). Er ist aus, bis du ihn einschaltest.
- Werkzeuge: `list_kinds`, `list_records`, `get_record`, `search`. Mit eingeschalteter Schreibstufe kommen `create_record` und `update_record` dazu. Löschen und Mailversand gibt es über die KI nicht.
- Typen mit Personenbezug (Ansprechpartner, beratene Personen, Protokolle) sind gesperrt, bis du sie einzeln freigibst.
- Jeder Zugriff wird protokolliert (Werkzeug, Typ, ID, Feldnamen, nie Inhalte). Das Protokoll ist in der App einsehbar.

## Datenschutz und Sicherheit

| Was | Wo | Geschützt durch |
|---|---|---|
| Alle Einträge | lokale Datenbank und Sync-Server | XChaCha20-Poly1305 mit einem zufälligen Datenschlüssel |
| Datenschlüssel | Keyring (lokal und auf dem Server) | verpackt unter Master-Passwort (Argon2id, 256 MiB) und Recovery-Key |
| Sync-Token, MCP-Token, SMTP-Passwort | lokale Datenbank, nicht synchronisiert | mit dem Datenschlüssel verschlüsselt |
| PDFs und Anhänge | Ordner, per Nextcloud synchronisiert | Festplattenverschlüsselung des Rechners (bewusst nicht zusätzlich verschlüsselt) |

- Der Server kennt von jedem Eintrag nur ID, Folgenummer, Größe und Änderungszeit, aber weder Typ noch Inhalt.
- Entsperrt wird mit dem **Master-Passwort** oder dem **Recovery-Key**. Den Recovery-Key zeigt die App beim Einrichten genau einmal an. Bewahre ihn offline auf: Ohne Passwort und Recovery-Key sind die Daten nicht wiederherstellbar.
- Nach 15 Minuten ohne Eingabe sperrt sich die App. Gesperrt kennt sie weder den Datenschlüssel noch die Token, und der KI-Zugriff antwortet nur mit einem Hinweis.
- Ein Passwortwechsel gilt nach dem nächsten Abgleich auf allen Rechnern. Der Keyring trägt dafür eine Prüfsumme, damit der Server keinen fremden Keyring unterschieben kann.
- Wer den Serverbestand erbeutet, kann das Master-Passwort offline durchprobieren. Nimm deshalb eine lange Passphrase aus sechs bis acht zufälligen Wörtern.

Die Einzelheiten stehen in [docs/ARCHITEKTUR.md](docs/ARCHITEKTUR.md).

## Aufbau

```
 Rechner A (Tauri-App)          Server (Go, LXC)          Rechner B (Tauri-App)
 Oberfläche (Preact)            nur Chiffretext           Oberfläche (Preact)
 Kern (Rust): Krypto,  ◄──TLS──► Einträge + Keyring ◄──TLS──► Kern (Rust): Krypto,
 lokale DB, Sync, MCP           Bearer-Token              lokale DB, Sync, MCP
        │                                                        │
   Ordner für PDFs und Anhänge ◄──────── Nextcloud ────────► Ordner
```

| Ordner | Sprache | Inhalt |
|---|---|---|
| `core/` | Rust | Bibliothek `bdb-core`: Krypto, Einträge und Zusammenführung, lokale SQLite-Datenbank, Sync-Client, Dienstschicht, JSON-Schnittstelle, Typdefinitionen, MCP-Server, PDF-Erzeugung, Mail |
| `app/src-tauri/` | Rust | Tauri-Hülle. Ein einziger Befehl `call` reicht alles an den Kern weiter, dazu `open_file` zum Öffnen geprüfter Dateien |
| `app/src/` | TypeScript (Preact) | Oberfläche. Formulare entstehen aus den Typdefinitionen des Kerns |
| `server/` | Go | Sync-Server: eine SQLite-Datei, vier Endpunkte, nur Chiffretext |
| `docs/` | | Anforderungen, Architektur, Installation |

Die wichtigsten Dateien im Kern:

| Datei | Aufgabe |
|---|---|
| `crypto.rs` | Datenschlüssel, Keyring, Ver- und Entschlüsselung der Einträge |
| `record.rs` | Eintragsmodell mit Feld-Zeitstempeln (hybride logische Uhr), Zusammenführung, Konflikterkennung |
| `store.rs` | lokale Datenbank (nur Chiffretext), Dreiwegevergleich, verschlüsselte Geheimnisse |
| `sync.rs` | Sync-Client und Protokoll |
| `service.rs` | Dienstschicht: Tresor, Entsperren, Einträge, Abgleich, Einstellungen |
| `api.rs` | JSON-Befehle für Oberfläche und Entwicklungsbrücke |
| `schema.rs` | alle Typen und Felder; neue Typen gehören hierher |
| `mcp.rs` | lokaler MCP-Server mit Sperre für Personen-Typen |
| `pdf.rs`, `dienstreise.rs` | Formulare ausfüllen, Unterschrift einsetzen, R001, FIBS und R002 |
| `mail.rs`, `reisen.rs` | SMTP-Versand und Abläufe des Dienstreisemoduls |

## Schnellstart für die Entwicklung

Voraussetzungen: Rust (stabil, getestet mit 1.97), Go (Version laut `server/go.mod`, Go lädt sie bei Bedarf selbst), Node.js 22. Für die Tauri-App zusätzlich die Systembibliotheken aus der [Installationsanleitung](docs/INSTALLATION.md#3-app-aus-dem-quelltext-bauen).

Im Browser entwickeln (ohne Tauri): Drei Prozesse laufen nebeneinander.

```bash
cd server && BDBB_TOKEN=entwicklung-entwicklung-entwicklung-123 go run .
```

```bash
BDB_FAST_KDF=1 cargo run -p bdb-core --bin dev_bridge -- /tmp/bdb-dev 8787
```

```bash
cd app && npm install && npm run dev
```

Dann <http://localhost:5173> öffnen und als Server `http://127.0.0.1:8080` mit dem Token von oben eintragen. `BDB_FAST_KDF=1` macht Anlegen und Entsperren schnell (schwache Schlüsselableitung, nur für Tests). Die Brücke `dev_bridge` lauscht nur auf localhost und ist nicht für den Betrieb gedacht.

In der Tauri-Hülle entwickeln:

```bash
cd app && npm run tauri dev
```

## Tests

| Teil | Befehl | Hinweis |
|---|---|---|
| Kern | `cargo test -p bdb-core` | Der Integrationstest baut und startet den Go-Server (ohne Go wird er übersprungen) |
| Server | `cd server && go test -race ./...` | |
| Oberfläche | `cd app && npm test` und `npm run typecheck` | |
| Ende zu Ende | `cd app && npm run e2e` | Browser (Chromium/Playwright) gegen Kern und Go-Server, zwei Rechner, echter MCP-Client |

Die PDF-Tests prüfen gegen Referenzwerte aus der alten Python-Fassung (alle Feldwerte und die Darstellung). Zum Neuerzeugen dienen die Skripte in `core/tests/fixtures/`.

## Stand und offene Punkte

- Läuft: alle oben genannten Module. Kern, Server und Oberfläche sind getestet, die Oberfläche im Browser gegen den echten Kern.
- Die Tauri-Hülle kompiliert, ist aber unter Windows (WebView2) und Linux (WebKitGTK) noch nicht im Alltag geprüft.
- Nicht gegen die Wirklichkeit geprüft: die TLS-Verbindung zu einem echten Mailserver, die Preiserkennung an echten Fahrkarten und der R002 durch die Behörde.
- Android (Tauri 2 oder PWA) ist nicht begonnen.

## Weitere Dokumentation

- [docs/INSTALLATION.md](docs/INSTALLATION.md): Server einrichten, App bauen und installieren, erster Start, Mail, KI-Zugriff, Sicherung, Fehlersuche
- [docs/ANFORDERUNGEN.md](docs/ANFORDERUNGEN.md): Anforderungen und Umfang
- [docs/ARCHITEKTUR.md](docs/ARCHITEKTUR.md): Architektur, Kryptografie, Sync-Protokoll und Entscheidungen
- [server/README.md](server/README.md) und [core/README.md](core/README.md): Kurzbeschreibungen der Teile
