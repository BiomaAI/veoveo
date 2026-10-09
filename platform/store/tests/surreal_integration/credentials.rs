//! Password replacement and session observations on the disposable pinned server.
use super::fixture::TestDb;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use std::time::Duration;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

const DEFINE: &str = include_str!("../queries/surreal_integration/credentials/define_root.surql");
const ALTER: &str = include_str!("../queries/surreal_integration/credentials/alter_root.surql");
const AUTHORITY: &str =
    include_str!("../queries/surreal_integration/credentials/root_authority.surql");

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum SessionObservation {
    #[vocabulary(rename = "authorized")]
    Authorized,
    #[vocabulary(rename = "authentication_denied")]
    AuthenticationDenied,
}

fn observe(result: Result<(), surrealdb::Error>) -> SessionObservation {
    match result {
        Ok(()) => SessionObservation::Authorized,
        Err(error)
            if matches!(
                error.not_allowed_details(),
                Some(surrealdb::types::NotAllowedError::Auth(_))
            ) =>
        {
            SessionObservation::AuthenticationDenied
        }
        Err(_) => panic!("credential observation failed outside authentication"),
    }
}

async fn connect(endpoint: &str) -> Surreal<Client> {
    let address = endpoint
        .strip_prefix("ws://")
        .expect("fixture requires its declared WebSocket profile");
    Surreal::new::<Ws>(address)
        .await
        .unwrap_or_else(|_| panic!("fixture WebSocket connection failed"))
}

async fn privileged(client: &Surreal<Client>) -> SessionObservation {
    observe(
        client
            .query(AUTHORITY)
            .await
            .and_then(|response| response.check())
            .map(|_| ()),
    )
}

async fn replace(
    client: &Surreal<Client>,
    statement: &str,
    username: &str,
    password: &SecretString,
) {
    // Native DEFINE/ALTER PASSWORD grammar requires a strand, not a value parameter.
    // Match the existing production administration adapter's JSON strand escaping.
    let literal =
        serde_json::to_string(password.expose_secret()).expect("secret strand encoding failed");
    let statement = statement.replace("__PASSWORD__", &literal);
    let result = client
        .query(statement)
        .bind(("username", username.to_owned()))
        .await
        .and_then(|response| response.check());
    assert!(result.is_ok(), "fixture root password replacement failed");
}

fn credentials(username: &str, password: &SecretString) -> Root {
    Root {
        username: username.to_owned(),
        password: password.expose_secret().to_owned(),
    }
}

#[tokio::test]
async fn root_password_replacement_observes_existing_sessions_and_recovers_data() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = TestDb::new().await;
        let admin = db.admin().await;
        let endpoint = admin.config().endpoint().as_str();
        let anonymous = connect(endpoint).await;
        assert!(
            matches!(
                privileged(&anonymous).await,
                SessionObservation::AuthenticationDenied
            ),
            "root observation query accepted an anonymous connection"
        );
        let recovery_password: SecretString = uuid::Uuid::now_v7().to_string().into();
        replace(
            admin.client(),
            DEFINE,
            "rotation_recovery",
            &recovery_password,
        )
        .await;
        let recovery = connect(endpoint).await;
        assert!(
            recovery
                .signin(credentials("rotation_recovery", &recovery_password))
                .await
                .is_ok(),
            "alternate root login failed"
        );
        assert!(
            matches!(privileged(&recovery).await, SessionObservation::Authorized),
            "alternate root authority failed before mutation"
        );
        let marker = uuid::Uuid::now_v7().to_string();
        assert!(
            db.a.client()
                .query(include_str!(
                    "../queries/surreal_integration/credentials/create_marker.surql"
                ))
                .bind(("value", marker.clone()))
                .await
                .and_then(|r| r.check())
                .is_ok(),
            "fixture data setup failed"
        );
        for (name, statement) in [("alter_user", ALTER), ("define_user_overwrite", DEFINE)] {
            let old: SecretString = format!("{}'\\old", uuid::Uuid::now_v7()).into();
            let new: SecretString = format!("{}'\\new", uuid::Uuid::now_v7()).into();
            replace(&recovery, DEFINE, "rotation_original", &old).await;
            let existing = connect(endpoint).await;
            let token = existing
                .signin(credentials("rotation_original", &old))
                .await
                .unwrap_or_else(|_| panic!("original root login failed"));
            assert!(
                matches!(privileged(&existing).await, SessionObservation::Authorized),
                "original root authority failed"
            );
            replace(&recovery, statement, "rotation_original", &new).await;
            let old_login = connect(endpoint).await;
            assert!(
                matches!(
                    observe(
                        old_login
                            .signin(credentials("rotation_original", &old))
                            .await
                            .map(|_| ())
                    ),
                    SessionObservation::AuthenticationDenied
                ),
                "old fresh root login accepted"
            );
            let new_login = connect(endpoint).await;
            assert!(
                new_login
                    .signin(credentials("rotation_original", &new))
                    .await
                    .is_ok(),
                "new root login failed"
            );
            assert!(
                matches!(privileged(&new_login).await, SessionObservation::Authorized),
                "new root authority failed"
            );
            let existing_session = privileged(&existing).await;
            let bearer = connect(endpoint).await;
            let old_bearer = match observe(
                bearer
                    .authenticate((token.access.clone(), None))
                    .await
                    .map(|_| ()),
            ) {
                SessionObservation::Authorized => privileged(&bearer).await,
                denied => denied,
            };
            assert!(
                existing.invalidate().await.is_ok(),
                "owned session invalidation failed"
            );
            assert!(
                matches!(
                    privileged(&existing).await,
                    SessionObservation::AuthenticationDenied
                ),
                "owned session remains authorized after invalidate"
            );
            let replay = connect(endpoint).await;
            let bearer_after_invalidate =
                match observe(replay.authenticate(token).await.map(|_| ())) {
                    SessionObservation::Authorized => privileged(&replay).await,
                    denied => denied,
                };
            // Recovery uses the independent, pre-proven root account, not an old session.
            replace(&recovery, statement, "rotation_original", &old).await;
            let restored = connect(endpoint).await;
            assert!(
                restored
                    .signin(credentials("rotation_original", &old))
                    .await
                    .is_ok(),
                "root recovery login failed"
            );
            assert!(
                matches!(privileged(&restored).await, SessionObservation::Authorized),
                "root recovery authority failed"
            );
            let mut result =
                db.a.client()
                    .query(include_str!(
                        "../queries/surreal_integration/credentials/read_marker.surql"
                    ))
                    .await
                    .unwrap_or_else(|_| panic!("retained fixture data query failed"))
                    .check()
                    .unwrap_or_else(|_| panic!("retained fixture data query rejected"));
            let retained: Option<String> = result
                .take(0)
                .unwrap_or_else(|_| panic!("retained marker decoding failed"));
            assert_eq!(
                retained.as_deref(),
                Some(marker.as_str()),
                "fixture data changed during root recovery"
            );
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Observation {
                method: &'static str,
                existing_session: SessionObservation,
                old_bearer: SessionObservation,
                bearer_after_invalidate: SessionObservation,
                recovery_data_intact: bool,
            }
            println!(
                "{}",
                serde_json::to_string(&Observation {
                    method: name,
                    existing_session,
                    old_bearer,
                    bearer_after_invalidate,
                    recovery_data_intact: true
                })
                .expect("safe observation encoding failed")
            );
        }
    })
    .await
    .expect("root credential qualification exceeded180seconds");
}

async fn bootstrap_step<T>(
    stage: &'static str,
    future: impl std::future::IntoFuture<Output = T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(15), future.into_future())
        .await
        .unwrap_or_else(|_| panic!("bootstrap credential {stage} exceeded 15 seconds"))
}

async fn ready_bootstrap(endpoint: &str) -> Surreal<Client> {
    let address = endpoint
        .strip_prefix("ws://")
        .expect("owned WebSocket endpoint");
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(Ok(client)) =
                tokio::time::timeout(Duration::from_secs(5), Surreal::new::<Ws>(address)).await
            {
                return client;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("bootstrap server readiness exceeded 30 seconds")
}

async fn bootstrap_marker(client: &Surreal<Client>, marker: &str) {
    let result = bootstrap_step(
        "marker namespace",
        client.use_ns("bootstrap_rotation").use_db("retained"),
    )
    .await;
    assert!(
        result.is_ok(),
        "bootstrap marker namespace selection failed"
    );
    let mut result = bootstrap_step(
        "marker read",
        client.query(include_str!(
            "../queries/surreal_integration/credentials/read_marker.surql"
        )),
    )
    .await
    .and_then(|response| response.check())
    .unwrap_or_else(|_| panic!("bootstrap retained marker read failed"));
    let retained: Option<String> = result
        .take(0)
        .unwrap_or_else(|_| panic!("bootstrap marker decoding failed"));
    assert_eq!(
        retained.as_deref(),
        Some(marker),
        "bootstrap retained marker differs"
    );
}

#[tokio::test]
async fn bootstrap_root_replacement_survives_persistent_server_restart() {
    tokio::time::timeout(Duration::from_secs(240), async {
        let original: SecretString = uuid::Uuid::now_v7().to_string().into();
        let replacement: SecretString = uuid::Uuid::now_v7().to_string().into();
        let alternate: SecretString = uuid::Uuid::now_v7().to_string().into();
        let (container, endpoint) = super::fixture::Container::start(
            super::fixture::Docker::default(),
            "rocksdb:/tmp/veoveo-test.db",
            original.expose_secret(),
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));
        let startup = ready_bootstrap(&endpoint).await;
        let old_token = bootstrap_step(
            "initial login",
            startup.signin(credentials("fixture_admin", &original)),
        )
        .await
        .unwrap_or_else(|_| panic!("bootstrap ROOT login failed"));
        assert!(
            bootstrap_step("initial authority", privileged(&startup)).await
                == SessionObservation::Authorized,
            "bootstrap ROOT lacks authority"
        );
        bootstrap_step(
            "alternate setup",
            replace(&startup, DEFINE, "bootstrap_recovery", &alternate),
        )
        .await;
        let recovery = bootstrap_step("alternate connection", connect(&endpoint)).await;
        assert!(
            bootstrap_step(
                "alternate login",
                recovery.signin(credentials("bootstrap_recovery", &alternate))
            )
            .await
            .is_ok(),
            "alternate ROOT login failed"
        );
        assert!(
            bootstrap_step("alternate authority", privileged(&recovery)).await
                == SessionObservation::Authorized,
            "alternate ROOT authority must precede replacement"
        );
        let marker = uuid::Uuid::now_v7().to_string();
        assert!(
            bootstrap_step(
                "marker setup",
                recovery
                    .query(include_str!(
                        "../queries/surreal_integration/credentials/setup_bootstrap_marker.surql"
                    ))
                    .bind(("value", marker.clone()))
            )
            .await
            .and_then(|r| r.check())
            .is_ok(),
            "bootstrap marker setup failed"
        );
        bootstrap_step(
            "marker before replacement",
            bootstrap_marker(&recovery, &marker),
        )
        .await;
        bootstrap_step(
            "replacement",
            replace(&recovery, DEFINE, "fixture_admin", &replacement),
        )
        .await;
        let retired = bootstrap_step("retired connection", connect(&endpoint)).await;
        assert!(
            observe(
                bootstrap_step(
                    "retired login",
                    retired.signin(credentials("fixture_admin", &original))
                )
                .await
                .map(|_| ())
            ) == SessionObservation::AuthenticationDenied,
            "old bootstrap password authenticated"
        );
        let current = bootstrap_step("replacement connection", connect(&endpoint)).await;
        assert!(
            bootstrap_step(
                "replacement login",
                current.signin(credentials("fixture_admin", &replacement))
            )
            .await
            .is_ok(),
            "replacement bootstrap password rejected"
        );
        assert!(
            bootstrap_step("replacement authority", privileged(&current)).await
                == SessionObservation::Authorized,
            "replacement bootstrap authority failed"
        );
        let bearer = bootstrap_step("retired JWT connection", connect(&endpoint)).await;
        assert!(
            observe(
                bootstrap_step(
                    "retired JWT",
                    bearer.authenticate((old_token.access.clone(), None))
                )
                .await
                .map(|_| ())
            ) == SessionObservation::AuthenticationDenied,
            "old bootstrap JWT reused"
        );
        let existing_session = bootstrap_step("existing session", privileged(&startup)).await;
        // Docker restart retains this exact guarded --rm container and writable layer.
        // Its unchanged startup environment still contains the ORIGINAL password.
        let endpoint = container
            .restart()
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let after = ready_bootstrap(&endpoint).await;
        assert!(
            bootstrap_step(
                "post-restart replacement login",
                after.signin(credentials("fixture_admin", &replacement))
            )
            .await
            .is_ok(),
            "restart did not preserve replacement password"
        );
        assert!(
            bootstrap_step("post-restart authority", privileged(&after)).await
                == SessionObservation::Authorized,
            "post-restart replacement authority failed"
        );
        bootstrap_step("post-restart data", bootstrap_marker(&after, &marker)).await;
        let old_after = bootstrap_step("post-restart retired connection", connect(&endpoint)).await;
        assert!(
            observe(
                bootstrap_step(
                    "post-restart retired login",
                    old_after.signin(credentials("fixture_admin", &original))
                )
                .await
                .map(|_| ())
            ) == SessionObservation::AuthenticationDenied,
            "startup environment restored the old password"
        );
        let old_bearer =
            bootstrap_step("post-restart retired JWT connection", connect(&endpoint)).await;
        assert!(
            observe(
                bootstrap_step(
                    "post-restart retired JWT",
                    old_bearer.authenticate(old_token)
                )
                .await
                .map(|_| ())
            ) == SessionObservation::AuthenticationDenied,
            "restart restored the old bootstrap JWT"
        );
        let recovery = bootstrap_step("post-restart recovery connection", connect(&endpoint)).await;
        assert!(
            bootstrap_step(
                "post-restart recovery login",
                recovery.signin(credentials("bootstrap_recovery", &alternate))
            )
            .await
            .is_ok(),
            "alternate ROOT did not survive restart"
        );
        assert!(
            bootstrap_step("post-restart recovery authority", privileged(&recovery)).await
                == SessionObservation::Authorized,
            "alternate ROOT lost authority"
        );
        bootstrap_step(
            "post-restart recovery data",
            bootstrap_marker(&recovery, &marker),
        )
        .await;
        bootstrap_step(
            "restore original",
            replace(&recovery, DEFINE, "fixture_admin", &original),
        )
        .await;
        let restored = bootstrap_step("restored connection", connect(&endpoint)).await;
        assert!(
            bootstrap_step(
                "restored login",
                restored.signin(credentials("fixture_admin", &original))
            )
            .await
            .is_ok(),
            "original ROOT recovery login failed"
        );
        assert!(
            bootstrap_step("restored authority", privileged(&restored)).await
                == SessionObservation::Authorized,
            "original ROOT recovery authority failed"
        );
        bootstrap_step("restored data", bootstrap_marker(&restored, &marker)).await;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct BootstrapObservation {
            existing_session_before_restart: SessionObservation,
            original_environment_did_not_restore_password: bool,
            alternate_root_and_data_retained: bool,
            original_root_recovered: bool,
        }
        println!(
            "{}",
            serde_json::to_string(&BootstrapObservation {
                existing_session_before_restart: existing_session,
                original_environment_did_not_restore_password: true,
                alternate_root_and_data_retained: true,
                original_root_recovered: true,
            })
            .expect("bootstrap safe observation encoding failed")
        );
        drop(container); // Await the maintained guard's cleanup before the result is reported.
    })
    .await
    .expect("persistent bootstrap credential qualification exceeded 240 seconds");
}
