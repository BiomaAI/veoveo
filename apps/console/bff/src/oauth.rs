use std::collections::BTreeSet;

use anyhow::Context;
use axum::{
    extract::{Query, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, HOST, PRAGMA},
    },
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use veoveo_mcp_contract::ScopeName;

use crate::{
    AppState,
    session::{
        AUTHORIZATION_AAD, ConsoleReturnPath, ConsoleSession, PendingAuthorization,
        clear_authorization_cookie, clear_session_cookie, random_token, read_authorization,
        set_authorization_cookie, set_session_cookie,
    },
};

const MAX_CONSOLE_SESSION_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_ACCESS_TOKEN_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LoginQuery {
    return_to: Option<String>,
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Response {
    match begin_login(&state, query.return_to.as_deref()) {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "failed to begin console login");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn begin_login(state: &AppState, return_to: Option<&str>) -> anyhow::Result<Response> {
    let oauth_state = random_value()?;
    let code_verifier = random_value()?;
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let pending = PendingAuthorization {
        state: oauth_state.clone(),
        code_verifier,
        expires_at: Utc::now().timestamp() + 600,
        return_path: ConsoleReturnPath::from_untrusted(return_to),
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
    let pending = read_authorization(&headers, &state.sessions);
    if let Some(error) = query.error.as_deref() {
        let failure = CallbackFailure::from_oauth_error(error);
        let return_path = valid_callback_return_path(
            pending.as_ref(),
            query.state.as_deref(),
            Utc::now().timestamp(),
        );
        return callback_error(&state, failure.status(), failure, return_path);
    }
    let Some(pending) = pending else {
        return callback_error(
            &state,
            StatusCode::BAD_REQUEST,
            CallbackFailure::SessionExpired,
            None,
        );
    };
    let Some(code) = query.code.filter(|value| !value.is_empty()) else {
        return callback_error(
            &state,
            StatusCode::BAD_REQUEST,
            CallbackFailure::InvalidResponse,
            Some(&pending.return_path),
        );
    };
    let Some(returned_state) = query.state else {
        return callback_error(
            &state,
            StatusCode::BAD_REQUEST,
            CallbackFailure::InvalidResponse,
            Some(&pending.return_path),
        );
    };
    if pending.expires_at < Utc::now().timestamp() || pending.state != returned_state {
        return callback_error(
            &state,
            StatusCode::BAD_REQUEST,
            CallbackFailure::SessionExpired,
            None,
        );
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
            return callback_error(
                &state,
                StatusCode::BAD_GATEWAY,
                CallbackFailure::ProviderUnavailable,
                Some(&pending.return_path),
            );
        }
    };
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "gateway rejected console token exchange");
        return callback_error(
            &state,
            StatusCode::UNAUTHORIZED,
            CallbackFailure::AuthenticationFailed,
            Some(&pending.return_path),
        );
    }
    let token = match response.json::<TokenResponse>().await {
        Ok(token) if token.token_type == "Bearer" && token.expires_in > 0 => token,
        Ok(_) => {
            return callback_error(
                &state,
                StatusCode::BAD_GATEWAY,
                CallbackFailure::ProviderUnavailable,
                Some(&pending.return_path),
            );
        }
        Err(error) => {
            tracing::error!(%error, "invalid gateway token response");
            return callback_error(
                &state,
                StatusCode::BAD_GATEWAY,
                CallbackFailure::ProviderUnavailable,
                Some(&pending.return_path),
            );
        }
    };
    let Some(refresh_token) = token.refresh_token else {
        return callback_error(
            &state,
            StatusCode::BAD_GATEWAY,
            CallbackFailure::ProviderUnavailable,
            Some(&pending.return_path),
        );
    };
    let Some(refresh_token_expires_in) = token.refresh_token_expires_in else {
        return callback_error(
            &state,
            StatusCode::BAD_GATEWAY,
            CallbackFailure::ProviderUnavailable,
            Some(&pending.return_path),
        );
    };
    let granted_scopes = match validated_granted_scopes(state.config.oauth_scopes(), &token.scope) {
        Ok(scopes) => scopes,
        Err(error) => {
            tracing::error!(%error, "gateway token omitted required console scopes");
            return callback_error(
                &state,
                StatusCode::BAD_GATEWAY,
                CallbackFailure::AuthenticationFailed,
                Some(&pending.return_path),
            );
        }
    };
    let expires_in = token.expires_in.min(MAX_ACCESS_TOKEN_SECONDS);
    let session_expires_in = refresh_token_expires_in.min(MAX_CONSOLE_SESSION_SECONDS);
    if session_expires_in == 0 {
        return callback_error(
            &state,
            StatusCode::BAD_GATEWAY,
            CallbackFailure::AuthenticationFailed,
            Some(&pending.return_path),
        );
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
                return callback_error(
                    &state,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    CallbackFailure::Internal,
                    Some(&pending.return_path),
                );
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
            return callback_error(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                CallbackFailure::Internal,
                Some(&pending.return_path),
            );
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
        return callback_error(
            &state,
            StatusCode::INTERNAL_SERVER_ERROR,
            CallbackFailure::Internal,
            Some(&pending.return_path),
        );
    }
    (response_headers, Redirect::to(pending.return_path.as_str())).into_response()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallbackFailure {
    AccessDenied,
    AuthorizationChanged,
    ProviderUnavailable,
    SessionExpired,
    InvalidResponse,
    AuthenticationFailed,
    Internal,
}

impl CallbackFailure {
    fn from_oauth_error(error: &str) -> Self {
        match error {
            "access_denied" => Self::AccessDenied,
            "invalid_scope" => Self::AuthorizationChanged,
            "server_error" | "temporarily_unavailable" => Self::ProviderUnavailable,
            _ => Self::InvalidResponse,
        }
    }

    const fn status(self) -> StatusCode {
        match self {
            Self::AccessDenied | Self::AuthenticationFailed => StatusCode::UNAUTHORIZED,
            Self::AuthorizationChanged | Self::ProviderUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::SessionExpired | Self::InvalidResponse => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::AccessDenied => "access_denied",
            Self::AuthorizationChanged => "authorization_changed",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::SessionExpired => "session_expired",
            Self::InvalidResponse => "invalid_response",
            Self::AuthenticationFailed => "authentication_failed",
            Self::Internal => "internal_error",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::AccessDenied => "Sign-in was cancelled",
            Self::AuthorizationChanged => "Console authorization changed",
            Self::ProviderUnavailable => "Sign-in service unavailable",
            Self::SessionExpired => "Sign-in session expired",
            Self::InvalidResponse => "Sign-in response was invalid",
            Self::AuthenticationFailed => "Sign-in could not be completed",
            Self::Internal => "Console could not create your session",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::AccessDenied => "No Console session was created. Retry when you are ready.",
            Self::AuthorizationChanged => {
                "The Console and the active authorization policy do not agree. This is an installation configuration problem, not an issue with your account. Retry after the installation operator resolves it."
            }
            Self::ProviderUnavailable => {
                "The identity service could not complete this request. Retry sign-in."
            }
            Self::SessionExpired => "Start a new sign-in request to continue to the Console.",
            Self::InvalidResponse => {
                "The Console could not validate the sign-in response. Start sign-in again."
            }
            Self::AuthenticationFailed => {
                "The Console could not establish an authorized session. Retry sign-in."
            }
            Self::Internal => {
                "The Console encountered an internal error while creating your session."
            }
        }
    }
}

fn valid_callback_return_path<'a>(
    pending: Option<&'a PendingAuthorization>,
    returned_state: Option<&str>,
    now: i64,
) -> Option<&'a ConsoleReturnPath> {
    let pending = pending?;
    let returned_state = returned_state?;
    (pending.expires_at >= now && pending.state == returned_state).then_some(&pending.return_path)
}

fn callback_error(
    state: &AppState,
    status: StatusCode,
    failure: CallbackFailure,
    return_path: Option<&ConsoleReturnPath>,
) -> Response {
    callback_error_response(state.config.secure_cookie(), status, failure, return_path)
}

fn callback_error_response(
    secure_cookie: bool,
    status: StatusCode,
    failure: CallbackFailure,
    return_path: Option<&ConsoleReturnPath>,
) -> Response {
    let reference = Uuid::now_v7().to_string();
    tracing::warn!(
        %reference,
        status = %status,
        reason = failure.code(),
        "console authentication callback failed"
    );
    let return_path = return_path
        .map(ConsoleReturnPath::as_str)
        .unwrap_or("/console/");
    let retry_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("return_to", return_path)
        .finish();
    let retry_href = format!("/auth/login?{retry_query}");
    let body = authentication_error_page(failure, &retry_href, &reference);
    let mut headers = no_store_headers();
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    clear_authorization_cookie(&mut headers, secure_cookie);
    (status, headers, Html(body)).into_response()
}

fn authentication_error_page(
    failure: CallbackFailure,
    retry_href: &str,
    reference: &str,
) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} | Console</title>
  <style>
    :root {{ color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }}
    body {{ min-height: 100vh; margin: 0; display: grid; place-items: center; background: #070b12; color: #edf3ff; }}
    main {{ width: min(34rem, calc(100vw - 3rem)); padding: 2.5rem; border: 1px solid #243044; border-radius: 1rem; background: #101722; box-shadow: 0 1.5rem 4rem #0008; }}
    p {{ color: #b7c3d8; line-height: 1.6; }}
    nav {{ display: flex; flex-wrap: wrap; gap: .75rem; margin: 2rem 0; }}
    a {{ padding: .75rem 1rem; border-radius: .5rem; color: #edf3ff; border: 1px solid #4d6485; text-decoration: none; }}
    a:first-child {{ background: #3478f6; border-color: #3478f6; }}
    small {{ color: #8290a7; }}
    code {{ user-select: all; }}
  </style>
</head>
<body>
  <main>
    <h1>{title}</h1>
    <p>{message}</p>
    <nav aria-label="Authentication recovery actions">
      <a href="{retry_href}">Retry sign-in</a>
      <a href="/console/">Return to Console</a>
    </nav>
    <small>If this continues, share reference <code>{reference}</code> with the installation operator.</small>
  </main>
</body>
</html>"#,
        title = failure.title(),
        message = failure.message(),
    )
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

    use axum::{
        body::to_bytes,
        http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, SET_COOKIE},
    };

    use super::*;

    #[test]
    fn oauth_errors_map_to_safe_user_categories() {
        assert_eq!(
            CallbackFailure::from_oauth_error("access_denied"),
            CallbackFailure::AccessDenied
        );
        assert_eq!(
            CallbackFailure::from_oauth_error("invalid_scope"),
            CallbackFailure::AuthorizationChanged
        );
        assert_eq!(
            CallbackFailure::from_oauth_error("server_error"),
            CallbackFailure::ProviderUnavailable
        );
        assert_eq!(
            CallbackFailure::from_oauth_error("temporarily_unavailable"),
            CallbackFailure::ProviderUnavailable
        );
        assert_eq!(
            CallbackFailure::from_oauth_error("provider-private-detail"),
            CallbackFailure::InvalidResponse
        );
    }

    #[test]
    fn callback_return_path_requires_current_matching_state() {
        let pending = PendingAuthorization {
            state: "expected-state".to_owned(),
            code_verifier: "verifier".to_owned(),
            expires_at: 200,
            return_path: ConsoleReturnPath::from_untrusted(Some("/console/#/apps/map/live.html")),
        };
        assert_eq!(
            valid_callback_return_path(Some(&pending), Some("expected-state"), 100)
                .map(ConsoleReturnPath::as_str),
            Some("/console/#/apps/map/live.html")
        );
        assert!(valid_callback_return_path(Some(&pending), Some("wrong-state"), 100).is_none());
        assert!(valid_callback_return_path(Some(&pending), Some("expected-state"), 201).is_none());
        assert!(valid_callback_return_path(None, Some("expected-state"), 100).is_none());
    }

    #[tokio::test]
    async fn callback_error_is_recoverable_private_and_no_store() {
        let return_path = ConsoleReturnPath::from_untrusted(Some("/console/#/apps/map/live.html"));
        let response = callback_error_response(
            true,
            StatusCode::BAD_GATEWAY,
            CallbackFailure::ProviderUnavailable,
            Some(&return_path),
        );
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        assert_eq!(response.headers().get(PRAGMA).unwrap(), "no-cache");
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert!(
            response
                .headers()
                .get(CONTENT_SECURITY_POLICY)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("default-src 'none'")
        );
        let cleared_cookie = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap())
            .find(|value| value.starts_with("veoveo_console_authorization="))
            .unwrap();
        assert!(cleared_cookie.contains("Max-Age=0"));
        assert!(cleared_cookie.contains("Secure"));

        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Sign-in service unavailable"));
        assert!(body.contains("Retry sign-in"));
        assert!(body.contains("Return to Console"));
        assert!(body.contains("/auth/login?return_to=%2Fconsole%2F%23%2Fapps%2Fmap%2Flive.html"));
        assert!(body.contains("share reference"));
        assert!(!body.contains("identity provider token exchange failed"));
        assert!(!body.contains("provider-private-detail"));
    }

    #[tokio::test]
    async fn authorization_change_error_explains_operator_recovery() {
        let response = callback_error_response(
            true,
            CallbackFailure::AuthorizationChanged.status(),
            CallbackFailure::AuthorizationChanged,
            None,
        );
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Console authorization changed"));
        assert!(body.contains("installation configuration problem"));
        assert!(body.contains("Retry sign-in"));
        assert!(body.contains("Return to Console"));
        assert!(!body.contains("invalid_scope"));
    }

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
