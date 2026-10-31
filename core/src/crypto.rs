//! Schlüsselverwaltung und Verschlüsselung der Einträge.
//!
//! Aufbau (siehe `docs/ARCHITEKTUR.md`):
//!
//! - Ein zufälliger Datenschlüssel (32 Byte) verschlüsselt alle Einträge.
//! - Der Datenschlüssel liegt zweimal verpackt im [`Keyring`]: einmal unter einem Schlüssel aus dem
//!   Master-Passwort (Argon2id), einmal unter einem Schlüssel aus dem Recovery-Key (HKDF-SHA-256).
//! - Einträge sind `0x01 || Nonce (24) || Chiffretext || Tag` (XChaCha20-Poly1305). Die Eintrags-ID ist
//!   als zusätzliche authentifizierte Daten gebunden.
//! - Der Keyring trägt eine Prüfsumme (HMAC-SHA-256 mit einem Schlüssel aus dem Datenschlüssel). Ein entsperrter
//!   Rechner erkennt daran, dass ein neuerer Keyring vom Server von jemandem stammt, der den Datenschlüssel kennt,
//!   und übernimmt ihn (zum Beispiel nach einem Passwortwechsel auf einem anderen Rechner).

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use data_encoding::BASE32_NOPAD;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const SALT_LEN: usize = 16;

const ENTRY_FORMAT_VERSION: u8 = 1;
const KEYRING_VERSION: u32 = 1;

const AAD_KEYRING_PASSWORD: &[u8] = b"bdbb/keyring/v1/password";
const AAD_KEYRING_RECOVERY: &[u8] = b"bdbb/keyring/v1/recovery";
const AAD_ENTRY_PREFIX: &[u8] = b"bdbb/entry/v1/";
const INFO_ENTRY_KEY: &[u8] = b"bdbb/entry-key/v1";
const INFO_RECOVERY_KEK: &[u8] = b"bdbb/kek/recovery/v1";
const INFO_KEYRING_MAC: &[u8] = b"bdbb/keyring-mac/v1";

/// Obergrenzen für Argon2id-Parameter aus einem Keyring. Sie schützen vor einem Keyring, der den
/// Rechner mit absurden Werten lahmlegen soll. Eine Untergrenze gibt es bewusst nicht: Ein schwächerer
/// Keyring hilft einem Angreifer nur bei dem Passwort, das er ohnehin offline angreifen kann.
const MAX_M_KIB: u32 = 1024 * 1024; // 1 GiB
const MAX_T: u32 = 20;
const MAX_P: u32 = 16;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Master-Passwort oder Recovery-Key falsch")]
    WrongSecret,
    #[error("Das Master-Passwort darf nicht leer sein")]
    EmptyPassword,
    #[error("Ungültiger Recovery-Key")]
    InvalidRecoveryKey,
    #[error("Ungültige Argon2-Parameter: {0}")]
    InvalidKdfParams(String),
    #[error("Eintrag ist beschädigt oder gehört zu einer anderen ID")]
    CorruptEntry,
    #[error("Nicht unterstützte Version")]
    UnsupportedVersion,
    #[error("Keyring ist ungültig")]
    InvalidKeyring,
    #[error("Zufallsquelle nicht verfügbar")]
    Random,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Parameter für Argon2id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Speicher in KiB.
    pub m_kib: u32,
    /// Durchläufe.
    pub t: u32,
    /// Parallelität.
    pub p: u32,
}

impl KdfParams {
    /// Standardwerte für den Alltag: 256 MiB, 3 Durchläufe, 1 Thread.
    pub const fn standard() -> Self {
        Self { m_kib: 256 * 1024, t: 3, p: 1 }
    }

    fn validate(&self) -> Result<()> {
        if self.m_kib > MAX_M_KIB || self.t > MAX_T || self.p > MAX_P {
            return Err(Error::InvalidKdfParams("Wert über der erlaubten Obergrenze".into()));
        }
        Params::new(self.m_kib, self.t, self.p, Some(KEY_LEN))
            .map(|_| ())
            .map_err(|e| Error::InvalidKdfParams(e.to_string()))
    }
}

/// Serialisierte Bytes als Base64-Zeichenkette.
mod b64 {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        B64.decode(text).map_err(serde::de::Error::custom)
    }
}

/// Wie [`b64`], für ein optionales Feld.
mod b64_opt {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => s.serialize_str(&B64.encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(text) => B64.decode(text).map(Some).map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct KdfRecord {
    alg: String,
    #[serde(flatten)]
    params: KdfParams,
    #[serde(with = "b64")]
    salt: Vec<u8>,
}

/// Der verpackte Datenschlüssel samt Parametern. Er liegt auf dem Server und ist ohne Master-Passwort
/// oder Recovery-Key wertlos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keyring {
    version: u32,
    kdf: KdfRecord,
    #[serde(with = "b64")]
    wrapped_by_password: Vec<u8>,
    #[serde(with = "b64")]
    wrapped_by_recovery: Vec<u8>,
    /// Prüfsumme über alle anderen Angaben (siehe [`Vault::seal_keyring`]). Fehlt bei Keyrings, die vor ihrer
    /// Einführung angelegt wurden. Ältere Clients ignorieren das Feld.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "b64_opt")]
    mac: Option<Vec<u8>>,
    /// Bestandskennung: Zufallswert, den jedes Neubefüllen des Servers neu setzt (siehe
    /// `docs/ARCHITEKTUR.md`, „Server neu befüllen“). Weicht sie auf dem Server von der lokalen ab, hat ein
    /// anderer Rechner den Server neu befüllt, und die Folgenummern dieses Rechners gelten nicht mehr. Fehlt, solange
    /// der Server nie neu befüllt wurde. Sie ist von der Prüfsumme abgedeckt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    epoch: Option<String>,
}

impl Keyring {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Keyring lässt sich immer serialisieren")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let keyring: Keyring = serde_json::from_str(json).map_err(|_| Error::InvalidKeyring)?;
        if keyring.version != KEYRING_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if keyring.kdf.alg != "argon2id" || keyring.kdf.salt.len() != SALT_LEN {
            return Err(Error::InvalidKeyring);
        }
        keyring.kdf.params.validate()?;
        if let Some(epoch) = &keyring.epoch {
            if !valid_epoch(epoch) {
                return Err(Error::InvalidKeyring);
            }
        }
        Ok(keyring)
    }

    /// Bestandskennung (siehe Feld `epoch`).
    pub fn epoch(&self) -> Option<&str> {
        self.epoch.as_deref()
    }

    /// Derselbe Keyring mit einer neuen, zufälligen Bestandskennung. Er ist danach nicht versiegelt und muss mit
    /// [`Vault::seal_keyring`] eine neue Prüfsumme bekommen.
    pub fn with_new_epoch(&self) -> Result<Keyring> {
        let mut bytes = [0u8; 16];
        random_fill(&mut bytes)?;
        let epoch: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Keyring { epoch: Some(epoch), mac: None, ..self.clone() })
    }

    /// Hat der Keyring eine Prüfsumme? (Ältere Keyrings haben keine, siehe [`Vault::seal_keyring`].)
    pub fn is_sealed(&self) -> bool {
        self.mac.is_some()
    }

    /// Alle Angaben außer der Prüfsumme in eindeutiger Form (jedes Teil mit Längenangabe).
    fn mac_input(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut part = |bytes: &[u8]| {
            out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            out.extend_from_slice(bytes);
        };
        part(&self.version.to_be_bytes());
        part(self.kdf.alg.as_bytes());
        part(&self.kdf.params.m_kib.to_be_bytes());
        part(&self.kdf.params.t.to_be_bytes());
        part(&self.kdf.params.p.to_be_bytes());
        part(&self.kdf.salt);
        part(&self.wrapped_by_password);
        part(&self.wrapped_by_recovery);
        // Nur vorhanden, wenn gesetzt: Prüfsummen älterer Keyrings ohne Bestandskennung bleiben gültig.
        if let Some(epoch) = &self.epoch {
            part(epoch.as_bytes());
        }
        out
    }
}

/// Bestandskennung: 1 bis 64 Zeichen aus Buchstaben, Ziffern und `-`.
fn valid_epoch(epoch: &str) -> bool {
    (1..=64).contains(&epoch.len()) && epoch.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Recovery-Key: 256 Bit, angezeigt als Base32 in Vierergruppen.
pub struct RecoveryKey(Zeroizing<[u8; KEY_LEN]>);

impl RecoveryKey {
    fn generate() -> Result<Self> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        random_fill(&mut *bytes)?;
        Ok(Self(bytes))
    }

    /// Zur Anzeige und zum Ausdrucken, zum Beispiel `ABCD-EFGH-…`.
    pub fn to_display(&self) -> String {
        let encoded = BASE32_NOPAD.encode(&*self.0);
        encoded
            .as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).expect("Base32 ist ASCII"))
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Liest einen eingegebenen Recovery-Key. Groß- und Kleinschreibung, Leerzeichen und Bindestriche
    /// spielen keine Rolle.
    pub fn parse(input: &str) -> Result<Self> {
        let cleaned: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let decoded = BASE32_NOPAD
            .decode(cleaned.as_bytes())
            .map_err(|_| Error::InvalidRecoveryKey)?;
        let bytes: [u8; KEY_LEN] = decoded.try_into().map_err(|_| Error::InvalidRecoveryKey)?;
        Ok(Self(Zeroizing::new(bytes)))
    }

    fn kek(&self) -> Zeroizing<[u8; KEY_LEN]> {
        hkdf_expand(&*self.0, INFO_RECOVERY_KEK)
    }
}

/// Entsperrter Zustand: hält den Datenschlüssel im Speicher.
pub struct Vault {
    data_key: Zeroizing<[u8; KEY_LEN]>,
    entry_key: Zeroizing<[u8; KEY_LEN]>,
    mac_key: Zeroizing<[u8; KEY_LEN]>,
}

impl Vault {
    fn from_data_key(data_key: Zeroizing<[u8; KEY_LEN]>) -> Self {
        let entry_key = hkdf_expand(&*data_key, INFO_ENTRY_KEY);
        let mac_key = hkdf_expand(&*data_key, INFO_KEYRING_MAC);
        Self { data_key, entry_key, mac_key }
    }

    fn keyring_mac(&self, keyring: &Keyring) -> Hmac<Sha256> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&*self.mac_key).expect("HMAC nimmt jede Schlüssellänge");
        mac.update(&keyring.mac_input());
        mac
    }

    /// Versieht den Keyring mit der Prüfsumme dieses Tresors.
    pub fn seal_keyring(&self, keyring: &Keyring) -> Keyring {
        let mut sealed = keyring.clone();
        sealed.mac = Some(self.keyring_mac(keyring).finalize().into_bytes().to_vec());
        sealed
    }

    /// Stammt der Keyring von jemandem, der den Datenschlüssel dieses Tresors kennt? Ohne Prüfsumme: nein.
    pub fn verify_keyring(&self, keyring: &Keyring) -> bool {
        match &keyring.mac {
            Some(tag) => self.keyring_mac(keyring).verify_slice(tag).is_ok(),
            None => false,
        }
    }

    /// Richtet einen neuen Tresor ein. Gibt den entsperrten Tresor, den Keyring (für den Server) und den
    /// Recovery-Key zurück. Der Recovery-Key wird nirgends gespeichert und muss dem Nutzer jetzt angezeigt
    /// werden.
    pub fn create(password: &str, params: KdfParams) -> Result<(Vault, Keyring, RecoveryKey)> {
        let mut data_key = Zeroizing::new([0u8; KEY_LEN]);
        random_fill(&mut *data_key)?;
        let recovery = RecoveryKey::generate()?;

        let (kdf, wrapped_by_password) = wrap_with_password(&data_key, password, params)?;
        let wrapped_by_recovery = wrap(&recovery.kek(), &data_key, AAD_KEYRING_RECOVERY)?;

        let keyring = Keyring {
            version: KEYRING_VERSION,
            kdf,
            wrapped_by_password,
            wrapped_by_recovery,
            mac: None,
            epoch: None,
        };
        let vault = Vault::from_data_key(data_key);
        let keyring = vault.seal_keyring(&keyring);
        Ok((vault, keyring, recovery))
    }

    /// Entsperrt mit dem Master-Passwort.
    pub fn unlock_with_password(keyring: &Keyring, password: &str) -> Result<Vault> {
        keyring.kdf.params.validate()?;
        let kek = derive_password_key(password, &keyring.kdf.salt, &keyring.kdf.params)?;
        let data_key = unwrap_key(&kek, &keyring.wrapped_by_password, AAD_KEYRING_PASSWORD)?;
        Vault::checked(data_key, keyring)
    }

    /// Entsperrt mit dem Recovery-Key (so, wie er angezeigt wurde).
    pub fn unlock_with_recovery(keyring: &Keyring, recovery_key: &str) -> Result<Vault> {
        let recovery = RecoveryKey::parse(recovery_key)?;
        let data_key = unwrap_key(&recovery.kek(), &keyring.wrapped_by_recovery, AAD_KEYRING_RECOVERY)?;
        Vault::checked(data_key, keyring)
    }

    /// Hat ein Keyring eine Prüfsumme, muss sie zum entpackten Datenschlüssel passen. Sonst sind Teile des Keyrings
    /// ausgetauscht worden.
    fn checked(data_key: Zeroizing<[u8; KEY_LEN]>, keyring: &Keyring) -> Result<Vault> {
        let vault = Vault::from_data_key(data_key);
        if keyring.mac.is_some() && !vault.verify_keyring(keyring) {
            return Err(Error::InvalidKeyring);
        }
        Ok(vault)
    }

    /// Setzt ein neues Master-Passwort (neues Salz). Die Daten bleiben unverändert. Der Recovery-Key
    /// bleibt gültig.
    pub fn change_password(&self, keyring: &Keyring, new_password: &str, params: KdfParams) -> Result<Keyring> {
        let (kdf, wrapped_by_password) = wrap_with_password(&self.data_key, new_password, params)?;
        Ok(self.seal_keyring(&Keyring {
            version: KEYRING_VERSION,
            kdf,
            wrapped_by_password,
            wrapped_by_recovery: keyring.wrapped_by_recovery.clone(),
            mac: None,
            epoch: keyring.epoch.clone(),
        }))
    }

    /// Erzeugt einen neuen Recovery-Key. Der alte wird ungültig, sobald der neue Keyring gespeichert ist.
    pub fn rotate_recovery_key(&self, keyring: &Keyring) -> Result<(Keyring, RecoveryKey)> {
        let recovery = RecoveryKey::generate()?;
        let wrapped_by_recovery = wrap(&recovery.kek(), &self.data_key, AAD_KEYRING_RECOVERY)?;
        let new_keyring = self.seal_keyring(&Keyring {
            version: KEYRING_VERSION,
            kdf: keyring.kdf.clone(),
            wrapped_by_password: keyring.wrapped_by_password.clone(),
            wrapped_by_recovery,
            mac: None,
            epoch: keyring.epoch.clone(),
        });
        Ok((new_keyring, recovery))
    }

    /// Verschlüsselt den Klartext eines Eintrags. Die ID ist an den Chiffretext gebunden.
    pub fn encrypt_entry(&self, id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(self.entry_key.as_slice().into());
        let mut nonce = [0u8; NONCE_LEN];
        random_fill(&mut nonce)?;
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: &entry_aad(id) })
            .map_err(|_| Error::Random)?;

        let mut blob = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        blob.push(ENTRY_FORMAT_VERSION);
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(blob)
    }

    /// Entschlüsselt einen Eintrag. Schlägt fehl, wenn der Blob verändert wurde oder zu einer anderen ID
    /// gehört.
    pub fn decrypt_entry(&self, id: &str, blob: &[u8]) -> Result<Vec<u8>> {
        if blob.len() < 1 + NONCE_LEN + TAG_LEN {
            return Err(Error::CorruptEntry);
        }
        if blob[0] != ENTRY_FORMAT_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        let (nonce, ciphertext) = blob[1..].split_at(NONCE_LEN);
        let cipher = XChaCha20Poly1305::new(self.entry_key.as_slice().into());
        cipher
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad: &entry_aad(id) })
            .map_err(|_| Error::CorruptEntry)
    }
}

fn entry_aad(id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(AAD_ENTRY_PREFIX.len() + id.len());
    aad.extend_from_slice(AAD_ENTRY_PREFIX);
    aad.extend_from_slice(id.as_bytes());
    aad
}

fn random_fill(buf: &mut [u8]) -> Result<()> {
    getrandom::getrandom(buf).map_err(|_| Error::Random)
}

fn hkdf_expand(ikm: &[u8], info: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(None, ikm)
        .expand(info, &mut *out)
        .expect("32 Byte sind eine gültige HKDF-Ausgabelänge");
    out
}

/// Normalisiert das Passwort (Unicode NFKC), damit dieselbe Eingabe auf verschiedenen Systemen denselben
/// Schlüssel ergibt.
fn derive_password_key(password: &str, salt: &[u8], params: &KdfParams) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    if password.is_empty() {
        return Err(Error::EmptyPassword);
    }
    let argon_params = Params::new(params.m_kib, params.t, params.p, Some(KEY_LEN))
        .map_err(|e| Error::InvalidKdfParams(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);

    let normalized: Zeroizing<String> = Zeroizing::new(password.nfkc().collect());
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(normalized.as_bytes(), salt, &mut *out)
        .map_err(|e| Error::InvalidKdfParams(e.to_string()))?;
    Ok(out)
}

fn wrap_with_password(data_key: &[u8; KEY_LEN], password: &str, params: KdfParams) -> Result<(KdfRecord, Vec<u8>)> {
    params.validate()?;
    let mut salt = vec![0u8; SALT_LEN];
    random_fill(&mut salt)?;
    let kek = derive_password_key(password, &salt, &params)?;
    let wrapped = wrap(&kek, data_key, AAD_KEYRING_PASSWORD)?;
    Ok((KdfRecord { alg: "argon2id".into(), params, salt }, wrapped))
}

/// `Nonce (24) || Chiffretext || Tag`.
fn wrap(kek: &[u8; KEY_LEN], data_key: &[u8; KEY_LEN], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(kek.into());
    let mut nonce = [0u8; NONCE_LEN];
    random_fill(&mut nonce)?;
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: data_key, aad })
        .map_err(|_| Error::Random)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn unwrap_key(kek: &[u8; KEY_LEN], wrapped: &[u8], aad: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    if wrapped.len() != NONCE_LEN + KEY_LEN + TAG_LEN {
        return Err(Error::InvalidKeyring);
    }
    let (nonce, ciphertext) = wrapped.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(kek.into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad })
            .map_err(|_| Error::WrongSecret)?,
    );
    let key: [u8; KEY_LEN] = plain.as_slice().try_into().map_err(|_| Error::InvalidKeyring)?;
    Ok(Zeroizing::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    /// Schwache Parameter, damit die Tests schnell laufen.
    const FAST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };

    fn new_vault() -> (Vault, Keyring, RecoveryKey) {
        Vault::create("korrekt pferd batterie heftklammer", FAST).unwrap()
    }

    #[test]
    fn unlock_with_password_gives_same_key() {
        let (vault, keyring, _) = new_vault();
        let blob = vault.encrypt_entry("e1", b"geheim").unwrap();

        let again = Vault::unlock_with_password(&keyring, "korrekt pferd batterie heftklammer").unwrap();
        assert_eq!(again.decrypt_entry("e1", &blob).unwrap(), b"geheim");
    }

    #[test]
    fn wrong_password_is_rejected() {
        let (_, keyring, _) = new_vault();
        let result = Vault::unlock_with_password(&keyring, "falsch");
        assert!(matches!(result, Err(Error::WrongSecret)));
    }

    #[test]
    fn empty_password_is_rejected() {
        assert!(matches!(Vault::create("", FAST), Err(Error::EmptyPassword)));
        let (_, keyring, _) = new_vault();
        assert!(matches!(Vault::unlock_with_password(&keyring, ""), Err(Error::EmptyPassword)));
    }

    #[test]
    fn unlock_with_recovery_key() {
        let (vault, keyring, recovery) = new_vault();
        let blob = vault.encrypt_entry("e1", b"geheim").unwrap();

        let again = Vault::unlock_with_recovery(&keyring, &recovery.to_display()).unwrap();
        assert_eq!(again.decrypt_entry("e1", &blob).unwrap(), b"geheim");
    }

    #[test]
    fn recovery_key_input_is_forgiving() {
        let (_, keyring, recovery) = new_vault();
        let shown = recovery.to_display();
        let sloppy = shown.to_lowercase().replace('-', "  ");
        assert!(Vault::unlock_with_recovery(&keyring, &sloppy).is_ok());
    }

    #[test]
    fn recovery_key_has_expected_shape() {
        let (_, _, recovery) = new_vault();
        let shown = recovery.to_display();
        assert_eq!(shown.replace('-', "").len(), 52);
        assert!(shown.split('-').all(|g| g.len() <= 4));
    }

    #[test]
    fn wrong_or_malformed_recovery_key_is_rejected() {
        let (_, keyring, _) = new_vault();
        assert!(matches!(
            Vault::unlock_with_recovery(&keyring, "kein base32 !!!"),
            Err(Error::InvalidRecoveryKey)
        ));
        let other = RecoveryKey::generate().unwrap().to_display();
        assert!(matches!(Vault::unlock_with_recovery(&keyring, &other), Err(Error::WrongSecret)));
    }

    #[test]
    fn password_is_unicode_normalized() {
        // "é" einmal vorkomponiert, einmal als e + kombinierender Akzent.
        let (_, keyring, _) = Vault::create("caf\u{e9} passwort", FAST).unwrap();
        assert!(Vault::unlock_with_password(&keyring, "cafe\u{301} passwort").is_ok());
    }

    #[test]
    fn change_password_keeps_data_and_recovery() {
        let (vault, keyring, recovery) = new_vault();
        let blob = vault.encrypt_entry("e1", b"geheim").unwrap();

        let new_keyring = vault.change_password(&keyring, "neues passwort", FAST).unwrap();
        assert_ne!(new_keyring.kdf.salt, keyring.kdf.salt);

        assert!(matches!(
            Vault::unlock_with_password(&new_keyring, "korrekt pferd batterie heftklammer"),
            Err(Error::WrongSecret)
        ));
        let by_new = Vault::unlock_with_password(&new_keyring, "neues passwort").unwrap();
        assert_eq!(by_new.decrypt_entry("e1", &blob).unwrap(), b"geheim");

        let by_recovery = Vault::unlock_with_recovery(&new_keyring, &recovery.to_display()).unwrap();
        assert_eq!(by_recovery.decrypt_entry("e1", &blob).unwrap(), b"geheim");
    }

    #[test]
    fn rotate_recovery_key_invalidates_old_one() {
        let (vault, keyring, old_recovery) = new_vault();
        let (new_keyring, new_recovery) = vault.rotate_recovery_key(&keyring).unwrap();

        assert!(matches!(
            Vault::unlock_with_recovery(&new_keyring, &old_recovery.to_display()),
            Err(Error::WrongSecret)
        ));
        assert!(Vault::unlock_with_recovery(&new_keyring, &new_recovery.to_display()).is_ok());
        assert!(Vault::unlock_with_password(&new_keyring, "korrekt pferd batterie heftklammer").is_ok());
    }

    #[test]
    fn entry_roundtrip_and_nonces_differ() {
        let (vault, _, _) = new_vault();
        let a = vault.encrypt_entry("e1", b"gleicher text").unwrap();
        let b = vault.encrypt_entry("e1", b"gleicher text").unwrap();
        assert_ne!(a, b);
        assert_eq!(vault.decrypt_entry("e1", &a).unwrap(), b"gleicher text");
        assert_eq!(a[0], ENTRY_FORMAT_VERSION);
    }

    #[test]
    fn empty_plaintext_works() {
        let (vault, _, _) = new_vault();
        let blob = vault.encrypt_entry("e1", b"").unwrap();
        assert_eq!(vault.decrypt_entry("e1", &blob).unwrap(), b"");
    }

    #[test]
    fn entry_is_bound_to_its_id() {
        let (vault, _, _) = new_vault();
        let blob = vault.encrypt_entry("e1", b"geheim").unwrap();
        assert!(matches!(vault.decrypt_entry("e2", &blob), Err(Error::CorruptEntry)));
    }

    #[test]
    fn tampered_entry_is_rejected() {
        let (vault, _, _) = new_vault();
        let mut blob = vault.encrypt_entry("e1", b"geheim").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 1;
        assert!(matches!(vault.decrypt_entry("e1", &blob), Err(Error::CorruptEntry)));
    }

    #[test]
    fn short_or_unknown_version_entry_is_rejected() {
        let (vault, _, _) = new_vault();
        assert!(matches!(vault.decrypt_entry("e1", &[1, 2, 3]), Err(Error::CorruptEntry)));
        let mut blob = vault.encrypt_entry("e1", b"geheim").unwrap();
        blob[0] = 99;
        assert!(matches!(vault.decrypt_entry("e1", &blob), Err(Error::UnsupportedVersion)));
    }

    #[test]
    fn entry_from_other_vault_is_rejected() {
        let (a, _, _) = new_vault();
        let (b, _, _) = new_vault();
        let blob = a.encrypt_entry("e1", b"geheim").unwrap();
        assert!(matches!(b.decrypt_entry("e1", &blob), Err(Error::CorruptEntry)));
    }

    #[test]
    fn keyring_json_roundtrip() {
        let (_, keyring, _) = new_vault();
        let json = keyring.to_json();
        let parsed = Keyring::from_json(&json).unwrap();
        assert_eq!(parsed, keyring);
        assert!(Vault::unlock_with_password(&parsed, "korrekt pferd batterie heftklammer").is_ok());
    }

    #[test]
    fn keyring_json_contains_no_plain_secrets() {
        let (vault, keyring, recovery) = new_vault();
        let json = keyring.to_json();
        assert!(!json.contains(&B64.encode(*vault.data_key)));
        assert!(!json.contains(&recovery.to_display().replace('-', "")));
        assert!(!json.contains("korrekt"));
    }

    #[test]
    fn keyring_with_excessive_params_is_rejected() {
        let (_, keyring, _) = new_vault();
        let mut value: serde_json::Value = serde_json::from_str(&keyring.to_json()).unwrap();
        value["kdf"]["m_kib"] = serde_json::json!(u32::MAX);
        assert!(matches!(
            Keyring::from_json(&value.to_string()),
            Err(Error::InvalidKdfParams(_))
        ));
    }

    #[test]
    fn keyring_with_unknown_version_or_garbage_is_rejected() {
        let (_, keyring, _) = new_vault();
        let mut value: serde_json::Value = serde_json::from_str(&keyring.to_json()).unwrap();
        value["version"] = serde_json::json!(2);
        assert!(matches!(Keyring::from_json(&value.to_string()), Err(Error::UnsupportedVersion)));
        assert!(matches!(Keyring::from_json("kein json"), Err(Error::InvalidKeyring)));
    }

    #[test]
    fn keyrings_are_sealed_and_verified() {
        let (vault, keyring, _) = new_vault();
        assert!(keyring.is_sealed() && vault.verify_keyring(&keyring));

        // Ein Passwortwechsel auf einem anderen Rechner ergibt einen Keyring, den dieser Rechner prüfen kann.
        let changed = vault.change_password(&keyring, "neues passwort", FAST).unwrap();
        assert!(vault.verify_keyring(&changed));
        let (rotated, _) = vault.rotate_recovery_key(&keyring).unwrap();
        assert!(vault.verify_keyring(&rotated));

        // Ein fremder Tresor, eine fehlende oder eine falsche Prüfsumme gelten nicht.
        let (other, other_keyring, _) = new_vault();
        assert!(!vault.verify_keyring(&other_keyring) && !other.verify_keyring(&keyring));
        let mut unsealed = keyring.clone();
        unsealed.mac = None;
        assert!(!vault.verify_keyring(&unsealed));
        assert!(vault.verify_keyring(&vault.seal_keyring(&unsealed)));
    }

    #[test]
    fn swapped_parts_of_a_sealed_keyring_are_detected() {
        // Der Server tauscht den Passwortteil gegen den eines eigenen Tresors mit bekanntem Passwort aus.
        let (_, keyring, recovery) = new_vault();
        let (_, attacker, _) = Vault::create("passwort des angreifers", FAST).unwrap();
        let mut forged = keyring.clone();
        forged.kdf = attacker.kdf.clone();
        forged.wrapped_by_password = attacker.wrapped_by_password.clone();
        assert!(matches!(Vault::unlock_with_password(&forged, "passwort des angreifers"), Err(Error::InvalidKeyring)));
        // Der Recovery-Teil ist echt, die Prüfsumme passt trotzdem nicht mehr.
        assert!(matches!(Vault::unlock_with_recovery(&forged, &recovery.to_display()), Err(Error::InvalidKeyring)));
    }

    #[test]
    fn keyrings_without_mac_still_work() {
        let (vault, keyring, recovery) = new_vault();
        let mut old: serde_json::Value = serde_json::from_str(&keyring.to_json()).unwrap();
        old.as_object_mut().unwrap().remove("mac");
        let old = Keyring::from_json(&old.to_string()).unwrap();
        assert!(!old.is_sealed());
        assert!(Vault::unlock_with_password(&old, "korrekt pferd batterie heftklammer").is_ok());
        assert!(Vault::unlock_with_recovery(&old, &recovery.to_display()).is_ok());
        assert!(!old.to_json().contains("mac"));
        assert!(vault.verify_keyring(&vault.seal_keyring(&old)));
    }

    #[test]
    fn epoch_is_covered_by_the_mac_and_survives_password_changes() {
        let (vault, keyring, recovery) = new_vault();
        assert_eq!(keyring.epoch(), None);

        let fresh = keyring.with_new_epoch().unwrap();
        assert!(!fresh.is_sealed());
        let sealed = vault.seal_keyring(&fresh);
        let epoch = sealed.epoch().unwrap().to_string();
        assert_eq!(epoch.len(), 32);
        assert_ne!(keyring.with_new_epoch().unwrap().epoch(), Some(epoch.as_str()));
        assert!(vault.verify_keyring(&sealed));
        let again = Keyring::from_json(&sealed.to_json()).unwrap();
        assert_eq!(again, sealed);
        assert!(Vault::unlock_with_recovery(&again, &recovery.to_display()).is_ok());

        // Der Server kann die Kennung nicht unbemerkt austauschen oder entfernen.
        let mut swapped = sealed.clone();
        swapped.epoch = Some("abc".into());
        assert!(!vault.verify_keyring(&swapped));
        let mut removed = sealed.clone();
        removed.epoch = None;
        assert!(!vault.verify_keyring(&removed));

        // Passwortwechsel und neuer Recovery-Key behalten die Kennung.
        let changed = vault.change_password(&sealed, "neues passwort", FAST).unwrap();
        assert_eq!(changed.epoch(), Some(epoch.as_str()));
        assert!(vault.verify_keyring(&changed));
        let (rotated, _) = vault.rotate_recovery_key(&sealed).unwrap();
        assert_eq!(rotated.epoch(), Some(epoch.as_str()));

        // Ungültige Kennungen werden abgelehnt.
        let mut value: serde_json::Value = serde_json::from_str(&sealed.to_json()).unwrap();
        value["epoch"] = serde_json::json!("nicht erlaubt/");
        assert!(matches!(Keyring::from_json(&value.to_string()), Err(Error::InvalidKeyring)));
        value["epoch"] = serde_json::json!("");
        assert!(matches!(Keyring::from_json(&value.to_string()), Err(Error::InvalidKeyring)));
    }

    #[test]
    fn standard_params_are_valid() {
        assert!(KdfParams::standard().validate().is_ok());
    }

    /// Dauert mit den Standardparametern spürbar. Manuell mit `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn standard_params_roundtrip_timing() {
        let start = std::time::Instant::now();
        let (_, keyring, _) = Vault::create("korrekt pferd batterie heftklammer", KdfParams::standard()).unwrap();
        println!("anlegen: {:?}", start.elapsed());
        let start = std::time::Instant::now();
        Vault::unlock_with_password(&keyring, "korrekt pferd batterie heftklammer").unwrap();
        println!("entsperren: {:?}", start.elapsed());
    }
}
