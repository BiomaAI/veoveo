//! Startup/shutdown evidence with real store and unavailable provider endpoints.
//! It establishes control availability, not native execution or installed capacity.
#[path = "support/application.rs"]
#[allow(dead_code)]
mod app_support;
#[path = "support/configuration.rs"]
mod configuration;
#[path = "support/signing.rs"]
mod signing;
#[path = "../../../platform/computers/tests/support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;
use serde_json::Value;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use veoveo_computers_mcp::config::Configuration;
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn bioma_reference_configuration_admits_retained_and_execution_templates() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let files = configuration::Files::new();
    let mut input: Value = serde_json::from_str(include_str!(
        "../../../examples/bioma/computers/computers.json"
    ))
    .unwrap();
    input["capacity"]["gateway"]["transport"] = files.tls();
    input["capacity"]["allocator"] = files.tls();
    input["capacity"]["execution"]["keys"][0]["file"] = files
        .0
        .join("command.key")
        .to_string_lossy()
        .into_owned()
        .into();
    serde_json::from_value::<Configuration>(input)
        .unwrap()
        .prepare()
        .await
        .unwrap();
}

#[tokio::test]
async fn selected_configuration_validates_pins_and_trust_before_provider_connection() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let files = configuration::Files::new();
    let selected = files.configured("127.0.0.1:8787".parse().unwrap());
    let valid: Configuration = serde_json::from_value(selected.clone()).unwrap();
    assert!(
        valid.prepare().await.is_ok(),
        "offline provider must not invalidate complete configuration"
    );
    for changed in [
        "fingerprint",
        "template",
        "tls",
        "root",
        "quota",
        "origin",
        "lifetime",
    ] {
        let mut config = selected.clone();
        match changed {
            "fingerprint" => {
                config["capacity"]["templates"][0]["fingerprint"] = "f".repeat(64).into()
            }
            "template" => config["capacity"]["defaultTemplate"] = "f".repeat(64).into(),
            "tls" => {
                config["capacity"]["gateway"]["transport"]["keyFile"] = files
                    .0
                    .join("missing.pem")
                    .to_string_lossy()
                    .into_owned()
                    .into()
            }
            "root" => {
                config["capacity"]["templates"][0]["policy"]["process"]["runAsUser"] = "0".into()
            }
            "quota" => config["capacity"]["limits"]["owner"] = 500.into(),
            "origin" => config["allowedOrigins"] = serde_json::json!(["https://veoveo.bioma.ai/"]),
            "lifetime" => config["access"]["idleSeconds"] = 99999.into(),
            _ => unreachable!(),
        }
        match serde_json::from_value::<Configuration>(config) {
            Ok(config) => assert!(config.prepare().await.is_err(), "{changed}"),
            Err(_) => assert_eq!(changed, "quota"),
        }
    }
    std::fs::write(files.0.join("key.pem"), "invalid PEM").unwrap();
    assert!(
        serde_json::from_value::<Configuration>(selected)
            .unwrap()
            .prepare()
            .await
            .is_err()
    );
}

#[tokio::test]
async fn service_reports_setup_or_outage_and_stops_without_waiting_for_compute() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    let signing = signing::Signing::new();
    let files = configuration::Files::new();
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for configured in [false, true] {
        let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = reservation.local_addr().unwrap();
        let input = if configured {
            files.configured(address)
        } else {
            configuration::unconfigured(address)
        };
        let config = serde_json::from_value::<Configuration>(input)
            .unwrap()
            .prepare()
            .await
            .unwrap();
        drop(reservation);
        let shutdown = CancellationToken::new();
        let task = tokio::spawn(veoveo_computers_mcp::server::serve(
            config,
            TaskRuntime::new(db.a.clone(), "computers", "service-fixture"),
            signing.verifier.clone(),
            shutdown.clone(),
        ));
        let guard = task.abort_handle();
        struct Cleanup(tokio::task::AbortHandle);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _cleanup = Cleanup(guard);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                assert!(!task.is_finished(), "service stopped during startup");
                if let Ok(response) = client
                    .get(format!("http://{address}/computers/healthz"))
                    .send()
                    .await
                    && response.status().is_success()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let snapshot: Value = client
            .get(format!("http://{address}/computers/admin/computers"))
            .bearer_auth(signing.bearer("alice", "computers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            snapshot["availability"],
            if configured {
                "compute_unavailable"
            } else {
                "setup_required"
            }
        );
        assert_eq!(snapshot["canCreate"], false);
        assert_eq!(snapshot["template"].is_object(), configured);
        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("shutdown waited for unreachable provider")
            .unwrap()
            .unwrap();
    }
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reservation.local_addr().unwrap();
    let mut changed = files.configured(address);
    changed["capacity"]["limits"]["perOwner"] = 2.into();
    let config = serde_json::from_value::<Configuration>(changed)
        .unwrap()
        .prepare()
        .await
        .unwrap();
    drop(reservation);
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        veoveo_computers_mcp::server::serve(
            config,
            TaskRuntime::new(db.b.clone(), "computers", "stale-quota"),
            signing.verifier.clone(),
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap();
    assert!(
        result.is_err(),
        "startup must not replace an existing quota"
    );
    assert_eq!(
        veoveo_computers::ComputersStore::new(db.a.clone(), uuid::Uuid::from_u128(100))
            .unwrap()
            .capacity()
            .await
            .unwrap()
            .per_owner,
        1
    );
}

#[tokio::test]
async fn executable_rejects_privileged_store_credentials_without_echoing_secrets() {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_computers-mcp"))
            .env("VEOVEO_COMPUTERS_CONFIG", "/unused/config.json")
            .env("VEOVEO_SURREAL_ENDPOINT", "ws://localhost:1")
            .env("VEOVEO_SURREAL_NAMESPACE", "fixture")
            .env("VEOVEO_SURREAL_DATABASE", "fixture")
            .env("VEOVEO_SURREAL_AUTH_LEVEL", "root")
            .env("VEOVEO_SURREAL_USERNAME", "fixture")
            .env("VEOVEO_SURREAL_PASSWORD", "redaction-sentinel-password")
            .env("VEOVEO_INTERNAL_TRUST_JWKS", "redaction-sentinel-trust")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("database-scoped"));
    assert!(!diagnostic.contains("redaction-sentinel"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("redaction-sentinel"));
}

#[tokio::test]
async fn configuration_file_bounds_and_shape_errors_keep_input_bytes_private() {
    let files = configuration::Files::new();
    let path = files.0.join("config.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(1024 * 1024 + 1).unwrap();
    assert!(Configuration::load(&path).await.is_err());
    assert!(Configuration::load(&files.0).await.is_err());
    std::fs::write(&path, r#"{"schema":"redaction-sentinel"}"#).unwrap();
    let error = match Configuration::load(&path).await {
        Err(error) => error,
        Ok(_) => panic!("invalid schema accepted"),
    };
    assert!(error.to_string().contains("line 1"));
    assert!(!error.to_string().contains("redaction-sentinel"));
}

#[tokio::test]
async fn execution_configuration_requires_private_keys_and_a_qualified_default() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let files = configuration::Files::new();
    let selected = files.configured("127.0.0.1:8787".parse().unwrap());
    for changed in [
        "profile",
        "duplicate",
        "active",
        "keys",
        "relative",
        "endpoint",
        "query",
    ] {
        let mut value = selected.clone();
        let execution = &mut value["capacity"]["execution"];
        match changed {
            "profile" => execution["templateFingerprints"] = serde_json::json!([]),
            "duplicate" => execution["templateFingerprints"]
                .as_array_mut()
                .unwrap()
                .push(selected["capacity"]["defaultTemplate"].clone()),
            "active" => execution["activeKeyId"] = uuid::Uuid::from_u128(2).to_string().into(),
            "keys" => execution["keys"] = serde_json::json!([]),
            "relative" => execution["keys"][0]["file"] = "relative-command.key".into(),
            "endpoint" => {
                execution["artifactEndpoint"] = "https://user:password@artifact.invalid".into()
            }
            "query" => {
                execution["artifactEndpoint"] = "https://artifact.invalid/?token=fixture".into()
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<Configuration>(value)
                .unwrap()
                .prepare()
                .await
                .is_err(),
            "{changed}"
        );
    }
    let mut old = selected.clone();
    old["schema"] = "veoveo.io/computers-service/v1".into();
    assert!(serde_json::from_value::<Configuration>(old).is_err());
    for size in [0, 31, 33, 1024] {
        std::fs::write(files.0.join("command.key"), vec![23; size]).unwrap();
        assert!(
            serde_json::from_value::<Configuration>(selected.clone())
                .unwrap()
                .prepare()
                .await
                .is_err()
        );
    }
    std::fs::write(files.0.join("command.key"), [23; 32]).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            files.0.join("command.key"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(
            serde_json::from_value::<Configuration>(selected.clone())
                .unwrap()
                .prepare()
                .await
                .is_err()
        );
        std::fs::set_permissions(
            files.0.join("command.key"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        assert!(
            serde_json::from_value::<Configuration>(selected)
                .unwrap()
                .prepare()
                .await
                .is_ok()
        );
    }
}
