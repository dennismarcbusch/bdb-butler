# BdB-Butler

App für die mBdB-Tätigkeit (Dienstreisen, Schulen, Ansprechpartner, Beratungen, Protokolle, Auftragsbuch, Fortbildungen, Tätigkeitsbericht). Genau 1 Nutzer.

Local-first: Jeder Rechner hält alle Daten in einer lokalen Datenbank. Alle Daten werden Ende-zu-Ende verschlüsselt; der Sync-Server speichert und sieht nur Chiffretext. Entsperrt wird mit Master-Passwort und Recovery-Key. PDFs und Anhänge liegen als Dateien in einem Ordner, den Nextcloud synchronisiert. Ein lokaler MCP-Server in der App ermöglicht den KI-Zugriff.

Anforderungen: `docs/ANFORDERUNGEN.md`. Architektur und Entscheidungen: `docs/ARCHITEKTUR.md`. Beide Dateien sind maßgeblich und werden bei jeder Entscheidung mitgepflegt.

## Aufbau

- `core/`: Rust-Bibliothek `bdb-core`. Krypto, Einträge, lokale Datenbank, Sync-Client, Dienstschicht `service`, JSON-Schnittstelle `api`, Typdefinitionen `schema`, lokaler MCP-Server `mcp`, Dienstreisemodul (`pdf`, `dienstreise`, `mail`, `reisen`, `secrets`, `ticket`) und Fortbildungsprozess (`fortbildung`). Neue Typen und Felder gehören nach `schema.rs`, die Oberfläche bildet Formulare daraus. Tests: `cargo test -p bdb-core` (der Integrationstest baut und startet den Go-Server, dafür muss Go installiert sein). Die PDF-Tests prüfen gegen Referenzwerte aus der alten Python-Fassung (`core/tests/fixtures/make_reference.py`, nur zum Neuerzeugen nötig).
- `server/`: Go-Sync-Server für Chiffretext. Tests: `go test -race ./...` im Ordner `server/`.
- `app/`: Tauri-App. `app/src-tauri` ist die Rust-Hülle (ein Befehl `call` reicht alles an `bdb_core::api::call` weiter), `app/src` die Oberfläche (Preact, TypeScript, Vite). Im Browser spricht die Oberfläche über die Entwicklungsbrücke `dev_bridge` mit dem Kern. Tests: `npm test` (Einheiten), `npm run typecheck`, `npm run e2e` (Browser gegen Kern und Go-Server, braucht Chromium und Go).
- Das alte Django-Tool „Dienstreisebuster“ ist entfernt (Stand im Git-Verlauf: Commit `9e561ca`). Seine Daten übernimmt der Import in den Einstellungen; die Fixture-Skripte in `core/tests/fixtures/` brauchen es nur zum Neuerzeugen (siehe dort).

## Regeln

- Der Server darf nie Klartext sehen. Neue Felder und Typen gehören in den verschlüsselten Eintrag, nicht in Spalten oder Endpunkte des Servers.
- Krypto-Änderungen brauchen Tests und eine Anpassung von `docs/ARCHITEKTUR.md`. Das Format der Einträge und des Keyrings ist versioniert.
- Geheimnisse (Master-Passwort, Recovery-Key, Datenschlüssel, Mail-Zugangsdaten, Token) werden nie protokolliert und nicht im Klartext gespeichert (Ausnahme: Mail-Zugangsdaten im Schlüsselbund des Betriebssystems).
- Texte, Kommentare und Commit-Nachrichten auf Deutsch.

Repo: https://github.com/dennismarcbusch/bdb-butler

Projektnotiz im Brain: `/vault/10-Projekte/BdB-Butler.md`
