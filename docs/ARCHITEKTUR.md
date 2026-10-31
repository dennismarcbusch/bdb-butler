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
   Mail-Zugangsdaten im Schlüsselbund des Betriebssystems
```

## Komponenten

| Ordner | Sprache | Inhalt | Stand |
|---|---|---|---|
| `core/` | Rust | Krypto-Kern, Eintragsmodell, lokale Datenbank, Sync-Client, Dienstschicht und lokaler MCP-Server. Wird von der App eingebunden. | Krypto, Einträge, Datenbank, Sync-Client angelegt und getestet (auch gegen den echten Go-Server) |
| `server/` | Go | Sync-Server für Chiffretext. | angelegt und getestet |
| `app/` | TypeScript (Preact), Rust (Tauri) | Oberfläche und Tauri-Hülle. Die Hülle hat nur einen Befehl `call`, der an `bdb_core::api::call` weiterreicht. | Einrichten, Entsperren, Einstellungen, Listen und Formulare, Konflikte; Hülle kompiliert, Oberfläche im Browser gegen den Kern getestet |
| `dienstreisebuster/` | Python (Django) | Bestandstool, wird durch das Dienstreisemodul in der App abgelöst. | läuft weiter bis Meilenstein 3 |

Der Kern gehört in Rust, weil der lokale MCP-Server in dem Prozess laufen muss, der den entschlüsselten Datenschlüssel hält. Dieselbe Rust-Bibliothek läuft auch auf Android (Tauri 2). Die Oberfläche in TypeScript ruft den Kern über Tauri-Befehle auf und kennt weder den Schlüssel noch das Dateiformat.

## Schlüsselverwaltung

- Ein zufälliger **Datenschlüssel** (32 Byte) verschlüsselt alle Einträge. Er verlässt den Rechner nie im Klartext.
- Das **Master-Passwort** wird mit Argon2id (Salz 16 Byte, Parameter im Keyring gespeichert) zu einem Schlüssel für den Datenschlüssel abgeleitet.
- Der **Recovery-Key** ist ein zufälliger 256-Bit-Schlüssel, der bei der Einrichtung einmal angezeigt wird (Base32, in Vierergruppen) und offline aufbewahrt wird. Daraus wird per HKDF-SHA-256 ein zweiter Schlüssel für den Datenschlüssel abgeleitet.
- Der **Keyring** enthält den Datenschlüssel zweimal verpackt (XChaCha20-Poly1305), einmal mit dem Passwortschlüssel und einmal mit dem Recovery-Schlüssel, dazu die Argon2id-Parameter und das Salz. Er liegt auf dem Server, damit sich ein neuer Rechner einrichten lässt, und ist ohne Passwort oder Recovery-Key wertlos.
- Ein Passwortwechsel verpackt nur den Datenschlüssel neu. Die Daten werden nicht neu verschlüsselt.
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

**Zusammenführen:** Beim Einspielen eines Serverstandes führt der Client lokalen und fremden Stand je Feld zusammen, der jüngere Zeitstempel gewinnt. Das Ergebnis ist kommutativ, assoziativ und idempotent und hängt nicht von der Reihenfolge der Abgleiche ab (durch Eigenschaftstests abgesichert). Bei 409 wird der aktuelle Serverstand geholt, zusammengeführt und erneut gesendet.

**Konflikte:** Lokal bleibt neben dem aktuellen Stand der zuletzt synchronisierte Stand (`base`). Ein echter Konflikt liegt vor, wenn ein Feld seit `base` auf beiden Seiten verschieden geändert wurde (Dreiwegevergleich). Änderungen an verschiedenen Feldern sind kein Konflikt. Bei einem echten Konflikt gewinnt der jüngere Stempel, und der Client legt einen verschlüsselten Eintrag vom Typ `konflikt` an (Felder `eintrag`, `eintragstyp`, `feld`, `lokal`, `fremd`). Er wird wie jeder Eintrag synchronisiert, damit jeder Rechner den Konflikt sieht und der Nutzer ihn auflösen kann. So geht nie stillschweigend ein Text verloren, auch nicht bei Markdown-Protokollen.

## Lokaler MCP-Server

Umgesetzt in `core/src/mcp.rs`, Einstellungen in der App unter „Einstellungen, KI-Zugriff“.

- **Transport:** Streamable HTTP (`POST /mcp`, JSON-RPC, Antworten als `application/json`, keine Sitzungen). Getestet mit dem offiziellen MCP-SDK (TypeScript) als Client. Unterstützte Protokollversionen: 2025-06-18, 2025-03-26, 2024-11-05.
- **Läuft im Prozess der App** und hat damit Zugriff auf den entsperrten Datenschlüssel. Ist die App gesperrt, antworten die Werkzeuge mit einem Hinweis. Der Schlüssel verlässt den Prozess nie.
- **Aus, bis der Nutzer ihn einschaltet.** Er lauscht nur auf `127.0.0.1`, verlangt ein Bearer-Token (256 Bit, in `mcp.json` mit Rechten 0600) und prüft `Host` und `Origin` gegen DNS-Rebinding.
- **Einstellungen gelten sofort:** Token, Schreibstufe und Freigaben liest der Server bei jeder Anfrage neu. Nur Ein/Aus und der Port erfordern einen Neustart. Grund: Die HTTP-Bibliothek schließt bestehende Verbindungen nicht, wenn ein Server angehalten wird, Anfragen darauf hingen bis zum Zeitablauf des Clients.
- **Werkzeuge:** `list_kinds`, `list_records`, `get_record`, `search`; mit Schreibstufe zusätzlich `create_record` und `update_record`. Kein Löschen, kein Mailversand. Ohne Schreibstufe erscheinen die Schreibwerkzeuge nicht in der Liste und werden abgelehnt.
- **Typen mit Personenbezug** (Ansprechpartner) sind für die KI gesperrt, bis der Nutzer sie einzeln freigibt. Das gilt für Lesen, Suchen und Schreiben, auch über `update_record` und `get_record` per ID. Die Fehlermeldung enthält keine Inhalte. Verweise (IDs) auf gesperrte Einträge in zugänglichen Einträgen bleiben sichtbar, die Inhalte nicht.
- **Protokoll** (`mcp-audit.log` im Datenordner, wird bei 1 MiB rotiert, in der App einsehbar): Zeit, Werkzeug, Typ, ID, Feldnamen und Ergebnis, nie Inhalte.
- Die Datenschutzfreigabe schließt den MCP-Zugriff auf Klartext ein. Der Nutzer achtet selbst darauf, dass über die KI keine personenbezogenen Daten verarbeitet werden.

## PDFs, Anhänge und Mail

- PDFs (Antrag, Abrechnung) werden lokal erzeugt und in dem Ordner abgelegt, den die App je Rechner angegeben bekommt. Anhänge (Hotelrechnungen, Teilnahmebescheinigungen, Veranstalter-Formulare) liegen ebenfalls dort.
- Die Synchronisation dieses Ordners übernimmt Nextcloud oder ein anderes Tool. Die App synchronisiert diese Dateien nicht.
- Die Dateien liegen im Klartext auf dem verschlüsselten Laptop und im Nextcloud-Ordner. Der Ordner wird bewusst nicht zusätzlich verschlüsselt (entschieden).
- Mail wird im Client per SMTP versendet. Die Zugangsdaten liegen im Schlüsselbund des Betriebssystems und werden nicht synchronisiert.

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
- Einstellungen dieses Rechners (Server, Token, Ordner für Anhänge) liegen in `settings.json` im Datenordner der App (Zugriffsrechte 0600 unter Linux). Das Token schützt nur den Zugriff auf Chiffretext. Eine Ablage im Schlüsselbund des Betriebssystems ist für die Mail-Zugangsdaten vorgesehen.

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
