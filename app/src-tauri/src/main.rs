// Unter Windows kein zusätzliches Konsolenfenster im Release-Build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use bdb_core::api::{self, ApiError};
use bdb_core::crypto::KdfParams;
use bdb_core::service::Service;
use serde_json::Value;
use tauri::Manager;

type SharedService = Arc<Mutex<Service>>;

/// Einziger Befehl der Oberfläche. Er reicht Befehl und Argumente an den Kern weiter (siehe
/// `core/src/api.rs`). Die Arbeit läuft in einem eigenen Thread, damit ein Abgleich mit dem Server die
/// Oberfläche nicht blockiert.
#[tauri::command]
async fn call(state: tauri::State<'_, SharedService>, command: String, args: Value) -> Result<Value, ApiError> {
    let service = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || api::call(&service, &command, &args))
        .await
        .unwrap_or_else(|_| {
            Err(ApiError { code: "internal", message: "Interner Fehler: Befehl abgebrochen".into() })
        })
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let service = Service::open(&dir, KdfParams::standard())
                .map_err(|e| format!("Datenordner {} konnte nicht geöffnet werden: {e}", dir.display()))?;
            app.manage::<SharedService>(Arc::new(Mutex::new(service)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call])
        .run(tauri::generate_context!())
        .expect("Fehler beim Start der App");
}
