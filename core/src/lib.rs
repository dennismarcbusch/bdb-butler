//! Kern des BdB-Butlers.
//!
//! Aktuell enthält er den Krypto-Kern (`crypto`). Lokale Datenbank, Sync-Client und
//! MCP-Server kommen später dazu. Die Spezifikation steht in `docs/ARCHITEKTUR.md`.

pub mod crypto;

pub use crypto::{Error, KdfParams, Keyring, RecoveryKey, Vault};
