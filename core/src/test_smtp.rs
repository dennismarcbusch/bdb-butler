//! Ein kleiner SMTP-Server für Tests (ohne Verschlüsselung). Er merkt sich die empfangenen Nachrichten und kann
//! eine Anmeldung verlangen.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use crate::mail::{Security, SmtpSettings};

/// Ein kleiner SMTP-Server ohne Verschlüsselung. Er merkt sich die empfangenen Nachrichten.
pub(crate) struct FakeSmtp {
    pub(crate) port: u16,
    pub(crate) received: Arc<Mutex<Vec<Received>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Received {
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) data: Vec<u8>,
    pub(crate) authenticated_as: Option<String>,
}

impl FakeSmtp {
    /// `auth`: erwarteter Benutzer und Passwort. Mit Angabe verlangt der Server die Anmeldung.
    pub(crate) fn start(auth: Option<(&'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::clone(&received);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let store = Arc::clone(&store);
                std::thread::spawn(move || serve(stream, auth, store));
            }
        });
        Self { port, received }
    }

    pub(crate) fn settings(&self, user: &str, password: &str) -> SmtpSettings {
        SmtpSettings { host: "127.0.0.1".into(), port: self.port, user: user.into(), password: password.into(), security: Security::None }
    }
}

fn serve(stream: std::net::TcpStream, auth: Option<(&str, &str)>, store: Arc<Mutex<Vec<Received>>>) {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    let say = |w: &mut std::net::TcpStream, line: &str| {
        let _ = w.write_all(format!("{line}\r\n").as_bytes());
    };
    say(&mut writer, "220 fake ESMTP");
    let (mut from, mut to, mut user) = (String::new(), Vec::new(), None::<String>);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let cmd = line.trim_end().to_string();
        let upper = cmd.to_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            if auth.is_some() {
                say(&mut writer, "250-fake");
                say(&mut writer, "250-AUTH PLAIN LOGIN");
                say(&mut writer, "250 8BITMIME");
            } else {
                say(&mut writer, "250 fake");
            }
        } else if upper.starts_with("AUTH PLAIN") {
            let encoded = match cmd.split_whitespace().nth(2) {
                Some(e) => e.to_string(),
                None => {
                    say(&mut writer, "334 ");
                    let mut l = String::new();
                    let _ = reader.read_line(&mut l);
                    l.trim().to_string()
                }
            };
            let decoded = B64.decode(encoded).unwrap_or_default();
            let parts: Vec<&[u8]> = decoded.split(|b| *b == 0).collect();
            let (u, p) = (parts.get(1).copied().unwrap_or_default(), parts.get(2).copied().unwrap_or_default());
            match auth {
                Some((eu, ep)) if u == eu.as_bytes() && p == ep.as_bytes() => {
                    user = Some(eu.to_string());
                    say(&mut writer, "235 ok");
                }
                _ => say(&mut writer, "535 5.7.8 Authentication credentials invalid"),
            }
        } else if upper.starts_with("AUTH LOGIN") {
            say(&mut writer, "334 VXNlcm5hbWU6");
            let mut u = String::new();
            let _ = reader.read_line(&mut u);
            say(&mut writer, "334 UGFzc3dvcmQ6");
            let mut p = String::new();
            let _ = reader.read_line(&mut p);
            let u = String::from_utf8(B64.decode(u.trim()).unwrap_or_default()).unwrap_or_default();
            let p = String::from_utf8(B64.decode(p.trim()).unwrap_or_default()).unwrap_or_default();
            match auth {
                Some((eu, ep)) if u == eu && p == ep => {
                    user = Some(u);
                    say(&mut writer, "235 ok");
                }
                _ => say(&mut writer, "535 5.7.8 Authentication credentials invalid"),
            }
        } else if upper.starts_with("MAIL FROM") {
            if auth.is_some() && user.is_none() {
                say(&mut writer, "530 Authentication required");
                continue;
            }
            from = cmd[10..].trim().trim_matches(|c| c == '<' || c == '>').to_string();
            say(&mut writer, "250 ok");
        } else if upper.starts_with("RCPT TO") {
            to.push(cmd[8..].trim().trim_matches(|c| c == '<' || c == '>').to_string());
            say(&mut writer, "250 ok");
        } else if upper == "DATA" {
            say(&mut writer, "354 go ahead");
            let mut data = Vec::new();
            loop {
                let mut l = Vec::new();
                if reader.read_until(b'\n', &mut l).unwrap_or(0) == 0 {
                    return;
                }
                if l == b".\r\n" {
                    break;
                }
                // Punkt am Zeilenanfang wird bei der Übertragung verdoppelt.
                if l.starts_with(b"..") {
                    l.remove(0);
                }
                data.extend_from_slice(&l);
            }
            store.lock().unwrap().push(Received { from: from.clone(), to: to.clone(), data, authenticated_as: user.clone() });
            say(&mut writer, "250 queued");
        } else if upper == "RSET" {
            say(&mut writer, "250 ok");
        } else if upper == "QUIT" {
            say(&mut writer, "221 bye");
            return;
        } else {
            say(&mut writer, "502 not implemented");
        }
    }
}

