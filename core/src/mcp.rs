//! Lokaler MCP-Server (Model Context Protocol) im Prozess der App.
//!
//! Er läuft nur, wenn der Nutzer ihn einschaltet, lauscht ausschließlich auf localhost, verlangt ein
//! Bearer-Token und prüft `Host` und `Origin` (Schutz vor DNS-Rebinding). Die Einstellungen (Token,
//! Schreibstufe, Freigaben) liest er bei jeder Anfrage neu, sie gelten also sofort ohne Neustart. Nur
//! Ein/Aus und der Port erfordern einen Neustart des Servers. Er spricht Streamable HTTP
//! (`POST /mcp`, JSON-RPC, Antworten als `application/json`) und hat keine Sitzungen.
//!
//! Zugriffsregeln:
//! - Typen mit Personenbezug (`personal` in der Typdefinition) sind gesperrt, bis der Nutzer sie einzeln
//!   freischaltet.
//! - Schreiben (anlegen, ändern) ist nur mit eingeschalteter Schreibstufe möglich.
//! - Es gibt kein Löschen und keinen Mailversand.
//! - Ist die App gesperrt, antworten alle Werkzeuge mit einem Hinweis. Der Datenschlüssel verlässt den
//!   Prozess nie.
//! - Jeder Aufruf wird protokolliert (Werkzeug, Typ, ID, Feldnamen), nie Inhalte.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::record;
use crate::schema::{self, KindDef};
use crate::service::{RecordView, Service, ServiceError};

/// Protokollversionen, die der Server beherrscht, die neueste zuerst.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_BODY: u64 = 1024 * 1024;
const MAX_RESULTS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Der Server läuft nur, wenn der Nutzer ihn eingeschaltet hat.
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    /// Schreibstufe: anlegen und ändern erlaubt.
    pub write: bool,
    /// Typen mit Personenbezug, die der Nutzer für die KI freigegeben hat.
    pub allow_personal: Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: false, port: 8765, token: String::new(), write: false, allow_personal: Vec::new() }
    }
}

impl McpConfig {
    /// Zufälliges Token (256 Bit, hexadezimal).
    pub fn new_token() -> Result<String, record::RecordError> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|_| record::RecordError::Random)?;
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Darf die KI Einträge dieses Typs sehen und (mit Schreibstufe) ändern?
    pub fn kind_allowed(&self, def: &KindDef) -> bool {
        !def.personal || self.allow_personal.iter().any(|k| k == def.kind)
    }
}

// ----- JSON-RPC -----------------------------------------------------------------------------------------

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

const INSTRUCTIONS: &str = "Zugriff auf den BdB-Butler, die Anwendung für die Tätigkeit als mBdB (Schulen, Beratungen, \
Protokolle, Aufträge, Dienstreisen, Fortbildungen). Beginne mit list_kinds, um Typen und Felder zu sehen. Typen mit \
Personenbezug sind für die KI gesperrt, solange der Nutzer sie nicht freigibt. Verarbeite über diesen Zugang keine \
personenbezogenen Daten. Es gibt kein Löschen und keinen Mailversand.";

/// Verarbeitet eine JSON-RPC-Nachricht. Gibt `None` für Benachrichtigungen und Antworten des Clients zurück.
pub fn handle(service: &Mutex<Service>, config: &McpConfig, message: &Value) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "Ungültige Anfrage"));
    };
    let id = object.get("id").cloned();
    let method = object.get("method").and_then(Value::as_str);
    let Some(method) = method else {
        // Antwort des Clients auf eine Anfrage des Servers (wir stellen keine) oder Unsinn.
        return if id.is_some() && (object.contains_key("result") || object.contains_key("error")) {
            None
        } else {
            Some(rpc_error(id.unwrap_or(Value::Null), -32600, "Ungültige Anfrage"))
        };
    };
    // Ohne `id` ist es eine Benachrichtigung: keine Antwort.
    let id = id?;
    let params = object.get("params").cloned().unwrap_or(Value::Null);

    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = PROTOCOL_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "bdb-butler", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({"tools": tool_definitions(config)})),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "Werkzeugname fehlt"));
            };
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let (text, is_error) = match call_tool(service, config, name, &args) {
                Ok(value) => (value.to_string(), false),
                Err(message) => (message, true),
            };
            rpc_result(id, json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        _ => rpc_error(id, -32601, "Methode nicht gefunden"),
    })
}

// ----- Werkzeuge ----------------------------------------------------------------------------------------

fn tool_definitions(config: &McpConfig) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "list_kinds",
            "description": "Listet die Typen von Einträgen mit ihren Feldern. Typen mit Personenbezug sind gesperrt, \
                            solange der Nutzer sie nicht freigegeben hat (accessible = false).",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "list_records",
            "description": "Listet die Einträge eines Typs, nach Titel sortiert. Optional gefiltert nach Suchtext.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "description": "Typ, zum Beispiel schule"},
                    "query": {"type": "string", "description": "Suchtext in allen Textfeldern"},
                },
                "required": ["kind"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "get_record",
            "description": "Liefert einen Eintrag anhand seiner ID.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "search",
            "description": "Sucht in allen für die KI zugänglichen Typen nach einem Text.",
            "inputSchema": {
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true},
        }),
    ];
    if config.write {
        tools.push(json!({
            "name": "create_record",
            "description": "Legt einen Eintrag an. Felder siehe list_kinds. Leere Texte lassen ein Feld leer. Verweise \
                            sind IDs bestehender Einträge.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string"},
                    "fields": {"type": "object", "description": "Feldname und Wert"},
                },
                "required": ["kind", "fields"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
        }));
        tools.push(json!({
            "name": "update_record",
            "description": "Ändert Felder eines Eintrags. Nur die übergebenen Felder werden geändert. Ein leerer Text \
                            oder null leert das Feld.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "fields": {"type": "object", "description": "Feldname und neuer Wert"},
                },
                "required": ["id", "fields"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
        }));
    }
    tools
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    kind: String,
    #[serde(default)]
    query: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    kind: String,
    fields: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    id: String,
    fields: Map<String, Value>,
}

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("Ungültige Argumente: {e}"))
}

fn blocked_message(kind: &str) -> String {
    format!(
        "Der Typ „{kind}“ enthält personenbezogene Daten und ist für die KI gesperrt. Der Nutzer kann ihn in den \
         Einstellungen der App freigeben."
    )
}

fn service_error(e: ServiceError) -> String {
    match e {
        ServiceError::Locked => "Die App ist gesperrt. Bitte in der App entsperren.".to_string(),
        other => other.to_string(),
    }
}

fn kind_def(config: &McpConfig, kind: &str) -> Result<KindDef, String> {
    let def = schema::kind(kind).ok_or_else(|| format!("Unbekannter Typ „{kind}“"))?;
    if config.kind_allowed(&def) {
        Ok(def)
    } else {
        Err(blocked_message(kind))
    }
}

fn record_json(r: &RecordView) -> Value {
    json!({"id": r.id, "kind": r.kind, "fields": r.fields})
}

fn text_matches(r: &RecordView, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || r.fields.values().any(|v| v.as_str().is_some_and(|s| s.to_lowercase().contains(&q)))
}

fn need_write(config: &McpConfig) -> Result<(), String> {
    if config.write {
        Ok(())
    } else {
        Err("Der Schreibzugriff für die KI ist ausgeschaltet. Der Nutzer kann ihn in den Einstellungen der App einschalten.".into())
    }
}

/// Führt ein Werkzeug aus und protokolliert den Aufruf.
pub fn call_tool(service: &Mutex<Service>, config: &McpConfig, name: &str, args: &Value) -> Result<Value, String> {
    let mut service = service.lock().map_err(|_| "Interner Fehler".to_string())?;
    let mut audit = json!({"t": record::now_ms(), "tool": name});

    let result = run_tool(&mut service, config, name, args, &mut audit);
    audit["ok"] = json!(result.is_ok());
    if let Err(message) = &result {
        audit["error"] = json!(message.chars().take(120).collect::<String>());
    }
    service.audit(&audit);
    result
}

fn run_tool(service: &mut Service, config: &McpConfig, name: &str, args: &Value, audit: &mut Value) -> Result<Value, String> {
    match name {
        "list_kinds" => {
            let kinds: Vec<Value> = schema::kinds()
                .into_iter()
                .map(|def| {
                    let accessible = config.kind_allowed(&def);
                    json!({
                        "kind": def.kind,
                        "label": def.label,
                        "label_plural": def.label_plural,
                        "title_field": def.title_field,
                        "personal": def.personal,
                        "accessible": accessible,
                        "fields": def.fields,
                    })
                })
                .collect();
            Ok(json!({"kinds": kinds, "write": config.write}))
        }
        "list_records" => {
            let a: ListArgs = parse(args)?;
            audit["kind"] = json!(a.kind);
            kind_def(config, &a.kind)?;
            let mut records = service.list(&a.kind).map_err(service_error)?;
            if let Some(q) = &a.query {
                records.retain(|r| text_matches(r, q));
            }
            let total = records.len();
            records.truncate(MAX_RESULTS);
            Ok(json!({"records": records.iter().map(record_json).collect::<Vec<_>>(), "total": total}))
        }
        "get_record" => {
            let a: IdArgs = parse(args)?;
            audit["id"] = json!(a.id);
            let record = service.get(&a.id).map_err(service_error)?;
            audit["kind"] = json!(record.kind);
            kind_def(config, &record.kind)?;
            Ok(record_json(&record))
        }
        "search" => {
            let a: SearchArgs = parse(args)?;
            let mut hits: Vec<RecordView> = Vec::new();
            for def in schema::kinds().into_iter().filter(|d| config.kind_allowed(d)) {
                hits.extend(service.list(def.kind).map_err(service_error)?.into_iter().filter(|r| text_matches(r, &a.query)));
            }
            let total = hits.len();
            hits.truncate(MAX_RESULTS);
            Ok(json!({"records": hits.iter().map(record_json).collect::<Vec<_>>(), "total": total}))
        }
        "create_record" => {
            need_write(config)?;
            let a: CreateArgs = parse(args)?;
            audit["kind"] = json!(a.kind);
            audit["fields"] = json!(a.fields.keys().collect::<Vec<_>>());
            kind_def(config, &a.kind)?;
            let id = service.save(&a.kind, None, &a.fields).map_err(service_error)?;
            audit["id"] = json!(id);
            Ok(json!({"id": id}))
        }
        "update_record" => {
            need_write(config)?;
            let a: UpdateArgs = parse(args)?;
            audit["id"] = json!(a.id);
            audit["fields"] = json!(a.fields.keys().collect::<Vec<_>>());
            let existing = service.get(&a.id).map_err(service_error)?;
            audit["kind"] = json!(existing.kind);
            kind_def(config, &existing.kind)?;
            service.save(&existing.kind, Some(&a.id), &a.fields).map_err(service_error)?;
            Ok(json!({"id": a.id}))
        }
        other => Err(format!("Unbekanntes Werkzeug „{other}“")),
    }
}

// ----- HTTP ---------------------------------------------------------------------------------------------

/// Der laufende Server. Beim Verwerfen wird er angehalten.
pub struct McpServer {
    addr: SocketAddr,
    server: Arc<Server>,
}

impl McpServer {
    /// Startet den Server auf `127.0.0.1:port` (0 wählt einen freien Port). Token, Schreibstufe und Freigaben
    /// liest er bei jeder Anfrage aus dem Dienst. Hinweis: Hält man einen Server an, hängen Anfragen auf
    /// bereits offenen Verbindungen, bis der Client sie abbricht (die HTTP-Bibliothek schließt sie nicht).
    /// Deshalb startet die App ihn nur bei Änderung von Ein/Aus oder Port neu.
    pub fn spawn(service: Arc<Mutex<Service>>, port: u16) -> std::io::Result<Self> {
        let server = Arc::new(
            Server::http(format!("127.0.0.1:{port}")).map_err(|e| std::io::Error::other(e.to_string()))?,
        );
        let addr = server.server_addr().to_ip().ok_or_else(|| std::io::Error::other("keine IP-Adresse"))?;
        let worker = Arc::clone(&server);
        std::thread::spawn(move || {
            for request in worker.incoming_requests() {
                handle_http(request, &service);
            }
        });
        Ok(Self { addr, server })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.server.unblock();
    }
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

/// Vergleich in konstanter Zeit, damit die Antwortzeit nichts über das Token verrät.
fn token_matches(given: &str, expected: &str) -> bool {
    if expected.is_empty() || given.len() != expected.len() {
        return false;
    }
    given.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn respond_json(request: Request, status: u16, body: Option<Value>, extra: &[(&str, &str)]) {
    let mut response = match body {
        Some(value) => Response::from_string(value.to_string()),
        None => Response::from_string(""),
    }
    .with_status_code(status);
    response.add_header(Header::from_bytes("Content-Type", "application/json").expect("gültiger Header"));
    response.add_header(Header::from_bytes("Cache-Control", "no-store").expect("gültiger Header"));
    for (name, value) in extra {
        if let Ok(h) = Header::from_bytes(*name, *value) {
            response.add_header(h);
        }
    }
    let _ = request.respond(response);
}

fn handle_http(mut request: Request, service: &Mutex<Service>) {
    let config = match service.lock() {
        Ok(guard) => guard.mcp_config().clone(),
        Err(_) => return respond_json(request, 500, Some(json!({"error": "Interner Fehler"})), &[]),
    };
    if header(&request, "Host").is_none_or(|h| !is_local_host(&h))
        || header(&request, "Origin").is_some_and(|o| !is_local_origin(&o))
    {
        return respond_json(request, 403, Some(json!({"error": "Nicht erlaubt"})), &[]);
    }
    if !config.enabled {
        return respond_json(request, 503, Some(json!({"error": "Der KI-Zugriff ist ausgeschaltet"})), &[]);
    }
    let bearer = header(&request, "Authorization")
        .and_then(|h| h.strip_prefix("Bearer ").map(str::to_string))
        .unwrap_or_default();
    if !token_matches(&bearer, &config.token) {
        return respond_json(request, 401, Some(json!({"error": "Nicht angemeldet"})), &[("WWW-Authenticate", "Bearer")]);
    }
    let path = request.url().split('?').next().unwrap_or("").to_string();
    if path != "/mcp" {
        return respond_json(request, 404, Some(json!({"error": "Nicht gefunden"})), &[]);
    }
    if *request.method() != Method::Post {
        return respond_json(request, 405, Some(json!({"error": "Nur POST"})), &[("Allow", "POST")]);
    }

    let mut body = String::new();
    if request.as_reader().take(MAX_BODY + 1).read_to_string(&mut body).is_err() || body.len() as u64 > MAX_BODY {
        return respond_json(request, 400, Some(rpc_error(Value::Null, -32700, "Ungültiger Body")), &[]);
    }
    let message: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return respond_json(request, 400, Some(rpc_error(Value::Null, -32700, "Kein gültiges JSON")), &[]),
    };
    match handle(service, &config, &message) {
        Some(response) => respond_json(request, 200, Some(response), &[]),
        None => respond_json(request, 202, None, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
    const PASSWORD: &str = "korrekt pferd batterie heftklammer";

    fn unlocked() -> Arc<Mutex<Service>> {
        let mut bytes = [0u8; 6];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("bdb-mcp-test-{name}"));
        let mut service = Service::open(&dir, FAST).unwrap();
        service.create_vault(PASSWORD).unwrap();
        Arc::new(Mutex::new(service))
    }

    fn config(write: bool, allow_personal: &[&str]) -> McpConfig {
        McpConfig {
            enabled: true,
            port: 0,
            token: "test-token-test-token-test-token-0123456789".into(),
            write,
            allow_personal: allow_personal.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn rpc(service: &Mutex<Service>, config: &McpConfig, method: &str, params: Value) -> Value {
        handle(service, config, &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).expect("Antwort")
    }

    fn tool(service: &Mutex<Service>, config: &McpConfig, name: &str, args: Value) -> (Value, bool) {
        let response = rpc(service, config, "tools/call", json!({"name": name, "arguments": args}));
        let result = &response["result"];
        let text = result["content"][0]["text"].as_str().unwrap().to_string();
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (value, result["isError"].as_bool().unwrap())
    }

    fn add(service: &Mutex<Service>, kind: &str, fields: Value) -> String {
        service.lock().unwrap().save(kind, None, fields.as_object().unwrap()).unwrap()
    }

    #[test]
    fn initialize_negotiates_the_protocol_version() {
        let s = unlocked();
        let c = config(false, &[]);
        let r = rpc(&s, &c, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}));
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "bdb-butler");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert!(r["result"]["instructions"].as_str().unwrap().contains("personenbezogen"));

        let r = rpc(&s, &c, "initialize", json!({"protocolVersion": "1999-01-01"}));
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        assert_eq!(rpc(&s, &c, "ping", Value::Null)["result"], json!({}));
    }

    #[test]
    fn notifications_and_client_responses_get_no_answer() {
        let s = unlocked();
        let c = config(false, &[]);
        assert!(handle(&s, &c, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        assert!(handle(&s, &c, &json!({"jsonrpc": "2.0", "id": 5, "result": {}})).is_none());
    }

    #[test]
    fn invalid_messages_and_unknown_methods_are_errors() {
        let s = unlocked();
        let c = config(false, &[]);
        assert_eq!(handle(&s, &c, &json!([1, 2])).unwrap()["error"]["code"], -32600);
        assert_eq!(handle(&s, &c, &json!({"jsonrpc": "2.0", "id": 1})).unwrap()["error"]["code"], -32600);
        assert_eq!(rpc(&s, &c, "gibt/es/nicht", Value::Null)["error"]["code"], -32601);
        assert_eq!(rpc(&s, &c, "tools/call", json!({}))["error"]["code"], -32602);
        // Die ID der Anfrage kommt in der Antwort zurück.
        let r = handle(&s, &c, &json!({"jsonrpc": "2.0", "id": "abc", "method": "ping"})).unwrap();
        assert_eq!(r["id"], "abc");
    }

    #[test]
    fn write_tools_only_appear_with_write_access() {
        let s = unlocked();
        let names = |c: &McpConfig| -> Vec<String> {
            rpc(&s, c, "tools/list", Value::Null)["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(names(&config(false, &[])), ["list_kinds", "list_records", "get_record", "search"]);
        let with_write = names(&config(true, &[]));
        assert!(with_write.contains(&"create_record".to_string()) && with_write.contains(&"update_record".to_string()));
        for forbidden in ["delete_record", "send_mail", "delete"] {
            assert!(!with_write.iter().any(|n| n == forbidden), "{forbidden} darf es nicht geben");
        }
        // Jedes Werkzeug hat ein gültiges Eingabeschema.
        for t in rpc(&s, &config(true, &[]), "tools/list", Value::Null)["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(t["description"].as_str().unwrap().len() > 10);
        }
    }

    #[test]
    fn read_tools_return_records() {
        let s = unlocked();
        let c = config(false, &[]);
        let grund = add(&s, "schule", json!({"name": "Grundschule am See", "ort": "Ulm"}));
        add(&s, "schule", json!({"name": "Gymnasium Nord", "ort": "Bonn"}));

        let (v, err) = tool(&s, &c, "list_records", json!({"kind": "schule"}));
        assert!(!err);
        assert_eq!(v["total"], 2);
        assert_eq!(v["records"][0]["fields"]["name"], "Grundschule am See");

        let (v, _) = tool(&s, &c, "list_records", json!({"kind": "schule", "query": "bonn"}));
        assert_eq!(v["total"], 1);
        assert_eq!(v["records"][0]["fields"]["name"], "Gymnasium Nord");

        let (v, err) = tool(&s, &c, "get_record", json!({"id": grund}));
        assert!(!err);
        assert_eq!(v["fields"]["ort"], "Ulm");

        let (v, _) = tool(&s, &c, "search", json!({"query": "ulm"}));
        assert_eq!(v["total"], 1);

        let (v, _) = tool(&s, &c, "list_kinds", json!({}));
        let kinds = v["kinds"].as_array().unwrap();
        let schule = kinds.iter().find(|k| k["kind"] == "schule").unwrap();
        assert_eq!(schule["accessible"], true);
        assert!(schule["fields"].as_array().unwrap().iter().any(|f| f["name"] == "name"));
        assert_eq!(v["write"], false);
    }

    #[test]
    fn personal_kinds_are_blocked_by_default() {
        let s = unlocked();
        let schule = add(&s, "schule", json!({"name": "Schule"}));
        let person = add(&s, "ansprechpartner", json!({"name": "Frau Geheim", "schule": schule, "telefon": "0123 456"}));
        let c = config(true, &[]);

        let (v, err) = tool(&s, &c, "list_records", json!({"kind": "ansprechpartner"}));
        assert!(err);
        assert!(v.as_str().unwrap().contains("gesperrt"));
        assert!(!v.to_string().contains("Geheim"));

        let (v, err) = tool(&s, &c, "get_record", json!({"id": person}));
        assert!(err);
        assert!(!v.to_string().contains("Geheim") && !v.to_string().contains("0123"));

        // Die Suche überspringt gesperrte Typen.
        let (v, _) = tool(&s, &c, "search", json!({"query": "Geheim"}));
        assert_eq!(v["total"], 0);

        // Schreiben in gesperrte Typen ist ebenfalls verboten, auch über update_record.
        let (_, err) = tool(&s, &c, "create_record", json!({"kind": "ansprechpartner", "fields": {"name": "X"}}));
        assert!(err);
        let (_, err) = tool(&s, &c, "update_record", json!({"id": person, "fields": {"name": "Y"}}));
        assert!(err);
        assert_eq!(s.lock().unwrap().get(&person).unwrap().fields["name"], "Frau Geheim");

        // list_kinds meldet den Typ als gesperrt.
        let (v, _) = tool(&s, &c, "list_kinds", json!({}));
        let person_kind = v["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == "ansprechpartner").unwrap().clone();
        assert_eq!((person_kind["personal"].clone(), person_kind["accessible"].clone()), (json!(true), json!(false)));
    }

    #[test]
    fn personal_kinds_can_be_unlocked_individually() {
        let s = unlocked();
        let person = add(&s, "ansprechpartner", json!({"name": "Frau Weber"}));
        let c = config(false, &["ansprechpartner"]);
        let (v, err) = tool(&s, &c, "get_record", json!({"id": person}));
        assert!(!err);
        assert_eq!(v["fields"]["name"], "Frau Weber");
        let (v, _) = tool(&s, &c, "search", json!({"query": "weber"}));
        assert_eq!(v["total"], 1);
    }

    #[test]
    fn writing_needs_write_access_and_validates() {
        let s = unlocked();
        let (v, err) = tool(&s, &config(false, &[]), "create_record", json!({"kind": "schule", "fields": {"name": "A"}}));
        assert!(err);
        assert!(v.as_str().unwrap().contains("Schreibzugriff"));
        assert!(s.lock().unwrap().list("schule").unwrap().is_empty());

        let c = config(true, &[]);
        let (v, err) = tool(&s, &c, "create_record", json!({"kind": "schule", "fields": {"name": "Neu", "ort": "Ulm"}}));
        assert!(!err);
        let id = v["id"].as_str().unwrap().to_string();

        let (_, err) = tool(&s, &c, "update_record", json!({"id": id, "fields": {"ort": "Bonn"}}));
        assert!(!err);
        assert_eq!(s.lock().unwrap().get(&id).unwrap().fields["ort"], "Bonn");
        assert_eq!(s.lock().unwrap().get(&id).unwrap().fields["name"], "Neu");

        // Fehlerhafte Eingaben werden als Werkzeugfehler gemeldet, nichts wird gespeichert.
        let (v, err) = tool(&s, &c, "create_record", json!({"kind": "schule", "fields": {"schulart": "Zirkus", "name": "X"}}));
        assert!(err && v.as_str().unwrap().contains("Auswahl"));
        let (_, err) = tool(&s, &c, "create_record", json!({"kind": "schule", "fields": {}}));
        assert!(err);
        let (_, err) = tool(&s, &c, "create_record", json!({"kind": "gibt-es-nicht", "fields": {"name": "X"}}));
        assert!(err);
        let (_, err) = tool(&s, &c, "update_record", json!({"id": "gibt-es-nicht", "fields": {"name": "X"}}));
        assert!(err);
        assert_eq!(s.lock().unwrap().list("schule").unwrap().len(), 1);
    }

    #[test]
    fn bad_arguments_and_unknown_tools_are_tool_errors() {
        let s = unlocked();
        let c = config(true, &[]);
        assert!(tool(&s, &c, "list_records", json!({})).1);
        assert!(tool(&s, &c, "list_records", json!({"kind": "schule", "extra": 1})).1);
        assert!(tool(&s, &c, "gibt_es_nicht", json!({})).1);
        assert!(tool(&s, &c, "get_record", json!({"id": 5})).1);
    }

    #[test]
    fn locked_app_answers_with_a_hint() {
        let s = unlocked();
        let c = config(true, &[]);
        add(&s, "schule", json!({"name": "A"}));
        s.lock().unwrap().lock();
        let (v, err) = tool(&s, &c, "list_records", json!({"kind": "schule"}));
        assert!(err);
        assert!(v.as_str().unwrap().contains("gesperrt"));
        assert!(tool(&s, &c, "create_record", json!({"kind": "schule", "fields": {"name": "B"}})).1);
        // list_kinds braucht keinen Schlüssel.
        assert!(!tool(&s, &c, "list_kinds", json!({})).1);
    }

    #[test]
    fn audit_log_records_calls_without_contents() {
        let s = unlocked();
        let c = config(true, &[]);
        let (v, _) = tool(&s, &c, "create_record", json!({"kind": "schule", "fields": {"name": "Streng Geheime Schule", "ort": "Ulm"}}));
        let id = v["id"].as_str().unwrap().to_string();
        tool(&s, &c, "get_record", json!({"id": id}));
        tool(&s, &c, "list_records", json!({"kind": "ansprechpartner"}));
        tool(&s, &c, "search", json!({"query": "Streng Geheim"}));

        let log = s.lock().unwrap().audit_tail(10);
        assert_eq!(log.len(), 4);
        assert_eq!(log[0]["tool"], "create_record");
        assert_eq!(log[0]["ok"], true);
        assert_eq!(log[0]["id"], json!(id));
        assert_eq!(log[0]["fields"], json!(["name", "ort"]));
        assert_eq!(log[2]["tool"], "list_records");
        assert_eq!(log[2]["ok"], false, "gesperrter Typ wird als Fehlschlag protokolliert");
        let text = serde_json::to_string(&log).unwrap();
        for secret in ["Streng", "Geheim", "Ulm"] {
            assert!(!text.contains(secret), "Inhalt „{secret}“ im Protokoll");
        }
        assert_eq!(s.lock().unwrap().audit_tail(2).len(), 2);
    }

    #[test]
    fn token_comparison() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abd", "abc") && !token_matches("ab", "abc") && !token_matches("abcd", "abc"));
        assert!(!token_matches("", ""), "ein leeres Token öffnet nichts");
    }

    // ----- HTTP -----------------------------------------------------------------------------------------

    /// Startet einen Server auf einem freien Port. Die Einstellungen kommen aus dem Dienst, das Token
    /// wird zurückgegeben.
    fn start(write: bool) -> (McpServer, Arc<Mutex<Service>>, String) {
        let s = unlocked();
        add(&s, "schule", json!({"name": "Grundschule am See"}));
        s.lock().unwrap().set_mcp_config(true, 0, write, &[]).unwrap();
        let token = s.lock().unwrap().mcp_config().token.clone();
        (McpServer::spawn(Arc::clone(&s), 0).unwrap(), s, token)
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder().http_status_as_error(false).build().into()
    }

    fn post(server: &McpServer, token: Option<&str>, extra: &[(&str, &str)], body: &str) -> (u16, String) {
        let mut req = agent().post(&server.url()).header("Content-Type", "application/json");
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let mut resp = req.send(body).unwrap();
        (resp.status().as_u16(), resp.body_mut().read_to_string().unwrap())
    }

    #[test]
    fn http_requires_the_token() {
        let (server, _s, token) = start(false);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

        assert_eq!(post(&server, None, &[], body).0, 401);
        assert_eq!(post(&server, Some("falsch"), &[], body).0, 401);
        assert_eq!(post(&server, Some(&format!("{token}x")), &[], body).0, 401);
        let (status, text) = post(&server, Some(&token), &[], body);
        assert_eq!(status, 200);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["result"], json!({}));
    }

    #[test]
    fn http_checks_host_and_origin() {
        let (server, _s, token) = start(false);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        assert_eq!(post(&server, Some(&token), &[("Origin", "https://boese.example")], body).0, 403);
        assert_eq!(post(&server, Some(&token), &[("Host", "boese.example")], body).0, 403);
        assert_eq!(post(&server, Some(&token), &[("Origin", "http://localhost:3000")], body).0, 200);
    }

    #[test]
    fn http_status_codes_and_methods() {
        let (server, _s, token) = start(false);

        // Benachrichtigungen: 202 ohne Inhalt.
        let (status, text) = post(&server, Some(&token), &[], r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert_eq!((status, text.as_str()), (202, ""));
        // Kaputtes JSON.
        let (status, text) = post(&server, Some(&token), &[], "kein json");
        assert_eq!(status, 400);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["error"]["code"], -32700);

        let get = agent().get(&server.url()).header("Authorization", format!("Bearer {token}")).call().unwrap();
        assert_eq!(get.status().as_u16(), 405);
        let other = agent().post(&format!("http://{}/anderswo", server.addr())).header("Authorization", format!("Bearer {token}")).send("").unwrap();
        assert_eq!(other.status().as_u16(), 404);
    }

    #[test]
    fn http_tool_call_end_to_end() {
        let (server, _s, token) = start(false);
        let (status, text) = post(
            &server,
            Some(&token),
            &[],
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"list_records","arguments":{"kind":"schule"}}}"#,
        );
        assert_eq!(status, 200);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["id"], 7);
        let inner: Value = serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(inner["records"][0]["fields"]["name"], "Grundschule am See");
    }

    #[test]
    fn settings_apply_live_without_restart() {
        let (server, svc, token) = start(false);
        let list = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let names = |token: &str| -> Option<Vec<String>> {
            let (status, text) = post(&server, Some(token), &[], list);
            (status == 200).then(|| {
                serde_json::from_str::<Value>(&text).unwrap()["result"]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t["name"].as_str().unwrap().to_string())
                    .collect()
            })
        };
        assert!(!names(&token).unwrap().contains(&"create_record".to_string()));

        // Schreibstufe einschalten: gilt sofort, ohne dass der Server neu starten muss.
        svc.lock().unwrap().set_mcp_config(true, 0, true, &[]).unwrap();
        assert!(names(&token).unwrap().contains(&"create_record".to_string()));

        // Neues Token: das alte ist sofort ungültig, das neue sofort gültig.
        svc.lock().unwrap().new_mcp_token().unwrap();
        let new_token = svc.lock().unwrap().mcp_config().token.clone();
        assert_ne!(new_token, token);
        assert!(names(&token).is_none());
        assert!(names(&new_token).is_some());

        // Ausgeschaltet: der Server antwortet nicht mehr mit Daten.
        svc.lock().unwrap().set_mcp_config(false, 0, true, &[]).unwrap();
        assert_eq!(post(&server, Some(&new_token), &[], list).0, 503);
    }

    #[test]
    fn server_stops_when_dropped() {
        let (server, _s, token) = start(false);
        let url = server.url();
        drop(server);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let result = agent().post(&url).header("Authorization", format!("Bearer {token}")).send("{}");
        assert!(result.is_err(), "nach dem Anhalten darf niemand mehr antworten");
    }
}
