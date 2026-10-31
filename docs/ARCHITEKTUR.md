# Architektur BdB-Butler

Stand: 2026-10-31. Anforderungen: siehe `ANFORDERUNGEN.md`.

## Überblick

Local-first. Jeder Rechner hält alle Daten in einer lokalen Datenbank. Ein Server speichert nur Chiffretext und verteilt ihn zwischen den Rechnern.

```
 Rechner A (Tauri-App)             Server (Go, LXC)             Rechner B (Tauri-App)
 ┌───────────────────────┐        ┌────────────────────┐        ┌───────────────────────┐
 │ Oberfläche (TypeScript)│        │ nur Chiffretext    │        │ Oberfläche (TypeScript)│
 │ Kern (Rust): Krypto,   │◄──────►│ Einträge + Keyring │◄──────►│ Kern (Rust): Krypto,   │
 │ lokale DB, Sync, MCP   │  TLS   │ Bearer-Token       │  TLS   │ lokale DB, Sync, MCP   │
 └──────────┬────────────┘        └────────────────────┘        └──────────┬────────────┘
            │ lokal                                                        │ lokal
   Ordner für PDFs und Anhänge  ◄──── Nextcloud (oder anderes Tool) ────►  Ordner
   Token und SMTP-Passwort verschlüsselt in der lokalen Datenbank
```

## Komponenten

| Ordner | Sprache | Inhalt | Stand |
|---|---|---|---|
| `core/` | Rust | Krypto-Kern, Eintragsmodell, lokale Datenbank, Sync-Client, Dienstschicht und lokaler MCP-Server. Wird von der App eingebunden. | Krypto, Einträge, Datenbank, Sync-Client angelegt und getestet (auch gegen den echten Go-Server) |
| `server/` | Go | Sync-Server für Chiffretext. | angelegt und getestet |
| `app/` | TypeScript (Preact), Rust (Tauri) | Oberfläche und Tauri-Hülle. Die Hülle hat nur einen Befehl `call`, der an `bdb_core::api::call` weiterreicht. | Einrichten, Entsperren, Einstellungen, Listen und Formulare, Konflikte; Hülle kompiliert, Oberfläche im Browser gegen den Kern getestet |

Der Kern gehört in Rust, weil der lokale MCP-Server in dem Prozess laufen muss, der den entschlüsselten Datenschlüssel hält. Dieselbe Rust-Bibliothek läuft auch auf Android (Tauri 2). Die Oberfläche in TypeScript ruft den Kern über Tauri-Befehle auf und kennt weder den Schlüssel noch das Dateiformat.

## Schlüsselverwaltung

- Ein zufälliger **Datenschlüssel** (32 Byte) verschlüsselt alle Einträge. Er verlässt den Rechner nie im Klartext.
- Das **Master-Passwort** wird mit Argon2id (Salz 16 Byte, Parameter im Keyring gespeichert) zu einem Schlüssel für den Datenschlüssel abgeleitet.
- Der **Recovery-Key** ist ein zufälliger 256-Bit-Schlüssel, der bei der Einrichtung einmal angezeigt wird (Base32, in Vierergruppen) und offline aufbewahrt wird. Daraus wird per HKDF-SHA-256 ein zweiter Schlüssel für den Datenschlüssel abgeleitet.
- Der **Keyring** enthält den Datenschlüssel zweimal verpackt (XChaCha20-Poly1305), einmal mit dem Passwortschlüssel und einmal mit dem Recovery-Schlüssel, dazu die Argon2id-Parameter und das Salz. Er liegt auf dem Server, damit sich ein neuer Rechner einrichten lässt, und ist ohne Passwort oder Recovery-Key wertlos.
- Ein Passwortwechsel verpackt nur den Datenschlüssel neu. Die Daten werden nicht neu verschlüsselt.
- Der Keyring trägt eine **Prüfsumme** (`mac`, HMAC-SHA-256 über Version, Argon2id-Angaben, Salz und beide verpackten Schlüssel; Schlüssel per HKDF aus dem Datenschlüssel). Nur wer den Datenschlüssel kennt, kann sie bilden. Das Feld ist optional, ältere Clients ignorieren es, das Format bleibt Version 1.
- **Neuer Keyring auf anderen Rechnern:** Vor jedem Abgleich vergleicht der Client die Keyring-Version des Servers mit seiner eigenen. Ist die des Servers neuer und passt ihre Prüfsumme zum Datenschlüssel, übernimmt er sie. Nach einem Passwortwechsel auf Rechner A gilt damit nach dem nächsten Abgleich auch auf Rechner B nur noch das neue Passwort. Passt die Prüfsumme nicht (oder fehlt sie), bleibt der alte Keyring, und der Abgleich meldet einen Hinweis. So kann der Server einem Rechner keinen fremden Keyring unterschieben.
- Beim Entsperren wird eine vorhandene Prüfsumme geprüft. Ein Keyring, in dem Teile ausgetauscht wurden (zum Beispiel der Passwortteil durch den eines fremden Tresors), wird abgelehnt. Keyrings ohne Prüfsumme (vor ihrer Einführung angelegt) bekommen sie beim nächsten Abgleich und werden neu hochgeladen.
- Weitere Entsperrwege (zum Beispiel ein YubiKey) lassen sich später als zusätzlicher verpackter Datenschlüssel ergänzen.

Sicherheitsannahme: Wer den Serverbestand erbeutet, kann das Master-Passwort offline durchprobieren. Es sollte deshalb eine lange Passphrase sein (zum Beispiel sechs bis acht zufällige Wörter). Die Argon2id-Parameter sind bewusst teuer gewählt.

## Verschlüsselte Einträge

- Jeder Datensatz ist ein **Eintrag** mit ID (UUID) und einem Blob.
- Der Blob hat das Format `0x01 || Nonce (24 Byte) || Chiffretext || Tag` (XChaCha20-Poly1305). Der Schlüssel wird per HKDF aus dem Datenschlüssel abgeleitet. Die **ID ist als zusätzliche authentifizierte Daten (AAD) gebunden**, damit der Server Blobs nicht zwischen Einträgen vertauschen kann.
- Der Klartext ist ein JSON-Dokument (`core/src/record.rs`): `format` (Version, aktuell 1), `kind` (Typ, zum Beispiel `schule`, `person`, `protokoll`), `fields` und optional `deleted`. Jedes Feld in `fields` hat einen Wert `v` (beliebiges JSON, `null` leert das Feld) und einen Zeitstempel `t`.
- Der Zeitstempel ist eine hybride logische Uhr in der Form `<Millisekunden, 13 Stellen>-<Zähler, 4 Hexstellen>-<Knoten-ID>`. Die alphabetische Reihenfolge entspricht der zeitlichen. Jeder Rechner hat eine zufällige Knoten-ID. Die Uhr liefert streng aufsteigende Stempel, auch wenn die Systemzeit stehen bleibt oder zurückspringt, und übernimmt Stempel anderer Rechner.
- Löschen ist ein Register `deleted` mit Wert und eigenem Stempel (Tombstone). Ein gelöschter Eintrag lässt sich mit einem jüngeren Stempel wiederherstellen. Der Server kann ihn nicht von anderen Einträgen unterscheiden.
- Der Server kennt nur ID, Folgenummer, Größe und Änderungszeit. Er kennt weder Typ noch Inhalt.

## Sync-Protokoll

HTTP mit JSON, Bearer-Token (für einen Nutzer reicht ein langes zufälliges Token, das in der App hinterlegt ist). Das Token schützt nur vor unbefugtem Zugriff auf den Chiffretext. Die Vertraulichkeit der Daten hängt allein an der Verschlüsselung.

| Endpunkt | Zweck |
|---|---|
| `GET /v1/keyring` | Keyring abrufen (404, wenn noch keiner existiert). |
| `PUT /v1/keyring` | Keyring anlegen oder ersetzen. Mit `base_version`; stimmt sie nicht mit der aktuellen überein, antwortet der Server mit 409. |
| `GET /v1/entries?since=N&limit=M` | Alle Einträge mit Folgenummer größer als `N`, aufsteigend. Antwort enthält `latest`. |
| `PUT /v1/entries/{id}` | Eintrag schreiben. Der Body enthält `base_seq` (Folgenummer des Stands, auf dem die Änderung beruht, 0 für neue Einträge) und `blob`. Stimmt `base_seq` nicht mit der aktuellen Folgenummer des Eintrags überein, antwortet der Server mit 409 und dem aktuellen Eintrag. |
| `GET /healthz` | Lebenszeichen ohne Anmeldung. |

Der Server vergibt bei jedem Schreiben eine global aufsteigende Folgenummer (`seq`). Der Client merkt sich die höchste empfangene Folgenummer und holt damit nur Neues.

**Unlesbare Einträge:** Lässt sich ein Eintrag vom Server nicht entschlüsseln oder einspielen (beschädigt, neueres Format einer aktualisierten App, anderer Typ als lokal), überspringt der Client ihn und meldet ihn als Hinweis mit seiner ID. Alle anderen Einträge spielt er trotzdem ein. Der gespeicherte Cursor bleibt vor dem ersten übersprungenen Eintrag stehen, damit er beim nächsten Abgleich erneut versucht wird (zum Beispiel nach einem Update). Bis dahin holt jeder Abgleich die Einträge ab dort noch einmal, bereits bekannte werden übersprungen.

**Zusammenführen:** Beim Einspielen eines Serverstandes führt der Client lokalen und fremden Stand je Feld zusammen, der jüngere Zeitstempel gewinnt. Das Ergebnis ist kommutativ, assoziativ und idempotent und hängt nicht von der Reihenfolge der Abgleiche ab (durch Eigenschaftstests abgesichert). Bei 409 wird der aktuelle Serverstand geholt, zusammengeführt und erneut gesendet.

**Konflikte:** Lokal bleibt neben dem aktuellen Stand der zuletzt synchronisierte Stand (`base`). Ein echter Konflikt liegt vor, wenn ein Feld seit `base` auf beiden Seiten verschieden geändert wurde (Dreiwegevergleich). Änderungen an verschiedenen Feldern sind kein Konflikt. Bei einem echten Konflikt gewinnt der jüngere Stempel, und der Client legt einen verschlüsselten Eintrag vom Typ `konflikt` an (Felder `eintrag`, `eintragstyp`, `feld`, `lokal`, `fremd`). Er wird wie jeder Eintrag synchronisiert, damit jeder Rechner den Konflikt sieht und der Nutzer ihn auflösen kann. So geht nie stillschweigend ein Text verloren, auch nicht bei Markdown-Protokollen.

## Lokaler MCP-Server

Umgesetzt in `core/src/mcp.rs`, Einstellungen in der App unter „Einstellungen, KI-Zugriff“.

- **Transport:** Streamable HTTP (`POST /mcp`, JSON-RPC, Antworten als `application/json`, keine Sitzungen). Getestet mit dem offiziellen MCP-SDK (TypeScript) als Client. Unterstützte Protokollversionen: 2025-06-18, 2025-03-26, 2024-11-05.
- **Läuft im Prozess der App** und hat damit Zugriff auf den entsperrten Datenschlüssel. Ist die App gesperrt, antworten die Werkzeuge mit einem Hinweis. Der Schlüssel verlässt den Prozess nie.
- **Aus, bis der Nutzer ihn einschaltet.** Er lauscht nur auf `127.0.0.1`, verlangt ein Bearer-Token (256 Bit, verschlüsselt in der lokalen Datenbank, siehe „Geheimnisse dieses Rechners“) und prüft `Host` und `Origin` gegen DNS-Rebinding.
- **Nur bei entsperrter App:** Token, Schreibstufe und Freigaben lassen sich nur bei entsperrter App ansehen und ändern (auch das Protokoll). Ist die App gesperrt, kennt der Server das Token nicht und antwortet mit 503 und einem Hinweis.
- **Einstellungen gelten sofort:** Token, Schreibstufe und Freigaben liest der Server bei jeder Anfrage neu. Nur Ein/Aus und der Port erfordern einen Neustart. Grund: Die HTTP-Bibliothek schließt bestehende Verbindungen nicht, wenn ein Server angehalten wird, Anfragen darauf hingen bis zum Zeitablauf des Clients.
- **Werkzeuge:** `list_kinds`, `list_records`, `get_record`, `search`; mit Schreibstufe zusätzlich `create_record` und `update_record`. Kein Löschen, kein Mailversand. Ohne Schreibstufe erscheinen die Schreibwerkzeuge nicht in der Liste und werden abgelehnt.
- **Typen mit Personenbezug** (Ansprechpartner) sind für die KI gesperrt, bis der Nutzer sie einzeln freigibt. Das gilt für Lesen, Suchen und Schreiben, auch über `update_record` und `get_record` per ID. Die Fehlermeldung enthält keine Inhalte. Verweise (IDs) auf gesperrte Einträge in zugänglichen Einträgen bleiben sichtbar, die Inhalte nicht.
- **Protokoll** (`mcp-audit.log` im Datenordner, wird bei 1 MiB rotiert, in der App einsehbar): Zeit, Werkzeug, Typ, ID, Feldnamen und Ergebnis, nie Inhalte.
- Die Datenschutzfreigabe schließt den MCP-Zugriff auf Klartext ein. Der Nutzer achtet selbst darauf, dass über die KI keine personenbezogenen Daten verarbeitet werden.

## PDFs, Anhänge und Mail

- PDFs (Antrag, Abrechnung) werden lokal erzeugt und in dem Ordner abgelegt, den die App je Rechner angegeben bekommt. Anhänge (Hotelrechnungen, Teilnahmebescheinigungen, Veranstalter-Formulare) liegen ebenfalls dort.
- Die Synchronisation dieses Ordners übernimmt Nextcloud oder ein anderes Tool. Die App synchronisiert diese Dateien nicht.
- Die Dateien liegen im Klartext auf dem verschlüsselten Laptop und im Nextcloud-Ordner. Der Ordner wird bewusst nicht zusätzlich verschlüsselt (entschieden).
- Mail wird im Client per SMTP versendet. Das SMTP-Passwort liegt verschlüsselt in der lokalen Datenbank und wird nicht synchronisiert (siehe „Geheimnisse dieses Rechners“).

## Entscheidungen im Überblick

| Frage | Entscheidung |
|---|---|
| Architektur | Local-first mit verschlüsseltem Sync |
| Client | Tauri, Oberfläche in TypeScript, Kern in Rust |
| Server | Go. Rust wurde erwogen: Der Go-Server baut auf 13 Fremdpaketen auf, ein Rust-Server auf etwa 40 (minimal) bis 80 (mit axum) Crates. Der Rust-Kern hat bereits 54. |
| Sync | Eigenes schlankes Protokoll mit verschlüsselten Einzeleinträgen statt Evolu, Automerge oder CouchDB |
| Entsperren | Master-Passwort und Recovery-Key |
| KI | Lokaler MCP-Server im App-Prozess, von Anfang an |
| PDFs und Anhänge | Ordner, per Nextcloud synchronisiert |

Begründung für das eigene Sync-Protokoll: Es gibt genau einen Nutzer und wenig Daten. Die Schlüsselverwaltung (Master-Passwort und Recovery-Key) ist vorgegeben. Fertige Bausteine bringen eigene Schlüssel- und Servermodelle mit, die dazu nicht passen, und hängen von jungen Bibliotheken ab. Ein Protokoll mit einer Tabelle und drei Endpunkten ist überschaubar, gut zu testen und bleibt unter eigener Kontrolle.

## Oberfläche und Dienstschicht

- Alle Fachlogik liegt im Kern (`core/src/service.rs`): Zustand (eingerichtet, gesperrt, entsperrt), Tresor anlegen, beitreten, entsperren, Einträge anlegen, ändern, löschen, Abgleich. Typen und Felder stehen in `core/src/schema.rs` und gelten für Formulare, Prüfung beim Speichern und später den MCP-Server.
- `core/src/api.rs` bietet diese Dienste als JSON-Befehle an (`status`, `unlock`, `list`, `save`, `sync` …). Die Tauri-Hülle hat genau einen Befehl `call`, der Befehl und Argumente dorthin weiterreicht. Der MCP-Server wird denselben Einstiegspunkt benutzen.
- Im Browser (Entwicklung, Tests) ersetzt die HTTP-Brücke `dev_bridge` die Tauri-Hülle. Sie lauscht nur auf localhost, prüft `Host` und `Origin` und ist nicht für den Betrieb gedacht.
- Die Oberfläche bildet Formulare aus den Typdefinitionen. Neue Typen erscheinen damit ohne Oberflächencode. Besondere Ansichten (Graph, Dienstreise, Fortbildungsprozess) kommen als eigene Komponenten dazu.
- **Graph-Ansicht** (`app/src/graph.ts`, `GraphView.tsx`): Knoten sind alle Einträge, Kanten ergeben sich aus den Verweisfeldern der Typdefinitionen. Neue Typen mit Verweisen erscheinen damit automatisch. Das Layout ist ein kraftbasiertes Verfahren (Fruchterman und Reingold) mit deterministischen Startpositionen, festem Wunschabstand (110 Einheiten) und Schwerkraft (0.3). Die Werte sind per Messreihe gewählt: Ein Wunschabstand, der von der Zeichenfläche abhängt, ergab für wenige Knoten riesige Abstände, und zu schwache Schwerkraft ließ unverbundene Knoten an den Rand wandern und verbundene zu einem Haufen zusammenrücken. Die Ansicht hat Zoomen, Verschieben, Auswahl mit Nachbarn, Hervorheben per Suche und Filter nach Typ. In jedem Eintrag steht außerdem die Liste der verknüpften Einträge.
- Das Master-Passwort mindestens 10 Zeichen. Nach 15 Minuten ohne Eingabe sperrt sich die App.
- Einstellungen dieses Rechners (Server-Adresse, Ordner für Anhänge) liegen in `settings.json` im Datenordner der App (Zugriffsrechte 0600 unter Unix, die Datei entsteht gleich mit diesen Rechten). Das Token steht nicht darin.

### Geheimnisse dieses Rechners

- Sync-Token, MCP-Token und SMTP-Passwort (`core/src/secrets.rs`) liegen in der Tabelle `meta` der lokalen Datenbank, verschlüsselt wie ein Eintrag (XChaCha20-Poly1305 mit dem Eintragsschlüssel). Als ID für die zusätzlichen authentifizierten Daten dient `local/<Name>`. Sie enthält `/`, das in Eintrags-IDs des Servers nicht vorkommen kann, damit sich ein Geheimnis nie gegen einen Eintrag tauschen lässt. Sie werden nicht synchronisiert.
- Lesbar sind sie nur bei entsperrter App. Beim Sperren verlassen die Token den Speicher. Ob ein Token hinterlegt ist, lässt sich auch gesperrt prüfen (für die Anzeige „eingerichtet“).
- Bei der Einrichtung (vor dem Anlegen oder Beitreten) bleibt das eingegebene Sync-Token im Speicher und wird abgelegt, sobald der Tresor entsperrt ist.
- Ältere Fassungen legten die Werte im Klartext ab (`secrets.json`, Token in `settings.json` und `mcp.json`). Beim ersten Entsperren übernimmt die App sie, löscht `secrets.json` und schreibt die beiden anderen Dateien ohne Token neu.
- Der Schlüsselbund des Betriebssystems wird nicht verwendet. Die verschlüsselte Ablage funktioniert auf allen Plattformen gleich (auch Android) und braucht keinen Dienst wie den Secret Service unter Linux. Nachteil: Mail lässt sich nur bei entsperrter App senden, was ohnehin gilt.

## Beratungen, Protokolle und Markdown

- Typen (`core/src/schema.rs`): `beratene_person` (personenbezogen), `beratung` (verweist per ID auf Personen, nicht personenbezogen), `protokoll` (Art: Beratung, Jour fixe, Dienstbesprechung, Sonstiges; Verweise auf Schule und Beratung; Text als Markdown). **Protokolle sind für die KI zunächst gesperrt**, weil sie oft Personen nennen. Der Nutzer kann sie in den Einstellungen freigeben. Listen sortieren je Typ nach einem eigenen Feld (Beratungen und Protokolle: neueste zuerst).
- Markdown-Parser (`app/src/markdown.ts`): erzeugt einen Syntaxbaum, kein HTML. Die Oberfläche setzt den Baum in Preact-Knoten um, eingegebenes HTML und Skripte werden als Text angezeigt. Links sind nur mit `http:`, `https:` und `mailto:` erlaubt. Unterstützt werden Überschriften, Absätze (Zeilenumbrüche bleiben), verschachtelte und nummerierte Listen, Aufgaben (`- [ ]`), Zitate, Codeblöcke, **fett**, *kursiv*, `Code`, Links und Wikilinks.
- Wikilinks `[[Titel]]` und `[[Titel|Anzeigetext]]` verweisen auf Einträge mit diesem Titel (ohne Beachtung der Schreibweise, über alle Typen). Ist ein Titel mehrdeutig, bleibt der Link unaufgelöst, statt irgendeinen Eintrag zu treffen. Wikilinks in Markdown-Feldern erscheinen als Kanten im Graphen und in den verknüpften Einträgen beider Seiten.
- Aufgaben lassen sich in der Vorschau abhaken, das ändert den Quelltext.
- Gleichzeitige Änderungen am Protokolltext zweier Rechner ergeben einen Konflikteintrag mit beiden Fassungen (siehe „Konflikte“). Der Nutzer kann die andere Fassung mit einem Klick übernehmen.
- Formulare verbinden Beschriftung und Eingabefeld über `for` und `id`. Ein umschließendes Label würde bei Auswahlfeldern den Text aller Optionen in den Namen des Feldes aufnehmen.

## Auftragsbuch

- Typ `auftrag` (`core/src/schema.rs`): Titel, Auftraggeber (Ministerium, Sachgebietsleitung, Schulleitung, Sonstige), Status (Offen, In Arbeit, Wartet, Erledigt; Vorgabe „Offen“), Erteilt am, Frist, Erledigt am, Schule, Verweis auf das Protokoll, Beschreibung (Markdown). Auswahlfelder können einen Vorgabewert haben, der beim Anlegen gilt, wenn das Feld fehlt. Dadurch kann auch die KI Aufträge ohne Status anlegen.
- Die Liste ist nach Frist sortiert, Einträge ohne Sortierwert stehen am Ende. Filter: Offen (Standard), Alle, Erledigt. Jeder Eintrag zeigt die Fälligkeit in Worten („überfällig seit 3 Tagen“, „fällig in 2 Tagen“). Ein Abzeichen in der Navigation zählt überfällige offene Aufträge. Die Fristberechnung (`app/src/auftrag.ts`) ist eine reine Funktion des übergebenen Datums und deshalb testbar.
- **Aufträge aus Protokollen:** Jede Aufgabe (`- [ ] Text`) im Protokolltext hat die Schaltfläche „Auftrag anlegen“. Im Bearbeiten-Modus legt „Auftrag aus Markierung anlegen“ einen Auftrag aus markiertem Text an. Der Auftrag verweist auf das Protokoll und übernimmt dessen Schule und Datum (als „Erteilt am“). Ein Auftrag mit gleichem Titel aus demselben Protokoll wird nicht doppelt angelegt, stattdessen erscheint „Auftrag öffnen“. Das Protokoll muss dafür gespeichert sein. Aufträge sind nicht personenbezogen und damit auch für die KI zugänglich.

## Dienstreisemodul

Portierung des Dienstreisebusters (Python/Django) nach Rust und die App. Es läuft vollständig auf dem Rechner, der Server sieht nichts davon.

- **Typen:** `reise` (Dienstreise, alle Felder des alten Modells, Status Entwurf, Beantragt, Genehmigt, Abgerechnet, Bezahlt), `anhang` und `stammdaten` (beide versteckt: nicht in Navigation, Graph und für die KI). Die Stammdaten sind ein einziger Eintrag mit fester ID (`stammdaten-0000`), damit sich die Stände mehrerer Rechner zusammenführen lassen. Die Reisezeiten müssen in der Reihenfolge Antritt, Beginn Dienstgeschäft, Ende Dienstgeschäft, Ende der Reise liegen.
- **Dateien** (Antrag, Abrechnung, Genehmigung, Formular des Veranstalters, Anhänge) liegen im Ordner für PDFs und Anhänge unter `Dienstreisen/<Datum>-<Zweck>-<ID>/`, den Nextcloud synchronisiert. Die Einträge speichern nur den relativen Pfad. Jeder Zugriff prüft, dass der Pfad im Ordner bleibt (keine `..`, keine absoluten Pfade, keine Verknüpfungen aus dem Ordner heraus). Dateien öffnet die App über einen eigenen Befehl, der den Pfad vom Kern prüfen lässt. Die Oberfläche hat kein allgemeines Recht, Pfade zu öffnen.
- **PDF** (`core/src/pdf.rs`, `dienstreise.rs`): Der Antrag R001 füllt die mitgelieferte Vorlage (`core/assets/r001_antrag.pdf`). Die Abrechnung füllt das Formular, das der Veranstalter vorausgefüllt liefert. Die Unterschrift (SVG aus den Stammdaten) wird als Vektorpfade eingepasst. Abgesichert gegen die Python-Fassung: Alle 109 Feldwerte und Widget-Zustände sind identisch, die Darstellung ist pixelgleich (siehe `core/tests/fixtures/make_reference.py`). Abweichungen: Das Datum der Unterschrift im Antrag ist der Tag der Erzeugung, und ohne gewählte Mahlzeit kreuzt die Abrechnung keine an (Python kreuzte alle an). Formulare ohne Erscheinungsbild für Textfelder werden mit `NeedAppearances` vom Betrachter gezeichnet. Zentrierte und rechtsbündige Felder bekommen kein eigenes Erscheinungsbild.
- **Mail** (`core/src/mail.rs`): SMTP direkt vom Rechner (STARTTLS, TLS oder unverschlüsselt) mit Anmeldung und Anhängen, Entwurf nach dem Wortlaut der Python-Fassung, Auswahl der Anhänge, Grenze 20 MB für die Mail. Gemessen wird die kodierte Größe (Base64 mit Zeilenumbrüchen, etwa ein Drittel mehr als die Dateien), an Dateien passen also etwa 14,6 MB. Die alte Fassung maß die Dateien selbst, Mails bis gut 26 MB wurden dann vom Mailserver abgelehnt. Nach erfolgreichem Versand wechselt der Status (Antrag: Entwurf zu Beantragt; Abrechnung: Entwurf, Beantragt und Genehmigt zu Abgerechnet). Schlägt der Versand fehl, bleibt alles unverändert. Die TLS-Pfade sind nicht gegen einen echten Server getestet (nur gegen einen Test-SMTP-Server ohne Verschlüsselung).
- **SMTP-Passwort** liegt nicht in den synchronisierten Daten, sondern verschlüsselt in der lokalen Datenbank (siehe „Geheimnisse dieses Rechners“).
- **R002** (`dienstreise::r002_values`): Für Reisen ohne Formular des Veranstalters füllt die App den Erstattungsantrag R002 des LfF (Stand 04/2025) mit den Grunddaten vor und legt ihn als Abrechnung ab (derselbe Mailweg). Befüllt wird nur, was sich eindeutig zuordnen lässt: Person, Bankverbindung (IBAN ohne Leerzeichen im Kammfeld), Reisegrund, Genehmigung (wenn hinterlegt), Zweck, Orte, Zeiten, Abfahrt und Rückkehr sowie Bahn-Fahrkarte mit Preis. Tagegeld, Übernachtung, Art der BahnCard, Mitfahrer und weitere Verkehrsmittel bleiben leer und sind im PDF zu ergänzen. Eine Unterschrift setzt die App dort nicht ein. Geprüft an der Darstellung der Vorlage, **nicht** von der Behörde. Poppler-basierte Betrachter zeichnen unangekreuzte Kästchen dünn, weil die Vorlage nach dem Ausfüllen `NeedAppearances` bekommt. Acrobat und Browser zeichnen sie neu.
- **Fahrkartenpreis** (`core/src/ticket.rs`): Aus dem Text der PDF-Fahrkarten wird ein Preis vorgeschlagen, den der Nutzer übernimmt. Das ist eine Heuristik (Schlüsselwörter wie „Gesamtpreis“ vor „Preis“, Steuer-, Rabatt- und BahnCard-Zeilen zählen nie, ohne Schlüsselwort nur ein einziger Betrag mit Währung). Sie ist an erfundenen Mustern und einem erzeugten Beispiel getestet, **nicht an echten Fahrkarten**. Bei Scans oder Bildern erkennt sie nichts.
- **Import aus dem alten Django-Tool** (`core/src/import.rs`, Einstellungen, „Import aus dem alten Dienstreisebuster“): Übernimmt Stammdaten (mit Unterschrift und SMTP-Passwort), Reisen, Dateien und Anhänge aus `db.sqlite3` und dem Ordner `media`. Die Daten bleiben im alten Tool unverändert, Dateien werden kopiert. Einträge bekommen feste IDs (`import-reise-<Nr>`), ein erneuter Import überspringt Vorhandenes und überschreibt nichts. Zeiten rechnet er von UTC nach Europe/Berlin um, ausgefüllte Stammdaten bleiben unangetastet. Fehler bei einzelnen Einträgen und Dateien (fehlend, Pfad aus dem Medienordner heraus) stehen im Bericht, sie brechen den Import nicht ab. Getestet gegen Daten, die das alte Tool selbst erzeugt hat (`core/tests/fixtures/make_django_fixture.py`), nicht gegen deine echte Datenbank.
- **Noch offen:** Tests der TLS-Pfade gegen einen echten Mailserver.

## Fortbildungen

- **Typen:** `bedarf` (Bedarfsanmeldung: Thema, Status Offen, Eingeplant oder Abgelehnt, Schule, Kontakt per ID, Anzahl Interessenten, Beschreibung) und `fortbildung` mit den Feldern der Planung (Referent, Inhalt, Datum und Uhrzeit, Ort, Anzahl Teilnehmer), der Ausschreibung (Datum, Anmeldeschluss, Veranstaltungsnummer in FIBS) und je einer Checkliste (Markdown mit Aufgaben) für Ausschreibung, Durchführung und Nachbereitung.
- **Prozess:** Bedarf, dann Planung, Ausschreibung in FIBS, Durchführung, Nachbereitung, Abgeschlossen (`core/src/fortbildung.rs`). Der Kern prüft jeden Phasenwechsel beim Speichern, die Oberfläche und die KI unterliegen derselben Regel. Vorwärts nur mit vollständigen Angaben: Ausschreibung braucht Referent, Inhalt, Beginn, Ort und geplante Teilnehmerzahl. Durchführung braucht die FIBS-Nummer. Abschluss braucht eine abgehakte Checkliste der Nachbereitung. Überspringen prüft jede Phase auf dem Weg. Zurück geht immer. Die Meldung nennt, was fehlt.
- **Bedarfsanmeldungen** lassen sich in der Fortbildung zuordnen (Status wird „Eingeplant“) und lösen. Die Ansicht zeigt die Summe der Interessenten neben der geplanten Teilnehmerzahl. Ein Zähler in der Navigation zeigt offene Anmeldungen.
- **Checklisten:** Standardvorschläge lassen sich einfügen, zum Beispiel für die Nachbereitung „Teilnahmebescheinigungen erstellen und versenden“, „Referentenabrechnung einreichen“, „Reisekostenabrechnungen versenden“. Die App erzeugt **keine** Teilnahmebescheinigungen und versendet sie nicht. Das bleibt eine Aufgabe der Checkliste.

## Tätigkeitsbericht

- Ansicht „Tätigkeitsbericht“ (`app/src/bericht.ts`, `ReportView.tsx`): Zeitraum frei wählbar oder als Voreinstellung (Kalenderjahr, Schuljahr 1.8. bis 31.7.). Ausgewertet werden alle Module: Beratungen (Anzahl, Zeit, nach Art und Schule), Fortbildungen (durchgeführt = Phase Durchführung oder später, mit Teilnehmenden; in Planung oder Ausschreibung getrennt), Dienstreisen (ohne Entwürfe), Protokolle nach Art, Aufträge (erteilt, erledigt, offen, überfällig) und Bedarfsanmeldungen.
- Der Bericht enthält Zahlen, Titel und Schulnamen, keine Personen (getestet). Er lässt sich als Markdown-Datei im Unterordner `Berichte` des Ordners für PDFs und Anhänge speichern (ein erneut gespeicherter Bericht gleichen Namens ersetzt den alten), kopieren oder drucken.
- Die Auswertung besteht aus reinen Funktionen und ist einzeln getestet, einschließlich Grenztagen, leerer Zeiträume und ungültiger Daten.
