use std::collections::BTreeSet;

use anyhow::anyhow;
use axum::{
    Json,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, LOCATION, PRAGMA},
    },
    response::IntoResponse,
};
use base64::{
    Engine as _,
    engine::general_purpose::{
        STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD,
    },
};
use jsonwebtoken::{Algorithm, jwk::JwkSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use veoveo_mcp_contract::{
    JwksSource, OAuthAuthorizationCode, OAuthRedirectUri, OAuthStateValue, OidcClientAuthMethod,
    OidcNonce, PkceCodeChallenge, PkceCodeVerifier, ScopeName,
};
use veoveo_mcp_gateway::ResolvedSecretString;

#[derive(Debug, Serialize)]
pub(super) struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct OidcTokenResponse {
    pub id_token: String,
}

pub(super) struct OidcTokenExchangeRequest {
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: ResolvedSecretString,
    pub auth_method: OidcClientAuthMethod,
    pub redirect_uri: String,
    pub code_verifier: String,
}

#[derive(Debug, Serialize)]
struct OAuthErrorResponse {
    error: &'static str,
    error_description: &'static str,
}

pub(super) fn scope_string(scopes: &BTreeSet<ScopeName>) -> String {
    scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn random_token_value() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(super) fn random_oauth_state() -> anyhow::Result<OAuthStateValue> {
    Ok(OAuthStateValue::new(random_token_value())?)
}

pub(super) fn random_authorization_code() -> anyhow::Result<OAuthAuthorizationCode> {
    Ok(OAuthAuthorizationCode::new(random_token_value())?)
}

pub(super) fn random_pkce_verifier() -> anyhow::Result<PkceCodeVerifier> {
    Ok(PkceCodeVerifier::new(random_token_value())?)
}

pub(super) fn random_oidc_nonce() -> anyhow::Result<OidcNonce> {
    Ok(OidcNonce::new(random_token_value())?)
}

pub(super) fn pkce_s256_challenge(
    verifier: &PkceCodeVerifier,
) -> anyhow::Result<PkceCodeChallenge> {
    let digest = Sha256::digest(verifier.as_str().as_bytes());
    Ok(PkceCodeChallenge::new(
        BASE64_URL_SAFE_NO_PAD.encode(digest),
    )?)
}

pub(super) async fn exchange_oidc_authorization_code(
    http: &reqwest::Client,
    exchange: OidcTokenExchangeRequest,
    idp_code: String,
) -> anyhow::Result<OidcTokenResponse> {
    let mut request = http.post(&exchange.token_endpoint);
    let form_body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code")
            .append_pair("code", &idp_code)
            .append_pair("redirect_uri", &exchange.redirect_uri)
            .append_pair("client_id", &exchange.client_id)
            .append_pair("code_verifier", &exchange.code_verifier);
        match exchange.auth_method {
            OidcClientAuthMethod::ClientSecretPost => {
                form.append_pair("client_secret", exchange.client_secret.expose_secret());
            }
            OidcClientAuthMethod::ClientSecretBasic => {
                let credentials = BASE64_STANDARD.encode(format!(
                    "{}:{}",
                    exchange.client_id,
                    exchange.client_secret.expose_secret()
                ));
                request = request.header(AUTHORIZATION, format!("Basic {credentials}"));
            }
        }
        form.finish()
    };
    let response = request
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!(
            "identity provider token endpoint returned {status}"
        ));
    }
    Ok(response.json::<OidcTokenResponse>().await?)
}

pub(super) fn redirect_response(location: &str) -> axum::response::Response {
    let Ok(location) = HeaderValue::from_str(location) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut headers = HeaderMap::new();
    headers.insert(LOCATION, location);
    (StatusCode::FOUND, headers).into_response()
}

pub(super) fn redirect_with_authorization_code(
    redirect_uri: &OAuthRedirectUri,
    code: &OAuthAuthorizationCode,
    state: Option<&OAuthStateValue>,
) -> axum::response::Response {
    let mut url = match Url::parse(redirect_uri.as_str()) {
        Ok(url) => url,
        Err(err) => return internal_error_response(err),
    };
    url.query_pairs_mut().append_pair("code", code.as_str());
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state.as_str());
    }
    redirect_response(url.as_str())
}

pub(super) fn redirect_with_oauth_error(
    redirect_uri: &OAuthRedirectUri,
    error: &str,
    error_description: Option<&str>,
    state: Option<&OAuthStateValue>,
) -> axum::response::Response {
    let mut url = match Url::parse(redirect_uri.as_str()) {
        Ok(url) => url,
        Err(err) => return internal_error_response(err),
    };
    url.query_pairs_mut().append_pair("error", error);
    if let Some(error_description) = error_description {
        url.query_pairs_mut()
            .append_pair("error_description", error_description);
    }
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state.as_str());
    }
    redirect_response(url.as_str())
}

pub(super) fn token_response(response: TokenResponse) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, Json(response)).into_response()
}

pub(super) fn oauth_error_response(
    status: StatusCode,
    error: &'static str,
    error_description: &'static str,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    (
        status,
        headers,
        Json(OAuthErrorResponse {
            error,
            error_description,
        }),
    )
        .into_response()
}

pub(super) async fn load_jwks(http: &reqwest::Client, jwks: &JwksSource) -> anyhow::Result<JwkSet> {
    match jwks {
        JwksSource::Remote { jwks_uri } => fetch_jwks(http, jwks_uri.as_str()).await,
        JwksSource::File { path } => {
            let bytes = std::fs::read(path.as_str())?;
            Ok(serde_json::from_slice::<JwkSet>(&bytes)?)
        }
    }
}

async fn fetch_jwks(http: &reqwest::Client, url: &str) -> anyhow::Result<JwkSet> {
    let response = http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<JwkSet>().await?)
}

pub(super) fn allowed_gateway_jwt_algorithms() -> Vec<Algorithm> {
    vec![
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ]
}

fn internal_error_response(err: impl std::fmt::Display) -> axum::response::Response {
    tracing::error!("gateway internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
