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
| `core/` | Rust | Krypto-Kern, später lokale Datenbank, Sync-Client, MCP-Server. Wird von der App eingebunden. | Krypto-Kern angelegt |
| `server/` | Go | Sync-Server für Chiffretext. | Gerüst angelegt |
| `app/` | TypeScript, Rust (Tauri) | Oberfläche und Tauri-Hülle. | noch nicht angelegt |
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
- Der Klartext ist ein JSON-Dokument (geplant): Typ (zum Beispiel `schule`, `person`, `reise`, `protokoll`), Schemaversion, Felder mit Zeitstempel je Feld (hybride logische Uhr) und ein Löschkennzeichen. Löschen ist ein neuer Eintragsstand mit Löschkennzeichen (Tombstone), den der Server nicht von anderen unterscheiden kann.
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

**Konflikte:** Bei 409 holt der Client den aktuellen Stand, führt ihn mit seiner Änderung zusammen (Zusammenführung je Feld nach dem Zeitstempel im Klartext, „der Jüngere gewinnt“) und schreibt erneut. Markdown-Texte (Protokolle) werden bei echten Konflikten nicht automatisch gemischt. Dort legt der Client eine Konfliktkopie an.

## Lokaler MCP-Server

- Läuft im Prozess der App und hat damit Zugriff auf den entsperrten Datenschlüssel. Er funktioniert nur, solange die App läuft und entsperrt ist.
- Er lauscht ausschließlich auf localhost und verlangt ein Token. Claude Desktop oder Claude Code verbinden sich per HTTP, gegebenenfalls über einen kleinen stdio-Vermittler.
- Geplante Eigenschaften: Zugriffsstufen (nur lesen, lesen und schreiben), ein Protokoll der Schreibzugriffe, einstellbare Sichtbarkeit einzelner Felder für die KI, kein Löschen (nur Archivieren) und kein Mailversand.
- Die Logik für Datenzugriff und Validierung liegt im Kern und wird von Oberfläche und MCP-Server gemeinsam genutzt.
- Offen: Welche Tools genau, welche Felder sichtbar sind und ob die Datenschutzfreigabe die Übermittlung von Klartext an den KI-Anbieter einschließt.

## PDFs, Anhänge und Mail

- PDFs (Antrag, Abrechnung) werden lokal erzeugt und in dem Ordner abgelegt, den die App je Rechner angegeben bekommt. Anhänge (Hotelrechnungen, Teilnahmebescheinigungen, Veranstalter-Formulare) liegen ebenfalls dort.
- Die Synchronisation dieses Ordners übernimmt Nextcloud oder ein anderes Tool. Die App synchronisiert diese Dateien nicht.
- Die Dateien liegen im Klartext auf dem verschlüsselten Laptop und im Nextcloud-Ordner. Ob der Nextcloud-Server vertrauenswürdig ist oder der Ordner zusätzlich verschlüsselt werden soll (zum Beispiel mit Cryptomator), ist offen.
- Mail wird im Client per SMTP versendet. Die Zugangsdaten liegen im Schlüsselbund des Betriebssystems und werden nicht synchronisiert.

## Entscheidungen im Überblick

| Frage | Entscheidung |
|---|---|
| Architektur | Local-first mit verschlüsseltem Sync |
| Client | Tauri, Oberfläche in TypeScript, Kern in Rust |
| Server | Go (Rust als Alternative offen) |
| Sync | Eigenes schlankes Protokoll mit verschlüsselten Einzeleinträgen statt Evolu, Automerge oder CouchDB |
| Entsperren | Master-Passwort und Recovery-Key |
| KI | Lokaler MCP-Server im App-Prozess, von Anfang an |
| PDFs und Anhänge | Ordner, per Nextcloud synchronisiert |

Begründung für das eigene Sync-Protokoll: Es gibt genau einen Nutzer und wenig Daten. Die Schlüsselverwaltung (Master-Passwort und Recovery-Key) ist vorgegeben. Fertige Bausteine bringen eigene Schlüssel- und Servermodelle mit, die dazu nicht passen, und hängen von jungen Bibliotheken ab. Ein Protokoll mit einer Tabelle und drei Endpunkten ist überschaubar, gut zu testen und bleibt unter eigener Kontrolle.
