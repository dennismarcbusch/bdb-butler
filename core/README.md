# bdb-core

Rust-Kern des BdB-Butlers. Aktuell der Krypto-Kern (`src/crypto.rs`):

- Datenschlüssel, verpackt unter Master-Passwort (Argon2id) und Recovery-Key (HKDF-SHA-256) im `Keyring`
- Einträge verschlüsseln und entschlüsseln (XChaCha20-Poly1305, Eintrags-ID als zusätzliche authentifizierte Daten)
- Passwort ändern, Recovery-Key erneuern

Spezifikation: `../docs/ARCHITEKTUR.md`.

```bash
cargo test
cargo test --release -- --ignored --nocapture   # Dauer mit den Standardparametern (256 MiB Argon2id)
```

Mit den Standardparametern dauern Anlegen und Entsperren auf einem Cloud-Container jeweils knapp eine Sekunde (Release-Build).
