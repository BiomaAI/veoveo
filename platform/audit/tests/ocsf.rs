//! Independent OCSF 1.9.0 qualification using the upstream validator.
//! Sends only synthetic fixtures; no installation records or credentials are read.
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use veoveo_audit::{export::ocsf::Event, *};

const UPSTREAM: &str = "https://schema.ocsf.io/1.9.0";

#[derive(Deserialize)]
struct Version {
    version: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Issue {
    error: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Warning {
    warning: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Validation {
    #[serde(default)]
    error: Option<String>,
    error_count: u64,
    errors: Vec<Issue>,
    warning_count: u64,
    warnings: Vec<Warning>,
}

#[derive(Serialize)]
struct CaseResult {
    case: &'static str,
    outcome: AuditOutcome,
    validation: Validation,
}

fn cases() -> Vec<(&'static str, AuditDetail, AuditTarget, bool)> {
    vec![
        (
            "anonymous",
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditTarget::Installation,
            false,
        ),
        (
            "read",
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditTarget::PlatformResource {
                uri: veoveo_types::ResourceUri::new("veoveo://fixture/resource").unwrap(),
            },
            true,
        ),
        (
            "authentication",
            AuditDetail::Authentication {
                activity: AuthenticationActivity::Login,
                method: veoveo_types::AuthMethod::OidcAuthorizationCodePkce,
                reason: veoveo_types::AuthReasonCode::AuthAllow,
            },
            AuditTarget::Profile {
                profile: "fixture".parse().unwrap(),
            },
            true,
        ),
        (
            "account",
            AuditDetail::AccountChange {
                activity: AccountActivity::Delete,
            },
            AuditTarget::Principal {
                tenant: "fixture".parse().unwrap(),
                principal: "removed-user".parse().unwrap(),
            },
            true,
        ),
        (
            "work-context",
            AuditDetail::AccountChange {
                activity: AccountActivity::Update,
            },
            AuditTarget::WorkContext {
                tenant: "fixture".parse().unwrap(),
                context: "operations".parse().unwrap(),
            },
            true,
        ),
        (
            "artifact",
            AuditDetail::Artifact {
                activity: ArtifactActivity::Download,
                requested: None,
                subject: None,
                release_state: None,
                related: None,
                bytes: Some(16),
                window_start: None,
            },
            AuditTarget::Artifact {
                artifact: uuid::Uuid::now_v7().to_string().parse().unwrap(),
            },
            true,
        ),
        (
            "live-view",
            AuditDetail::LiveView {
                activity: LiveViewActivity::Issue,
            },
            AuditTarget::PlatformResource {
                uri: veoveo_types::ResourceUri::new("veoveo://fixture/live-view").unwrap(),
            },
            true,
        ),
        (
            "computer",
            AuditDetail::Computer {
                activity: ComputerActivity::Create,
                stage: ComputerAuditStage::Reserved,
                task: None,
            },
            computer_target(),
            true,
        ),
    ]
}

#[tokio::test]
#[ignore = "requires network access to the pinned upstream OCSF 1.9.0 validator; sends only synthetic fixtures"]
async fn upstream_accepts_each_export_class_and_outcome() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let version = client
            .get(format!("{UPSTREAM}/api/version"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Version>()
            .await
            .unwrap();
        assert_eq!(version.version, "1.9.0");
        let mut results = Vec::new();
        for (case, detail, target, identified) in cases() {
            for outcome in [
                AuditOutcome::Allowed,
                AuditOutcome::Succeeded,
                AuditOutcome::Denied,
                AuditOutcome::Failed,
            ] {
                let reason = match outcome {
                    AuditOutcome::Allowed | AuditOutcome::Succeeded => AuditReason::Accepted,
                    AuditOutcome::Denied => AuditReason::PolicyDenied,
                    AuditOutcome::Failed => AuditReason::Unavailable,
                };
                let mut request = AuditRequest::background();
                if identified {
                    request.source_ip = Some("192.0.2.1".parse().unwrap());
                }
                let mut detail = detail.clone();
                if let AuditDetail::Authentication { reason, .. } = &mut detail {
                    *reason = match outcome {
                        AuditOutcome::Allowed | AuditOutcome::Succeeded => {
                            veoveo_types::AuthReasonCode::AuthAllow
                        }
                        AuditOutcome::Denied => veoveo_types::AuthReasonCode::PolicyDenied,
                        AuditOutcome::Failed => veoveo_types::AuthReasonCode::AuthStateUnavailable,
                    };
                }
                let mut draft =
                    AuditDraft::builder(request, target.clone(), detail, outcome, reason);
                if identified {
                    draft = draft.actor(AuditActor {
                        principal: "synthetic-user".parse().unwrap(),
                        tenant: Some("fixture".parse().unwrap()),
                        kind: AuditPrincipalKind::User,
                        oauth_client: None,
                        session_family: None,
                        delegating_principal: None,
                        managed_agent: None,
                    });
                }
                let record = AuditRecord {
                    draft: draft.build().unwrap(),
                    recorded_at: Utc::now(),
                };
                let validation = client
                    .post(format!("{UPSTREAM}/api/v2/validate"))
                    .json(&Event::from_record(&record))
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .json::<Validation>()
                    .await
                    .unwrap();
                results.push(CaseResult {
                    case,
                    outcome,
                    validation,
                });
            }
        }
        println!("{}", serde_json::to_string(&results).unwrap());
        let failures: Vec<_> = results
            .iter()
            .filter(|result| {
                result.validation.error.is_some()
                    || result.validation.error_count != 0
                    || !result.validation.errors.is_empty()
            })
            .map(|result| (result.case, result.outcome, &result.validation))
            .collect();
        assert!(
            failures.is_empty(),
            "OCSF validation failures: {failures:#?}"
        );
    })
    .await
    .expect("OCSF qualification exceeded 180 seconds");
}

fn computer_target() -> AuditTarget {
    let mut builder = AuditTargetRegistry::builder();
    veoveo_computers_contract::register_audit_target(&mut builder).unwrap();
    builder
        .build()
        .target(veoveo_computers_contract::ComputerAuditTarget {
            computer: veoveo_computers_contract::ComputerId::new(),
        })
        .unwrap()
}
