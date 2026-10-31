//! Startet die HTTP-Brücke für Entwicklung und Browser-Tests (nicht für den Produktivbetrieb).
//!
//! Aufruf: `dev_bridge <Datenordner> [Port]`. Mit `BDB_FAST_KDF=1` laufen Anlegen und Entsperren mit
//! schwachen Argon2-Parametern, damit Tests schnell sind. Nur für Tests verwenden.

use std::sync::Arc;

use bdb_core::api::App;
use bdb_core::bridge;
use bdb_core::crypto::KdfParams;
use bdb_core::service::Service;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("Aufruf: dev_bridge <Datenordner> [Port]");
        std::process::exit(2);
    };
    let port = args.next().unwrap_or_else(|| "8787".into());
    let kdf = if std::env::var("BDB_FAST_KDF").as_deref() == Ok("1") {
        eprintln!("WARNUNG: schwache Schlüsselableitung, nur für Tests");
        KdfParams { m_kib: 8, t: 1, p: 1 }
    } else {
        KdfParams::standard()
    };
    let service = match Service::open(std::path::Path::new(&dir), kdf) {
        Ok(s) => Arc::new(App::new(s)),
        Err(e) => {
            eprintln!("Datenordner öffnen: {e}");
            std::process::exit(1);
        }
    };
    match service.apply_mcp_config() {
        Ok(Some(url)) => println!("KI-Zugriff läuft auf {url}"),
        Ok(None) => {}
        Err(e) => eprintln!("KI-Zugriff: {e}"),
    }
    match bridge::spawn(&format!("127.0.0.1:{port}"), service) {
        Ok((addr, handle)) => {
            println!("Brücke läuft auf http://{addr}");
            let _ = handle.join();
        }
        Err(e) => {
            eprintln!("Brücke starten: {e}");
            std::process::exit(1);
        }
    }
}
