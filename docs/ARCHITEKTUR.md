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
| `core/` | Rust | Krypto-Kern, Eintragsmodell, lokale Datenbank und Sync-Client; später MCP-Server. Wird von der App eingebunden. | Krypto, Einträge, Datenbank, Sync-Client angelegt und getestet (auch gegen den echten Go-Server) |
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

- Läuft im Prozess der App und hat damit Zugriff auf den entsperrten Datenschlüssel. Er funktioniert nur, solange die App läuft und entsperrt ist.
- Er lauscht ausschließlich auf localhost und verlangt ein Token. Claude Desktop oder Claude Code verbinden sich per HTTP, gegebenenfalls über einen kleinen stdio-Vermittler.
- Geplante Eigenschaften: Zugriffsstufen (nur lesen, lesen und schreiben), ein Protokoll der Schreibzugriffe, einstellbare Sichtbarkeit einzelner Felder für die KI, kein Löschen (nur Archivieren) und kein Mailversand.
- Entschieden: Typen mit Personenbezug (Ansprechpartner, beratene Personen) sind für die KI standardmäßig gesperrt und lassen sich einzeln freischalten. Das unterstützt die Vorgabe, dass über die KI keine personenbezogenen Daten verarbeitet werden. Die Verantwortung dafür bleibt beim Nutzer.
- Die Logik für Datenzugriff und Validierung liegt im Kern und wird von Oberfläche und MCP-Server gemeinsam genutzt.
- Die Datenschutzfreigabe schließt den MCP-Zugriff auf Klartext ein.
- Offen: Welche Tools genau und welche Felder sichtbar sind.

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
- Das Master-Passwort mindestens 10 Zeichen. Nach 15 Minuten ohne Eingabe sperrt sich die App.
- Einstellungen dieses Rechners (Server, Token, Ordner für Anhänge) liegen in `settings.json` im Datenordner der App (Zugriffsrechte 0600 unter Linux). Das Token schützt nur den Zugriff auf Chiffretext. Eine Ablage im Schlüsselbund des Betriebssystems ist für die Mail-Zugangsdaten vorgesehen.
