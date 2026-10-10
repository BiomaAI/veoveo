use super::{LoginArgs, run_loopback};
#[path = "test_support/mod.rs"]
mod test_support;
use std::{future::Future, sync::atomic::Ordering, time::Duration};
use test_support::{CLIENT, Context, Fixture, SCOPE, SECRET_MARKER, TOKEN};
use url::Url;

async fn case<F, Fut>(offline: bool, grant: &str, deny: bool, body: F)
where
    F: FnOnce(Context) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut fixture = Fixture::start(offline, grant, deny).await;
    let mut body = tokio::spawn(body(fixture.context.clone()));
    let outcome = tokio::time::timeout(Duration::from_secs(40), &mut body).await;
    if outcome.is_err() {
        body.abort();
        let _ = body.await;
    }
    fixture.close().await;
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => std::panic::resume_unwind(error.into_panic()),
        Err(_) => panic!("OAuth native control exceeded 40 seconds"),
    }
}

fn args(cx: &Context) -> LoginArgs {
    let callback = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = callback.local_addr().unwrap();
    LoginArgs {
        issuer: format!("{}/", cx.base),
        client_id: CLIENT.into(),
        resource: format!("{}/mcp", cx.base),
        redirect_uri: format!("http://{address}/callback"),
        scope: vec![SCOPE.parse().unwrap()],
        authorization_url_file: cx.directory.join("authorization-url"),
        token_file: cx.output(),
        timeout_seconds: 30,
    }
}

async fn authorization(args: &LoginArgs) -> Url {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            if let Ok(bytes) = std::fs::read_to_string(&args.authorization_url_file)
                && let Ok(url) = Url::parse(bytes.trim())
            {
                return url;
            }
        }
    })
    .await
    .expect("authorization URL was not published")
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

async fn exchange(cx: &Context, args: &LoginArgs) -> anyhow::Result<()> {
    let drive = async {
        let url = authorization(args).await;
        let callback = cx.admit_authorization(&url).await;
        let response = http().get(callback).send().await.unwrap();
        assert!(response.status().is_success() || response.status().is_redirection());
    };
    let (result, ()) = tokio::join!(run_loopback(args), drive);
    result
}

fn assert_no_token(args: &LoginArgs) {
    assert!(
        std::fs::read(&args.token_file)
            .unwrap_or_default()
            .is_empty()
    );
}

async fn reusable(args: &LoginArgs) {
    tokio::task::yield_now().await;
    let url = Url::parse(&args.redirect_uri).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", url.port().unwrap()))
        .await
        .expect("callback listener survived login settlement");
    drop(listener);
}

#[tokio::test]
async fn sdk_s256_preregistered_exchange_writes_only_private_token() {
    case(false, SCOPE, false, |cx| async move {
        use std::os::unix::fs::PermissionsExt;
        let args = args(&cx);
        exchange(&cx, &args).await.unwrap();
        let token = std::fs::read_to_string(&args.token_file).unwrap();
        assert_eq!(token.trim(), TOKEN);
        assert_eq!(
            std::fs::metadata(&args.token_file)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let url = std::fs::read_to_string(&args.authorization_url_file).unwrap();
        assert!(!url.contains(TOKEN));
        assert_eq!(cx.tokens.load(Ordering::SeqCst), 1);
        reusable(&args).await;
    })
    .await;
}

#[tokio::test]
async fn foreign_callback_state_issuer_and_path_cannot_settle() {
    for invalid in ["state", "iss", "path"] {
        case(false, SCOPE, false, move |cx| async move {
            let args = args(&cx);
            let drive = async {
                let mut callback = cx.admit_authorization(&authorization(&args).await).await;
                if invalid == "path" {
                    let valid = callback.clone();
                    callback.set_path("/foreign");
                    let response = http().get(callback).send().await.unwrap();
                    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
                    assert_eq!(cx.tokens.load(Ordering::SeqCst), 0);
                    assert_no_token(&args);
                    let _ = http().get(valid).send().await.unwrap();
                    return;
                } else {
                    let pairs: Vec<_> = callback
                        .query_pairs()
                        .map(|(key, value)| (key.into_owned(), value.into_owned()))
                        .collect();
                    callback.set_query(None);
                    for (key, value) in pairs {
                        callback
                            .query_pairs_mut()
                            .append_pair(&key, if key == invalid { "foreign" } else { &value });
                    }
                }
                let _ = http().get(callback).send().await;
            };
            let (result, ()) = tokio::join!(run_loopback(&args), drive);
            if invalid == "path" {
                result.unwrap();
                assert_eq!(cx.tokens.load(Ordering::SeqCst), 1);
            } else {
                assert!(result.is_err());
                assert_no_token(&args);
                assert_eq!(cx.tokens.load(Ordering::SeqCst), 0);
            }
            reusable(&args).await;
        })
        .await;
    }
}

#[tokio::test]
async fn sdk_implicit_offline_scope_and_granted_widening_are_rejected() {
    case(true, SCOPE, false, |cx| async move {
        let args = args(&cx);
        assert!(run_loopback(&args).await.is_err());
        assert!(
            std::fs::read(&args.authorization_url_file)
                .unwrap_or_default()
                .is_empty()
        );
        assert_no_token(&args);
        assert_eq!(cx.tokens.load(Ordering::SeqCst), 0);
        reusable(&args).await;
    })
    .await;
    for grant in [
        "knowledge:read offline_access",
        "knowledge:read admin:manage",
    ] {
        case(false, grant, false, |cx| async move {
            let args = args(&cx);
            assert!(exchange(&cx, &args).await.is_err());
            assert_no_token(&args);
            assert_eq!(cx.tokens.load(Ordering::SeqCst), 1);
            reusable(&args).await;
        })
        .await;
    }
}

#[tokio::test]
async fn remote_denial_does_not_disclose_provider_response() {
    case(false, SCOPE, true, |cx| async move {
        let args = args(&cx);
        let error = exchange(&cx, &args).await.unwrap_err();
        assert!(!format!("{error:#}").contains(SECRET_MARKER));
        assert!(!format!("{error:?}").contains(SECRET_MARKER));
        assert_no_token(&args);
        reusable(&args).await;
    })
    .await;
}

#[tokio::test]
async fn existing_output_and_symlink_are_never_overwritten() {
    for (symlink, authorization_output) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        case(false, SCOPE, false, move |cx| async move {
            let args = args(&cx);
            let output = if authorization_output {
                &args.authorization_url_file
            } else {
                &args.token_file
            };
            let retained = cx.directory.join("retained");
            std::fs::write(&retained, "retained-content").unwrap();
            if symlink {
                std::os::unix::fs::symlink(&retained, output).unwrap();
            } else {
                std::fs::write(output, "retained-content").unwrap();
            }
            assert!(run_loopback(&args).await.is_err());
            assert_eq!(
                std::fs::read_to_string(&retained).unwrap(),
                "retained-content"
            );
            assert_eq!(std::fs::read_to_string(output).unwrap(), "retained-content");
            assert_eq!(cx.tokens.load(Ordering::SeqCst), 0);
            reusable(&args).await;
        })
        .await;
    }
}

#[tokio::test]
async fn timeout_and_dropped_login_release_callback_listener() {
    case(false, SCOPE, false, |cx| async move {
        let args = args(&cx);
        let hold = async {
            authorization(&args).await;
            test_support::StalledConnection::open(&args.redirect_uri).await
        };
        let (result, connection) = tokio::join!(run_loopback(&args), hold);
        assert!(result.is_err());
        assert_no_token(&args);
        connection.assert_closed().await;
        reusable(&args).await;
    })
    .await;
    case(false, SCOPE, false, |cx| async move {
        let args = args(&cx);
        {
            let login = run_loopback(&args);
            tokio::pin!(login);
            tokio::select! {
                _ = authorization(&args) => {},
                _ = &mut login => panic!("login ended before callback cancellation control"),
            }
        }
        assert_no_token(&args);
        reusable(&args).await;
    })
    .await;
}

#[tokio::test]
async fn callback_route_patterns_are_rejected_before_owned_effects() {
    case(false, SCOPE, false, |cx| async move {
        for path in ["/callback/{id}", "/callback/*tail", "/callback/%2f"] {
            let mut args = args(&cx);
            let mut callback = Url::parse(&args.redirect_uri).unwrap();
            callback.set_path(path);
            args.redirect_uri = callback.to_string();
            assert!(run_loopback(&args).await.is_err());
            assert!(!args.authorization_url_file.exists());
            assert!(!args.token_file.exists());
            assert_eq!(cx.tokens.load(Ordering::SeqCst), 0);
            reusable(&args).await;
        }
    })
    .await;
}

#[tokio::test]
async fn sdk_debug_subscriber_cannot_observe_code_token_or_remote_denial() {
    use tracing::instrument::WithSubscriber;
    for deny in [false, true] {
        case(false, SCOPE, deny, move |cx| async move {
            let args = args(&cx);
            let capture = test_support::LogCapture::default();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .with_ansi(false)
                .without_time()
                .with_writer(capture.clone())
                .finish();
            let result = exchange(&cx, &args).with_subscriber(subscriber).await;
            assert_eq!(result.is_err(), deny);
            let logs = capture.text();
            for sensitive in [
                TOKEN,
                SECRET_MARKER,
                "fixture-code",
                test_support::REFRESH_TOKEN,
                test_support::VENDOR_SECRET,
            ] {
                assert!(
                    !logs.contains(sensitive),
                    "OAuth diagnostics disclosed a sensitive fixture value"
                );
            }
            assert_eq!(cx.tokens.load(Ordering::SeqCst), 1);
            reusable(&args).await;
        })
        .await;
    }
}

#[tokio::test]
async fn settlement_and_cancellation_close_accepted_stalled_callback_connections() {
    for deny in [false, true] {
        case(false, SCOPE, deny, move |cx| async move {
            let args = args(&cx);
            let drive = async {
                let url = authorization(&args).await;
                let connection = test_support::StalledConnection::open(&args.redirect_uri).await;
                let callback = cx.admit_authorization(&url).await;
                assert!(
                    http()
                        .get(callback)
                        .send()
                        .await
                        .unwrap()
                        .status()
                        .is_redirection()
                );
                connection
            };
            let (result, connection) = tokio::join!(run_loopback(&args), drive);
            assert_eq!(result.is_err(), deny);
            connection.assert_closed().await;
            reusable(&args).await;
        })
        .await;
    }
    case(false, SCOPE, false, |cx| async move {
        let args = args(&cx);
        let connection;
        {
            let login = run_loopback(&args);
            tokio::pin!(login);
            let hold = async {
                authorization(&args).await;
                test_support::StalledConnection::open(&args.redirect_uri).await
            };
            connection = tokio::select! {
                connection = hold => connection,
                _ = &mut login => panic!("login settled before accepted-connection cancellation"),
            };
        }
        connection.assert_closed().await;
        assert_no_token(&args);
        reusable(&args).await;
    })
    .await;
}
