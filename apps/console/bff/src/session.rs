use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, anyhow};
use axum::http::{
    HeaderMap, HeaderValue,
    header::{COOKIE, SET_COOKIE},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_mcp_contract::ScopeName;

const NONCE_BYTES: usize = 24;
const SESSION_COOKIE: &str = "veoveo_console";
const AUTHORIZATION_COOKIE: &str = "veoveo_console_authorization";
pub(crate) const SESSION_AAD: &[u8] = b"veoveo-console-session-v1";
pub(crate) const AUTHORIZATION_AAD: &[u8] = b"veoveo-console-authorization-v1";

#[derive(Clone)]
pub(crate) struct SessionCipher(Arc<XChaCha20Poly1305>);

impl SessionCipher {
    pub(crate) fn new(key: &[u8; 32]) -> anyhow::Result<Self> {
        Ok(Self(Arc::new(
            XChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow!("invalid session key"))?,
        )))
    }

    pub(crate) fn seal<T: Serialize>(&self, value: &T, aad: &[u8]) -> anyhow::Result<String> {
        let plaintext = serde_json::to_vec(value).context("serializing encrypted cookie")?;
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).context("generating cookie nonce")?;
        let nonce_value = XNonce::from(nonce);
        let ciphertext = self
            .0
            .encrypt(
                &nonce_value,
                Payload {
                    msg: &plaintext,
                    aad,
                },
            )
            .map_err(|_| anyhow!("encrypting cookie failed"))?;
        let mut encoded = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        encoded.extend_from_slice(&nonce);
        encoded.extend_from_slice(&ciphertext);
        Ok(URL_SAFE_NO_PAD.encode(encoded))
    }

    pub(crate) fn open<T: DeserializeOwned>(&self, encoded: &str, aad: &[u8]) -> anyhow::Result<T> {
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .context("cookie is not base64url")?;
        let (nonce_bytes, ciphertext) = bytes
            .split_at_checked(NONCE_BYTES)
            .ok_or_else(|| anyhow!("cookie is truncated"))?;
        let nonce: [u8; NONCE_BYTES] = nonce_bytes
            .try_into()
            .map_err(|_| anyhow!("cookie nonce is invalid"))?;
        let nonce = XNonce::from(nonce);
        let plaintext = self
            .0
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| anyhow!("cookie authentication failed"))?;
        serde_json::from_slice(&plaintext).context("decoding encrypted cookie")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingAuthorization {
    pub(crate) state: String,
    pub(crate) code_verifier: String,
    pub(crate) expires_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConsoleSession {
    pub(crate) access_token: String,
    pub(crate) access_expires_at: i64,
    pub(crate) refresh_token: String,
    pub(crate) refresh_expires_at: i64,
    pub(crate) granted_scopes: BTreeSet<ScopeName>,
    pub(crate) csrf_token: String,
}

impl ConsoleSession {
    pub(crate) fn is_expired(&self, now: i64) -> bool {
        self.refresh_expires_at <= now
    }

    pub(crate) fn should_refresh(&self, now: i64) -> bool {
        self.access_expires_at <= now.saturating_add(30)
    }
}

pub(crate) fn random_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("generating session token")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn read_session(headers: &HeaderMap, cipher: &SessionCipher) -> Option<ConsoleSession> {
    read_cookie(headers, SESSION_COOKIE).and_then(|value| cipher.open(value, SESSION_AAD).ok())
}

pub(crate) fn read_authorization(
    headers: &HeaderMap,
    cipher: &SessionCipher,
) -> Option<PendingAuthorization> {
    read_cookie(headers, AUTHORIZATION_COOKIE)
        .and_then(|value| cipher.open(value, AUTHORIZATION_AAD).ok())
}

pub(crate) fn set_session_cookie(
    headers: &mut HeaderMap,
    value: &str,
    max_age: u64,
    secure: bool,
) -> anyhow::Result<()> {
    append_cookie(headers, SESSION_COOKIE, value, max_age, secure)
}

pub(crate) fn set_authorization_cookie(
    headers: &mut HeaderMap,
    value: &str,
    secure: bool,
) -> anyhow::Result<()> {
    append_cookie(headers, AUTHORIZATION_COOKIE, value, 600, secure)
}

pub(crate) fn clear_session_cookie(headers: &mut HeaderMap, secure: bool) {
    clear_cookie(headers, SESSION_COOKIE, secure);
}

pub(crate) fn clear_authorization_cookie(headers: &mut HeaderMap, secure: bool) {
    clear_cookie(headers, AUTHORIZATION_COOKIE, secure);
}

fn read_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(candidate, value)| (candidate == name).then_some(value))
}

fn append_cookie(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
    max_age: u64,
    secure: bool,
) -> anyhow::Result<()> {
    let secure = if secure { "; Secure" } else { "" };
    let cookie =
        format!("{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}");
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).context("building Set-Cookie header")?,
    );
    Ok(())
}

fn clear_cookie(headers: &mut HeaderMap, name: &str, secure: bool) {
    let secure = if secure { "; Secure" } else { "" };
    let value = format!("{name}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{secure}");
    if let Ok(value) = HeaderValue::from_str(&value) {
        headers.append(SET_COOKIE, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_cookie_round_trips_and_rejects_tampering() {
        let cipher = SessionCipher::new(&[7_u8; 32]).unwrap();
        let value = ConsoleSession {
            access_token: "secret-token".to_owned(),
            access_expires_at: 42,
            refresh_token: "secret-refresh-token".to_owned(),
            refresh_expires_at: 84,
            granted_scopes: [ScopeName::new("operator:use").unwrap()]
                .into_iter()
                .collect(),
            csrf_token: "csrf-token".to_owned(),
        };
        let encoded = cipher.seal(&value, SESSION_AAD).unwrap();
        assert!(!encoded.contains("secret-token"));
        assert!(!encoded.contains("secret-refresh-token"));
        let decoded: ConsoleSession = cipher.open(&encoded, SESSION_AAD).unwrap();
        assert_eq!(decoded.access_token, "secret-token");
        assert_eq!(decoded.refresh_token, "secret-refresh-token");
        assert!(
            decoded
                .granted_scopes
                .contains(&ScopeName::new("operator:use").unwrap())
        );

        let mut bytes = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        let tampered = URL_SAFE_NO_PAD.encode(bytes);
        assert!(
            cipher
                .open::<ConsoleSession>(&tampered, SESSION_AAD)
                .is_err()
        );
    }

    #[test]
    fn session_refresh_and_expiry_boundaries_are_distinct() {
        let session = ConsoleSession {
            access_token: "access".to_owned(),
            access_expires_at: 100,
            refresh_token: "refresh".to_owned(),
            refresh_expires_at: 200,
            granted_scopes: [ScopeName::new("operator:use").unwrap()]
                .into_iter()
                .collect(),
            csrf_token: "csrf".to_owned(),
        };
        assert!(!session.should_refresh(69));
        assert!(session.should_refresh(70));
        assert!(!session.is_expired(199));
        assert!(session.is_expired(200));
    }
}
