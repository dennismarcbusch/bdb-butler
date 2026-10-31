# BdB-Butler

App für die mBdB-Tätigkeit (Dienstreisen, Schulen, Ansprechpartner, Beratungen, Protokolle, Auftragsbuch, Fortbildungen, Tätigkeitsbericht). Genau 1 Nutzer.

Local-first: Jeder Rechner hält alle Daten in einer lokalen Datenbank. Alle Daten werden Ende-zu-Ende verschlüsselt; der Sync-Server speichert und sieht nur Chiffretext. Entsperrt wird mit Master-Passwort und Recovery-Key. PDFs und Anhänge liegen als Dateien in einem Ordner, den Nextcloud synchronisiert. Ein lokaler MCP-Server in der App ermöglicht den KI-Zugriff.

Anforderungen: `docs/ANFORDERUNGEN.md`. Architektur und Entscheidungen: `docs/ARCHITEKTUR.md`. Beide Dateien sind maßgeblich und werden bei jeder Entscheidung mitgepflegt.

## Aufbau

- `core/`: Rust-Bibliothek `bdb-core` (Krypto-Kern; später lokale Datenbank, Sync-Client, MCP-Server). Tests: `cargo test` im Ordner `core/`.
- `server/`: Go-Sync-Server für Chiffretext. Tests: `go test -race ./...` im Ordner `server/`.
- `app/`: Tauri-App mit TypeScript-Oberfläche. Noch nicht angelegt.
- `dienstreisebuster/`: Bestandstool (Django), wird durch das Dienstreisemodul der App abgelöst. Der Name entfällt; der Ordner wird mit dem Dienstreisemodul ersetzt.

## Regeln

- Der Server darf nie Klartext sehen. Neue Felder und Typen gehören in den verschlüsselten Eintrag, nicht in Spalten oder Endpunkte des Servers.
- Krypto-Änderungen brauchen Tests und eine Anpassung von `docs/ARCHITEKTUR.md`. Das Format der Einträge und des Keyrings ist versioniert.
- Geheimnisse (Master-Passwort, Recovery-Key, Datenschlüssel, Mail-Zugangsdaten, Token) werden nie protokolliert und nicht im Klartext gespeichert (Ausnahme: Mail-Zugangsdaten im Schlüsselbund des Betriebssystems).
- Texte, Kommentare und Commit-Nachrichten auf Deutsch.

Repo: https://github.com/dennismarcbusch/bdb-butler

Projektnotiz im Brain: `/vault/10-Projekte/BdB-Butler.md`
