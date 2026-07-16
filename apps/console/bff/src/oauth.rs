use std::collections::BTreeSet;

use anyhow::Context;
use axum::{
    extract::{Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, HOST, PRAGMA},
    },
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::ScopeName;

use crate::{
    AppState,
    session::{
        AUTHORIZATION_AAD, ConsoleSession, PendingAuthorization, clear_authorization_cookie,
        clear_session_cookie, random_token, read_authorization, set_authorization_cookie,
        set_session_cookie,
    },
};

const MAX_CONSOLE_SESSION_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_ACCESS_TOKEN_SECONDS: u64 = 24 * 60 * 60;

pub(crate) async fn login(State(state): State<AppState>) -> Response {
    match begin_login(&state) {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "failed to begin console login");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn begin_login(state: &AppState) -> anyhow::Result<Response> {
    let oauth_state = random_value()?;
    let code_verifier = random_value()?;
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let pending = PendingAuthorization {
        state: oauth_state.clone(),
        code_verifier,
        expires_at: Utc::now().timestamp() + 600,
    };
    let encrypted = state.sessions.seal(&pending, AUTHORIZATION_AAD)?;
    let mut authorize = state.config.authorize_url();
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", state.config.oauth_client_id())
        .append_pair("scope", &state.config.oauth_scope())
        .append_pair("redirect_uri", state.config.callback_url().as_str())
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &oauth_state)
        .append_pair("resource", state.config.oauth_resource().as_str());
    let mut headers = no_store_headers();
    set_authorization_cookie(&mut headers, &encrypted, state.config.secure_cookie())?;
    Ok((headers, Redirect::to(authorize.as_str())).into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub(crate) async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if query.error.is_some() {
        return callback_error(&state, StatusCode::UNAUTHORIZED);
    }
    let Some(pending) = read_authorization(&headers, &state.sessions) else {
        return callback_error(&state, StatusCode::BAD_REQUEST);
    };
    let Some(code) = query.code.filter(|value| !value.is_empty()) else {
        return callback_error(&state, StatusCode::BAD_REQUEST);
    };
    let Some(returned_state) = query.state else {
        return callback_error(&state, StatusCode::BAD_REQUEST);
    };
    if pending.expires_at < Utc::now().timestamp() || pending.state != returned_state {
        return callback_error(&state, StatusCode::BAD_REQUEST);
    }

    let response = match state
        .http
        .post(state.config.token_url())
        .header(HOST, state.config.gateway_host())
        .form(&TokenRequest {
            grant_type: "authorization_code",
            client_id: state.config.oauth_client_id(),
            resource: state.config.oauth_resource().as_str(),
            code: &code,
            redirect_uri: state.config.callback_url().as_str(),
            code_verifier: &pending.code_verifier,
        })
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "gateway token exchange failed");
            return callback_error(&state, StatusCode::BAD_GATEWAY);
        }
    };
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "gateway rejected console token exchange");
        return callback_error(&state, StatusCode::UNAUTHORIZED);
    }
    let token = match response.json::<TokenResponse>().await {
        Ok(token) if token.token_type == "Bearer" && token.expires_in > 0 => token,
        Ok(_) => return callback_error(&state, StatusCode::BAD_GATEWAY),
        Err(error) => {
            tracing::error!(%error, "invalid gateway token response");
            return callback_error(&state, StatusCode::BAD_GATEWAY);
        }
    };
    let Some(refresh_token) = token.refresh_token else {
        return callback_error(&state, StatusCode::BAD_GATEWAY);
    };
    let Some(refresh_token_expires_in) = token.refresh_token_expires_in else {
        return callback_error(&state, StatusCode::BAD_GATEWAY);
    };
    let granted_scopes = match validated_granted_scopes(state.config.oauth_scopes(), &token.scope) {
        Ok(scopes) => scopes,
        Err(error) => {
            tracing::error!(%error, "gateway token omitted required console scopes");
            return callback_error(&state, StatusCode::BAD_GATEWAY);
        }
    };
    let expires_in = token.expires_in.min(MAX_ACCESS_TOKEN_SECONDS);
    let session_expires_in = refresh_token_expires_in.min(MAX_CONSOLE_SESSION_SECONDS);
    if session_expires_in == 0 {
        return callback_error(&state, StatusCode::BAD_GATEWAY);
    }
    let now = Utc::now().timestamp();
    let console_session = ConsoleSession {
        access_token: token.access_token,
        access_expires_at: now + i64::try_from(expires_in).unwrap_or(0),
        refresh_token,
        refresh_expires_at: now + i64::try_from(session_expires_in).unwrap_or(0),
        granted_scopes,
        csrf_token: match random_token() {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(%error, "failed to generate console CSRF token");
                return callback_error(&state, StatusCode::INTERNAL_SERVER_ERROR);
            }
        },
    };
    let encrypted = match state
        .sessions
        .seal(&console_session, crate::session::SESSION_AAD)
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to establish console session");
            return callback_error(&state, StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut response_headers = no_store_headers();
    clear_authorization_cookie(&mut response_headers, state.config.secure_cookie());
    if set_session_cookie(
        &mut response_headers,
        &encrypted,
        session_expires_in,
        state.config.secure_cookie(),
    )
    .is_err()
    {
        return callback_error(&state, StatusCode::INTERNAL_SERVER_ERROR);
    }
    (response_headers, Redirect::to("/console/")).into_response()
}

pub(crate) async fn logout(State(state): State<AppState>, request_headers: HeaderMap) -> Response {
    let Some(session) = crate::session::read_session(&request_headers, &state.sessions) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let revocation = state
        .http
        .post(state.config.revocation_url())
        .header(HOST, state.config.gateway_host())
        .form(&RevocationRequest {
            client_id: state.config.oauth_client_id(),
            token: &session.refresh_token,
            token_type_hint: "refresh_token",
            resource: state.config.oauth_resource().as_str(),
        })
        .send()
        .await;
    match revocation {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            tracing::error!(
                status = %response.status(),
                "gateway rejected console session revocation"
            );
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(error) => {
            tracing::error!(%error, "gateway session revocation failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    }
    let mut headers = no_store_headers();
    clear_session_cookie(&mut headers, state.config.secure_cookie());
    (headers, StatusCode::NO_CONTENT).into_response()
}

fn callback_error(state: &AppState, status: StatusCode) -> Response {
    let mut headers = no_store_headers();
    clear_authorization_cookie(&mut headers, state.config.secure_cookie());
    (status, headers, "console authentication failed").into_response()
}

fn random_value() -> anyhow::Result<String> {
    random_token()
}

fn no_store_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    grant_type: &'static str,
    client_id: &'a str,
    resource: &'a str,
    code: &'a str,
    redirect_uri: &'a str,
    code_verifier: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
}

#[derive(Serialize)]
struct RefreshTokenRequest<'a> {
    grant_type: &'static str,
    client_id: &'a str,
    refresh_token: &'a str,
}

#[derive(Serialize)]
struct RevocationRequest<'a> {
    client_id: &'a str,
    token: &'a str,
    token_type_hint: &'static str,
    resource: &'a str,
}

pub(crate) struct UpstreamSession {
    pub(crate) session: ConsoleSession,
    pub(crate) replacement_cookie: Option<(String, u64)>,
}

pub(crate) async fn upstream_session(
    state: &AppState,
    mut session: ConsoleSession,
) -> anyhow::Result<UpstreamSession> {
    let now = Utc::now().timestamp();
    if session.is_expired(now) {
        anyhow::bail!("console session expired");
    }
    if !state
        .config
        .oauth_scopes()
        .is_subset(&session.granted_scopes)
    {
        anyhow::bail!("console session lacks configured OAuth scopes");
    }
    if !session.should_refresh(now) {
        return Ok(UpstreamSession {
            session,
            replacement_cookie: None,
        });
    }

    let response = state
        .http
        .post(state.config.token_url())
        .header(HOST, state.config.gateway_host())
        .form(&RefreshTokenRequest {
            grant_type: "refresh_token",
            client_id: state.config.oauth_client_id(),
            refresh_token: &session.refresh_token,
        })
        .send()
        .await
        .context("gateway refresh request failed")?;
    if !response.status().is_success() {
        anyhow::bail!("gateway rejected console refresh");
    }
    let token: TokenResponse = response
        .json()
        .await
        .context("gateway returned an invalid refresh response")?;
    if token.token_type != "Bearer" || token.expires_in == 0 {
        anyhow::bail!("gateway returned an invalid bearer refresh response");
    }
    let refresh_token = token
        .refresh_token
        .ok_or_else(|| anyhow::anyhow!("gateway omitted rotated refresh token"))?;
    let refresh_expires_in = token
        .refresh_token_expires_in
        .ok_or_else(|| anyhow::anyhow!("gateway omitted refresh token lifetime"))?
        .min(MAX_CONSOLE_SESSION_SECONDS);
    if refresh_expires_in == 0 {
        anyhow::bail!("gateway returned an expired refresh token");
    }
    let access_expires_in = token.expires_in.min(MAX_ACCESS_TOKEN_SECONDS);
    let granted_scopes = validated_granted_scopes(state.config.oauth_scopes(), &token.scope)?;
    let now = Utc::now().timestamp();
    session.access_token = token.access_token;
    session.access_expires_at = now + i64::try_from(access_expires_in).unwrap_or(0);
    session.refresh_token = refresh_token;
    session.refresh_expires_at = now + i64::try_from(refresh_expires_in).unwrap_or(0);
    session.granted_scopes = granted_scopes;
    let encrypted = state
        .sessions
        .seal(&session, crate::session::SESSION_AAD)
        .context("encrypting rotated console session")?;
    Ok(UpstreamSession {
        session,
        replacement_cookie: Some((encrypted, refresh_expires_in)),
    })
}

fn validated_granted_scopes(
    required: &BTreeSet<ScopeName>,
    value: &str,
) -> anyhow::Result<BTreeSet<ScopeName>> {
    let scopes = value
        .split_ascii_whitespace()
        .map(ScopeName::new)
        .collect::<Result<BTreeSet<_>, _>>()
        .context("gateway token returned an invalid scope set")?;
    if !required.is_subset(&scopes) {
        anyhow::bail!("gateway token omitted required console scopes");
    }
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn granted_scopes_must_cover_the_console_configuration() {
        let required = ["operator:use", "view:read"]
            .into_iter()
            .map(|scope| ScopeName::new(scope).unwrap())
            .collect();
        let granted = validated_granted_scopes(&required, "view:read operator:use").unwrap();
        assert_eq!(granted, required);
        assert!(validated_granted_scopes(&required, "operator:use").is_err());
    }

    #[test]
    fn revocation_request_is_form_encoded_and_keeps_the_secret_out_of_the_url() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let request = reqwest::Client::new()
            .post("https://gateway.example/oauth/revoke")
            .form(&RevocationRequest {
                client_id: "admin-console",
                token: "secret-refresh-token",
                token_type_hint: "refresh_token",
                resource: "https://veoveo.example/mcp/admin",
            })
            .build()
            .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/oauth/revoke"
        );
        let body = request.body().and_then(reqwest::Body::as_bytes).unwrap();
        let fields = url::form_urlencoded::parse(body)
            .into_owned()
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            fields.get("client_id").map(String::as_str),
            Some("admin-console")
        );
        assert_eq!(
            fields.get("token").map(String::as_str),
            Some("secret-refresh-token")
        );
        assert_eq!(
            fields.get("token_type_hint").map(String::as_str),
            Some("refresh_token")
        );
        assert_eq!(
            fields.get("resource").map(String::as_str),
            Some("https://veoveo.example/mcp/admin")
        );
    }
}
