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
