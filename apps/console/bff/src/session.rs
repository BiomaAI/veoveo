use std::{collections::BTreeSet, sync::Arc};

use crate::browser::BrowserApp;
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
use url::Url;
use veoveo_mcp_contract::ScopeName;

const NONCE_BYTES: usize = 24;
const MAX_BROWSER_RETURN_PATH_BYTES: usize = 4096;
const BROWSER_RETURN_ORIGIN: &str = "https://console.invalid/";
const CONSOLE_ROOT: &str = "/console/";
const BROWSER_RETURN_ROOTS: [&str; 3] = [CONSOLE_ROOT, "/apps/", "/workspace/"];
#[cfg(test)]
pub(crate) const SESSION_COOKIE: &str = "veoveo_console";
pub(crate) const SESSION_AAD: &[u8] = b"veoveo-console-session-v1";
pub(crate) const AUTHORIZATION_AAD: &[u8] = b"veoveo-console-authorization-v2";

#[derive(Clone)]
pub(crate) struct SessionCipher {
    cipher: Arc<XChaCha20Poly1305>,
    app: BrowserApp,
}

impl SessionCipher {
    pub(crate) fn new(key: &[u8; 32]) -> anyhow::Result<Self> {
        Ok(Self {
            cipher: Arc::new(
                XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| anyhow!("invalid session key"))?,
            ),
            app: BrowserApp::Console,
        })
    }

    pub(crate) fn for_app(&self, app: BrowserApp) -> Self {
        Self {
            cipher: self.cipher.clone(),
            app,
        }
    }

    fn associated_data<'a>(&self, aad: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        match self.app {
            BrowserApp::Console => std::borrow::Cow::Borrowed(aad),
            BrowserApp::Workspace => {
                std::borrow::Cow::Owned([b"veoveo-workspace:".as_slice(), aad].concat())
            }
        }
    }

    pub(crate) fn seal<T: Serialize>(&self, value: &T, aad: &[u8]) -> anyhow::Result<String> {
        let aad = self.associated_data(aad);
        let plaintext = serde_json::to_vec(value).context("serializing encrypted cookie")?;
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).context("generating cookie nonce")?;
        let nonce_value = XNonce::from(nonce);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce_value,
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow!("encrypting cookie failed"))?;
        let mut encoded = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        encoded.extend_from_slice(&nonce);
        encoded.extend_from_slice(&ciphertext);
        Ok(URL_SAFE_NO_PAD.encode(encoded))
    }

    pub(crate) fn open<T: DeserializeOwned>(&self, encoded: &str, aad: &[u8]) -> anyhow::Result<T> {
        let aad = self.associated_data(aad);
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
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: &aad,
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
    pub(crate) return_path: BrowserReturnPath,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct BrowserReturnPath(String);

impl BrowserReturnPath {
    pub(crate) fn for_app(candidate: Option<&str>, app: BrowserApp) -> Self {
        let value = Self::from_untrusted(candidate);
        let allowed = value.as_str().starts_with(app.root())
            || (app == BrowserApp::Console && value.as_str().starts_with("/apps/"));
        if allowed {
            value
        } else {
            Self(app.root().to_owned())
        }
    }
    pub(crate) fn from_untrusted(candidate: Option<&str>) -> Self {
        let Some(candidate) = candidate
            .filter(|value| !value.is_empty() && value.len() <= MAX_BROWSER_RETURN_PATH_BYTES)
        else {
            return Self::root();
        };
        let Ok(base) = Url::parse(BROWSER_RETURN_ORIGIN) else {
            return Self::root();
        };
        let Ok(parsed) = base.join(candidate) else {
            return Self::root();
        };
        if parsed.origin() != base.origin()
            || !BROWSER_RETURN_ROOTS
                .iter()
                .any(|root| parsed.path().starts_with(root))
        {
            return Self::root();
        }

        let mut return_path = parsed.path().to_owned();
        if let Some(query) = parsed.query() {
            return_path.push('?');
            return_path.push_str(query);
        }
        if let Some(fragment) = parsed.fragment() {
            return_path.push('#');
            return_path.push_str(fragment);
        }
        Self(return_path)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn root() -> Self {
        Self(CONSOLE_ROOT.to_owned())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserSession {
    pub(crate) access_token: String,
    pub(crate) access_expires_at: i64,
    pub(crate) refresh_token: String,
    pub(crate) refresh_expires_at: i64,
    pub(crate) granted_scopes: BTreeSet<ScopeName>,
    pub(crate) csrf_token: String,
}

impl BrowserSession {
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

pub(crate) fn read_session(headers: &HeaderMap, cipher: &SessionCipher) -> Option<BrowserSession> {
    read_cookie(headers, cipher.app.session_cookie())
        .and_then(|value| cipher.open(value, SESSION_AAD).ok())
}

pub(crate) fn read_authorization(
    headers: &HeaderMap,
    cipher: &SessionCipher,
) -> Option<PendingAuthorization> {
    read_cookie(headers, cipher.app.authorization_cookie())
        .and_then(|value| cipher.open(value, AUTHORIZATION_AAD).ok())
}

pub(crate) fn set_session_cookie(
    headers: &mut HeaderMap,
    value: &str,
    max_age: u64,
    secure: bool,
    app: BrowserApp,
) -> anyhow::Result<()> {
    append_cookie(headers, app.session_cookie(), value, max_age, secure)
}

pub(crate) fn set_authorization_cookie(
    headers: &mut HeaderMap,
    value: &str,
    secure: bool,
    app: BrowserApp,
) -> anyhow::Result<()> {
    append_cookie(headers, app.authorization_cookie(), value, 600, secure)
}

pub(crate) fn clear_session_cookie(headers: &mut HeaderMap, secure: bool, app: BrowserApp) {
    clear_cookie(headers, app.session_cookie(), secure);
}

pub(crate) fn clear_authorization_cookie(headers: &mut HeaderMap, secure: bool, app: BrowserApp) {
    clear_cookie(headers, app.authorization_cookie(), secure);
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
    fn applications_have_distinct_cookie_names_and_authenticated_encryption_domains() {
        let console = SessionCipher::new(&[9_u8; 32]).unwrap();
        let workspace = console.for_app(BrowserApp::Workspace);
        let secret = "fixture-session";
        let a = console.seal(&secret, SESSION_AAD).unwrap();
        let b = workspace.seal(&secret, SESSION_AAD).unwrap();
        assert!(console.open::<String>(&b, SESSION_AAD).is_err());
        assert!(workspace.open::<String>(&a, SESSION_AAD).is_err());
        assert_eq!(workspace.open::<String>(&b, SESSION_AAD).unwrap(), secret);
        assert_ne!(console.app.session_cookie(), workspace.app.session_cookie());
        assert_ne!(
            console.app.authorization_cookie(),
            workspace.app.authorization_cookie()
        );
        assert_eq!(
            BrowserReturnPath::for_app(Some("/console/"), BrowserApp::Workspace).as_str(),
            "/workspace/"
        );
        assert_eq!(
            BrowserReturnPath::for_app(Some("/workspace/?chat=one"), BrowserApp::Workspace)
                .as_str(),
            "/workspace/?chat=one"
        );
    }

    #[test]
    fn encrypted_cookie_round_trips_and_rejects_tampering() {
        let cipher = SessionCipher::new(&[7_u8; 32]).unwrap();
        let value = BrowserSession {
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
        let decoded: BrowserSession = cipher.open(&encoded, SESSION_AAD).unwrap();
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
                .open::<BrowserSession>(&tampered, SESSION_AAD)
                .is_err()
        );
    }

    #[test]
    fn session_refresh_and_expiry_boundaries_are_distinct() {
        let session = BrowserSession {
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

    #[test]
    fn console_return_path_preserves_local_routes_and_rejects_redirects() {
        assert_eq!(
            BrowserReturnPath::from_untrusted(Some("/console/#/apps/uav-sim/live")).as_str(),
            "/console/#/apps/uav-sim/live"
        );
        assert_eq!(
            BrowserReturnPath::from_untrusted(Some("/console/?theme=dark#/recordings/019fa8b0"))
                .as_str(),
            "/console/?theme=dark#/recordings/019fa8b0"
        );
        for rejected in [
            "https://attacker.example/console/",
            "//attacker.example/console/",
            "/admin",
            "/console/../../admin",
        ] {
            assert_eq!(
                BrowserReturnPath::from_untrusted(Some(rejected)).as_str(),
                CONSOLE_ROOT
            );
        }
        let oversized = format!("/console/#/{}", "a".repeat(MAX_BROWSER_RETURN_PATH_BYTES));
        assert_eq!(
            BrowserReturnPath::from_untrusted(Some(&oversized)).as_str(),
            CONSOLE_ROOT
        );
    }

    #[test]
    fn browser_return_path_preserves_standalone_apps_without_open_redirects() {
        assert_eq!(
            BrowserReturnPath::from_untrusted(Some("/apps/uav-sim/live.html")).as_str(),
            "/apps/uav-sim/live.html"
        );
        for rejected in [
            "https://attacker.example/apps/map/admin.html",
            "//attacker.example/apps/map/admin.html",
            "/appswitch/map/admin.html",
            "/apps/../../admin",
        ] {
            assert_eq!(
                BrowserReturnPath::from_untrusted(Some(rejected)).as_str(),
                CONSOLE_ROOT
            );
        }
    }
}
