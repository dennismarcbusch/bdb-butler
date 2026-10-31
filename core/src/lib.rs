//! Kern des BdB-Butlers.
//!
//! Aktuell enthält er den Krypto-Kern (`crypto`) das Eintragsmodell (`record`) die lokale Datenbank (`store`) und den Sync-Client (`sync`). Lokale Datenbank, Sync-Client und
//! MCP-Server kommen später dazu. Die Spezifikation steht in `docs/ARCHITEKTUR.md`.

pub mod api;
pub mod bridge;
pub mod crypto;
pub mod dienstreise;
pub mod mail;
pub mod mcp;
pub mod pdf;
pub mod record;
pub mod reisen;
pub mod schema;
pub mod secrets;
pub mod service;
#[cfg(test)]
pub(crate) mod test_smtp;
pub mod store;
pub mod sync;

pub use crypto::{Error, KdfParams, Keyring, RecoveryKey, Vault};
pub use record::{Clock, Conflict, Record, RecordError, Stamp};
pub use store::{ApplyOutcome, Pending, RemoteEntry, Store, StoreError};
pub use api::App;
pub use mcp::{McpConfig, McpServer};
pub use service::{Config, RecordView, Service, ServiceError, Status};
pub use sync::{SyncClient, SyncError, SyncReport};
