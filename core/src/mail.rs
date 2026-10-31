//! Mailversand über den eingetragenen SMTP-Server (Portierung von `reisen/mail.py` des alten Dienstreisebusters).
//!
//! Die Mail geht direkt von diesem Rechner an den SMTP-Server. Der Server der App sieht sie nie.

use std::time::Duration;

use lettre::message::{header::ContentType, Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// So groß darf die Mail mit allen Anhängen höchstens werden (üblicher Grenzwert der Mailserver). Gemessen wird die
/// kodierte Größe: Anhänge gehen als Base64 mit Zeilenumbrüchen und werden dadurch etwa ein Drittel größer als die
/// Dateien (siehe [`encoded_size`]). An Dateien passen damit zusammen etwa 14,6 MB.
pub const MAX_MAIL_BYTES: usize = 20 * 1024 * 1024;

/// Größe eines Anhangs in der Mail: Base64 mit einem Zeilenumbruch (CRLF) nach je 76 Zeichen, wie es `lettre`
/// schreibt.
pub fn encoded_size(raw: usize) -> usize {
    let base64 = raw.div_ceil(3) * 4;
    base64 + base64.div_ceil(76) * 2
}

/// Sind Anhänge dieser Größen (in Byte) zusammen zu groß für eine Mail?
pub fn too_large(sizes: impl IntoIterator<Item = usize>) -> bool {
    sizes.into_iter().map(encoded_size).sum::<usize>() > MAX_MAIL_BYTES
}

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("In den Einstellungen fehlt: {0}.")]
    Missing(String),
    #[error("Die Adresse „{0}“ ist ungültig.")]
    InvalidAddress(String),
    #[error("Die Mail wäre mit den Anhängen größer als 20 MB. Anhänge werden beim Versand etwa ein Drittel größer, die Dateien dürfen zusammen also höchstens etwa 14,6 MB haben.")]
    TooLarge,
    #[error("Versand fehlgeschlagen: {0}")]
    Send(String),
}

/// Verschlüsselung der Verbindung zum SMTP-Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// Unverschlüsselt anfangen, dann auf TLS wechseln (meist Port 587).
    StartTls,
    /// Von Anfang an TLS (meist Port 465).
    Tls,
    /// Keine Verschlüsselung. Nur für Tests und vertrauenswürdige lokale Server.
    None,
}

impl Security {
    /// Liest die Angabe aus den Stammdaten. Unbekanntes wird zu STARTTLS, der sicheren Standardeinstellung.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "ssl" | "tls" | "ssl/tls" => Security::Tls,
            "keine" | "none" => Security::None,
            _ => Security::StartTls,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub security: Security,
}

#[derive(Debug, Clone)]
pub struct MailAttachment {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Mail {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<MailAttachment>,
}

/// Prüft, ob alles Nötige eingetragen ist.
pub fn check(settings: &SmtpSettings, from: &str, to: &str) -> Result<(), MailError> {
    let mut missing = Vec::new();
    if to.trim().is_empty() {
        missing.push("Empfänger");
    }
    if from.trim().is_empty() {
        missing.push("Absenderadresse");
    }
    if settings.host.trim().is_empty() {
        missing.push("SMTP-Server");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(MailError::Missing(missing.join(", ")))
    }
}

fn mailbox(address: &str) -> Result<Mailbox, MailError> {
    address.trim().parse().map_err(|_| MailError::InvalidAddress(address.trim().to_string()))
}

fn content_type(name: &str) -> ContentType {
    let lower = name.to_lowercase();
    let mime = if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };
    ContentType::parse(mime).expect("gültiger MIME-Typ")
}

/// Baut die Nachricht (ohne zu senden).
pub fn build(mail: &Mail) -> Result<Message, MailError> {
    if too_large(mail.attachments.iter().map(|a| a.bytes.len())) {
        return Err(MailError::TooLarge);
    }
    let mut parts = MultiPart::mixed().singlepart(SinglePart::plain(mail.body.clone()));
    for attachment in &mail.attachments {
        parts = parts.singlepart(Attachment::new(attachment.name.clone()).body(attachment.bytes.clone(), content_type(&attachment.name)));
    }
    Message::builder()
        .from(mailbox(&mail.from)?)
        .to(mailbox(&mail.to)?)
        .subject(mail.subject.clone())
        .multipart(parts)
        .map_err(|e| MailError::Send(e.to_string()))
}

/// Baut die Verbindung zum SMTP-Server.
pub fn transport(settings: &SmtpSettings) -> Result<SmtpTransport, MailError> {
    let host = settings.host.trim();
    let builder = match settings.security {
        Security::StartTls => SmtpTransport::starttls_relay(host),
        Security::Tls => SmtpTransport::relay(host),
        Security::None => Ok(SmtpTransport::builder_dangerous(host)),
    }
    .map_err(|e| MailError::Send(e.to_string()))?;
    let mut builder = builder.port(settings.port).timeout(Some(TIMEOUT));
    if !settings.user.is_empty() {
        builder = builder.credentials(Credentials::new(settings.user.clone(), settings.password.clone()));
    }
    Ok(builder.build())
}

/// Sendet eine Mail.
pub fn send(settings: &SmtpSettings, mail: &Mail) -> Result<(), MailError> {
    check(settings, &mail.from, &mail.to)?;
    let message = build(mail)?;
    transport(settings)?.send(&message).map_err(|e| MailError::Send(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_smtp::FakeSmtp;
    use mail_parser::MimeHeaders;
    use std::net::TcpListener;

    fn mail_with(attachments: Vec<MailAttachment>) -> Mail {
        Mail {
            from: "max.mustermann@example.org".into(),
            to: "verwaltung@example.org".into(),
            subject: "Antrag auf Genehmigung einer Fortbildungsreise am 16.11.2026".into(),
            body: "Sehr geehrte Damen und Herren,\n\nanbei übersende ich meinen Antrag.\n\nMit freundlichen Grüßen\nMax Mustermann".into(),
            attachments,
        }
    }

    #[test]
    fn sends_a_mail_with_attachments() {
        let server = FakeSmtp::start(None);
        let pdf = b"%PDF-1.4 ... pretend pdf bytes \x00\x01\x02\xff".to_vec();
        let png: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
        let mail = mail_with(vec![
            MailAttachment { name: "antrag-2026-11-16.pdf".into(), bytes: pdf.clone() },
            MailAttachment { name: "Hotelrechnung Müller.png".into(), bytes: png.clone() },
        ]);
        send(&server.settings("", ""), &mail).unwrap();

        let got = server.received.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].from, "max.mustermann@example.org");
        assert_eq!(got[0].to, ["verwaltung@example.org"]);

        let parsed = mail_parser::MessageParser::default().parse(&got[0].data).expect("lesbare Nachricht");
        assert_eq!(parsed.subject(), Some("Antrag auf Genehmigung einer Fortbildungsreise am 16.11.2026"));
        let body = parsed.body_text(0).unwrap().to_string();
        assert!(body.contains("anbei übersende ich meinen Antrag") && body.contains("Mit freundlichen Grüßen"), "{body}");
        let attachments: Vec<_> = parsed.attachments().collect();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].attachment_name(), Some("antrag-2026-11-16.pdf"));
        assert_eq!(attachments[0].contents(), pdf.as_slice());
        assert_eq!(attachments[1].attachment_name(), Some("Hotelrechnung Müller.png"));
        assert_eq!(attachments[1].contents(), png.as_slice());
    }

    #[test]
    fn sends_without_attachments_and_with_special_text() {
        let server = FakeSmtp::start(None);
        let mut mail = mail_with(vec![]);
        mail.subject = "Fahrkosten für Größe – „Test“ €".into();
        mail.body = "Zeile mit Punkt am Anfang:\n.\n..doppelt\nUmlaute äöüß und Zeile\n".into();
        send(&server.settings("", ""), &mail).unwrap();
        let got = server.received.lock().unwrap().clone();
        let parsed = mail_parser::MessageParser::default().parse(&got[0].data).unwrap();
        assert_eq!(parsed.subject(), Some("Fahrkosten für Größe – „Test“ €"));
        let body = parsed.body_text(0).unwrap().to_string();
        assert!(body.contains("Umlaute äöüß"), "{body}");
        assert!(body.contains("\n.\n") || body.contains("\r\n.\r\n"), "Punkt am Zeilenanfang geht nicht verloren: {body:?}");
        assert_eq!(parsed.attachments().count(), 0);
    }

    #[test]
    fn authenticates_with_the_given_credentials() {
        let server = FakeSmtp::start(Some(("max", "geheim")));
        send(&server.settings("max", "geheim"), &mail_with(vec![])).unwrap();
        assert_eq!(server.received.lock().unwrap()[0].authenticated_as.as_deref(), Some("max"));

        let err = send(&server.settings("max", "falsch"), &mail_with(vec![])).unwrap_err();
        assert!(matches!(err, MailError::Send(_)), "{err:?}");
        assert!(err.to_string().starts_with("Versand fehlgeschlagen:"));
        let err = send(&server.settings("", ""), &mail_with(vec![])).unwrap_err();
        assert!(matches!(err, MailError::Send(_)), "ohne Anmeldedaten nimmt der Server nichts an: {err:?}");
        assert_eq!(server.received.lock().unwrap().len(), 1, "fehlgeschlagene Versuche liefern nichts aus");
    }

    #[test]
    fn reports_missing_settings_and_invalid_input() {
        let base = SmtpSettings { host: "mail.example.org".into(), port: 587, user: String::new(), password: String::new(), security: Security::StartTls };
        let none = SmtpSettings { host: "".into(), ..base.clone() };
        assert_eq!(check(&none, "", "").unwrap_err().to_string(), "In den Einstellungen fehlt: Empfänger, Absenderadresse, SMTP-Server.");
        assert_eq!(check(&base, "a@b.de", "").unwrap_err().to_string(), "In den Einstellungen fehlt: Empfänger.");
        assert!(check(&base, "a@b.de", "c@d.de").is_ok());

        let mut mail = mail_with(vec![]);
        mail.to = "kein-gueltige-adresse".into();
        assert!(matches!(send(&base, &mail), Err(MailError::InvalidAddress(a)) if a == "kein-gueltige-adresse"));
        mail.to = "ok@example.org".into();
        mail.from = "auch kaputt".into();
        assert!(matches!(send(&base, &mail), Err(MailError::InvalidAddress(_))));
    }

    /// Größte Dateigröße, deren kodierte Größe noch ins Limit passt.
    fn largest_raw_size() -> usize {
        let mut raw = MAX_MAIL_BYTES * 3 / 4;
        while encoded_size(raw + 1) <= MAX_MAIL_BYTES {
            raw += 1;
        }
        while encoded_size(raw) > MAX_MAIL_BYTES {
            raw -= 1;
        }
        raw
    }

    #[test]
    fn limit_counts_the_encoded_size() {
        // Rund 14,6 MB an Dateien ergeben eine Mail von 20 MB.
        let raw = largest_raw_size();
        assert!((15_250_000..15_400_000).contains(&raw), "{raw}");
        assert!(!too_large([raw]) && too_large([raw + 3]));
        // 19 MB an Dateien waren früher erlaubt, ergeben aber eine Mail von über 25 MB.
        assert!(too_large([19 * 1024 * 1024]));
        let over = mail_with(vec![MailAttachment { name: "gross.pdf".into(), bytes: vec![0; raw + 3] }]);
        assert!(matches!(build(&over), Err(MailError::TooLarge)));
        let split = mail_with(vec![
            MailAttachment { name: "a.pdf".into(), bytes: vec![0; raw / 2 + 3] },
            MailAttachment { name: "b.pdf".into(), bytes: vec![0; raw / 2 + 3] },
        ]);
        assert!(matches!(build(&split), Err(MailError::TooLarge)));
    }

    #[test]
    fn encoded_size_matches_the_built_message() {
        // Binärdaten wie in PDFs: `lettre` wählt dafür Base64 (für Text wählt es eine kürzere Kodierung, die Grenze
        // ist dann nur vorsichtiger).
        let size_of = |n: usize| {
            let bytes = (0..n).map(|i| 0x80 | (i * 7) as u8).collect();
            build(&mail_with(vec![MailAttachment { name: "a.pdf".into(), bytes }])).unwrap().formatted().len()
        };
        let empty = size_of(0);
        for n in [57, 1000, 123_456] {
            let grown = size_of(n) - empty;
            let expected = encoded_size(n);
            assert!(grown.abs_diff(expected) <= 4, "{n}: Mail wächst um {grown}, erwartet {expected}");
        }
        // Eine Mail genau am Limit bleibt (bis auf Kopfzeilen und Text) unter 20 MB.
        let at_limit = mail_with(vec![MailAttachment { name: "a.pdf".into(), bytes: vec![0; largest_raw_size()] }]);
        assert!(build(&at_limit).unwrap().formatted().len() <= MAX_MAIL_BYTES + 4096);
    }

    #[test]
    fn unreachable_server_is_reported() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let settings = SmtpSettings { host: "127.0.0.1".into(), port, user: String::new(), password: String::new(), security: Security::None };
        let err = send(&settings, &mail_with(vec![])).unwrap_err();
        assert!(matches!(err, MailError::Send(_)));
    }

    #[test]
    fn security_setting_is_parsed_and_transports_build() {
        assert_eq!(Security::parse("starttls"), Security::StartTls);
        assert_eq!(Security::parse("ssl"), Security::Tls);
        assert_eq!(Security::parse("SSL/TLS"), Security::Tls);
        assert_eq!(Security::parse("keine"), Security::None);
        assert_eq!(Security::parse("etwas anderes"), Security::StartTls, "unbekannt wird sicher");
        assert_eq!(Security::parse(""), Security::StartTls);
        for security in [Security::StartTls, Security::Tls, Security::None] {
            let settings = SmtpSettings { host: "mail.example.org".into(), port: 587, user: "u".into(), password: "p".into(), security };
            assert!(transport(&settings).is_ok(), "{security:?}");
        }
    }
}
