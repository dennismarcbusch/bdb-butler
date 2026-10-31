//! HTTP-Brücke zu [`crate::api::call`] für Entwicklung und Browser-Tests.
//!
//! `POST /api/<befehl>` mit JSON-Body liefert `{"ok":true,"data":…}` oder `{"ok":false,"error":{code,message}}`.
//! Die Brücke lauscht nur auf localhost, prüft `Host` und `Origin` gegen Angriffe von Webseiten (CSRF,
//! DNS-Rebinding) und ist **nicht** für den Produktivbetrieb gedacht. Die App spricht den Kern über
//! Tauri-Befehle an.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::api;
use crate::service::Service;

const MAX_BODY: u64 = 2 * 1024 * 1024;

/// Startet die Brücke in einem eigenen Thread. Gibt die tatsächliche Adresse und den Thread zurück.
pub fn spawn(addr: &str, service: Arc<Mutex<Service>>) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    let server = Server::http(addr).map_err(|e| std::io::Error::other(e.to_string()))?;
    let local = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| std::io::Error::other("keine IP-Adresse"))?;
    if !local.ip().is_loopback() {
        return Err(std::io::Error::other("Die Brücke darf nur auf localhost lauschen"));
    }
    let handle = std::thread::spawn(move || {
        for request in server.incoming_requests() {
            handle(request, &service);
        }
    });
    Ok((local, handle))
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.to_string().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn is_local_host(value: &str) -> bool {
    let host = value.rsplit_once(':').map_or(value, |(h, _)| h);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

fn is_local_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(is_local_host)
}

fn respond(request: Request, status: u16, body: Value) {
    let json = Header::from_bytes("Content-Type", "application/json").expect("gültiger Header");
    let no_store = Header::from_bytes("Cache-Control", "no-store").expect("gültiger Header");
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(json)
        .with_header(no_store);
    let _ = request.respond(response);
}

fn handle(mut request: Request, service: &Mutex<Service>) {
    if header(&request, "Host").is_none_or(|h| !is_local_host(&h))
        || header(&request, "Origin").is_some_and(|o| !is_local_origin(&o))
    {
        return respond(request, 403, json!({"ok": false, "error": {"code": "forbidden", "message": "Nicht erlaubt"}}));
    }
    let url = request.url().split('?').next().unwrap_or("").to_string();
    let Some(command) = url.strip_prefix("/api/") else {
        return respond(request, 404, json!({"ok": false, "error": {"code": "not_found", "message": "Nicht gefunden"}}));
    };
    if *request.method() != Method::Post {
        return respond(request, 405, json!({"ok": false, "error": {"code": "bad_request", "message": "Nur POST"}}));
    }

    let mut body = String::new();
    if request.as_reader().take(MAX_BODY + 1).read_to_string(&mut body).is_err() || body.len() as u64 > MAX_BODY {
        return respond(request, 400, json!({"ok": false, "error": {"code": "bad_request", "message": "Ungültiger Body"}}));
    }
    let args: Value = if body.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => {
                return respond(request, 400, json!({"ok": false, "error": {"code": "bad_request", "message": "Kein gültiges JSON"}}))
            }
        }
    };

    match api::call(service, command, &args) {
        Ok(data) => respond(request, 200, json!({"ok": true, "data": data})),
        Err(error) => respond(request, 400, json!({"ok": false, "error": error})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };

    fn start() -> String {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-bridge-test-{name}"));
        let service = Arc::new(Mutex::new(Service::open(&dir, FAST).unwrap()));
        let (addr, _) = spawn("127.0.0.1:0", service).unwrap();
        format!("http://{addr}")
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder().http_status_as_error(false).build().into()
    }

    fn post(base: &str, cmd: &str, body: &str) -> (u16, Value) {
        let mut resp = agent()
            .post(&format!("{base}/api/{cmd}"))
            .header("Content-Type", "application/json")
            .send(body)
            .unwrap();
        (resp.status().as_u16(), resp.body_mut().read_json().unwrap())
    }

    #[test]
    fn commands_work_over_http() {
        let base = start();
        let (status, body) = post(&base, "status", "");
        assert_eq!((status, body["ok"].as_bool()), (200, Some(true)));
        assert_eq!(body["data"]["has_vault"], false);

        let (_, body) = post(&base, "create_vault", r#"{"password":"korrekt pferd batterie heftklammer"}"#);
        assert!(body["data"]["recovery_key"].is_string());
        let (_, body) = post(&base, "save", r#"{"kind":"schule","fields":{"name":"Grundschule"}}"#);
        assert!(body["data"]["id"].is_string());
        let (_, body) = post(&base, "list", r#"{"kind":"schule"}"#);
        assert_eq!(body["data"][0]["fields"]["name"], "Grundschule");
    }

    #[test]
    fn errors_are_reported_as_json() {
        let base = start();
        let (status, body) = post(&base, "list", r#"{"kind":"schule"}"#);
        assert_eq!((status, body["ok"].as_bool(), body["error"]["code"].as_str()), (400, Some(false), Some("locked")));
        let (status, body) = post(&base, "unlock", "kein json");
        assert_eq!((status, body["error"]["code"].as_str()), (400, Some("bad_request")));
        let (status, body) = post(&base, "gibt-es-nicht", "");
        assert_eq!((status, body["error"]["code"].as_str()), (400, Some("unknown_command")));
    }

    #[test]
    fn only_post_to_api_from_local_origins() {
        let base = start();
        let get = agent().get(&format!("{base}/api/status")).call().unwrap();
        assert_eq!(get.status().as_u16(), 405);
        let other = agent().post(&format!("{base}/anderswo")).send("").unwrap();
        assert_eq!(other.status().as_u16(), 404);

        // Anfrage einer fremden Webseite im Browser.
        let evil = agent()
            .post(&format!("{base}/api/status"))
            .header("Origin", "https://boese.example")
            .send("")
            .unwrap();
        assert_eq!(evil.status().as_u16(), 403);
        // Falscher Host (DNS-Rebinding).
        let rebinding = agent()
            .post(&format!("{base}/api/status"))
            .header("Host", "boese.example")
            .send("")
            .unwrap();
        assert_eq!(rebinding.status().as_u16(), 403);
        // Lokaler Ursprung ist erlaubt.
        let local = agent()
            .post(&format!("{base}/api/status"))
            .header("Origin", "http://localhost:5173")
            .send("")
            .unwrap();
        assert_eq!(local.status().as_u16(), 200);
    }

    #[test]
    fn host_and_origin_checks() {
        assert!(is_local_host("localhost:8080") && is_local_host("127.0.0.1") && is_local_host("[::1]:1"));
        assert!(!is_local_host("localhost.evil.example") && !is_local_host("evil.example:80"));
        assert!(is_local_origin("http://localhost:5173") && is_local_origin("http://127.0.0.1:3000"));
        assert!(!is_local_origin("https://evil.example") && !is_local_origin("null") && !is_local_origin("file://x"));
    }

    #[test]
    fn refuses_non_local_addresses() {
        let service = Arc::new(Mutex::new(Service::open(&std::env::temp_dir().join("bdb-bridge-x"), FAST).unwrap()));
        assert!(spawn("0.0.0.0:0", service).is_err());
    }
}
