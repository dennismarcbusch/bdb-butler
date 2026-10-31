//! Kern des BdB-Butlers.
//!
//! Aktuell enthält er den Krypto-Kern (`crypto`) das Eintragsmodell (`record`) und die lokale Datenbank (`store`). Lokale Datenbank, Sync-Client und
//! MCP-Server kommen später dazu. Die Spezifikation steht in `docs/ARCHITEKTUR.md`.

pub mod crypto;
pub mod record;
pub mod store;

pub use crypto::{Error, KdfParams, Keyring, RecoveryKey, Vault};
pub use record::{Clock, Conflict, Record, RecordError, Stamp};
pub use store::{ApplyOutcome, Pending, RemoteEntry, Store, StoreError};
