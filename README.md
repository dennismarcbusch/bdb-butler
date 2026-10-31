# BdB-Butler

App für die mBdB-Tätigkeit (Dienstreisen, Schulen, Ansprechpartner, Beratungen, Protokolle, Auftragsbuch, Fortbildungen, Tätigkeitsbericht).

Local-first und Ende-zu-Ende verschlüsselt: Die Rechner halten die Daten lokal, ein Go-Server verteilt nur Chiffretext.

- [Anforderungen](docs/ANFORDERUNGEN.md)
- [Architektur](docs/ARCHITEKTUR.md)

| Ordner | Inhalt |
|---|---|
| `core/` | Rust-Kern (Krypto) |
| `server/` | Go-Sync-Server |
| `dienstreisebuster/` | Bestandstool (Django), wird abgelöst |
