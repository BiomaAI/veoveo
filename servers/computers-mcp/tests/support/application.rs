use crate::{support, template};
use std::time::Instant;
use tokio::sync::watch;
use uuid::Uuid;
use veoveo_computers::{CapacityPolicy, ComputersStore, api::*};
use veoveo_computers_mcp::{Application, CapacityHealth, NamedTemplate, Templates};
use veoveo_mcp_contract::{GatewayAction, PolicyRuleId};
use veoveo_task_runtime::TaskRuntime;
pub fn control() -> veoveo_mcp_contract::GatewayControlPlane {
    let mut control = support::policy::control();
    let mut read = control.policies[0].rules[0].clone();
    read.id = PolicyRuleId::new("computer-read").unwrap();
    read.actions = [GatewayAction::ResourcesRead].into_iter().collect();
    read.tools.clear();
    control.policies[0].rules.push(read);
    control
}
pub async fn identities(db: &support::TestDb) {
    support::policy::install(&db.a, control()).await;
    for name in ["alice", "bob"] {
        let owner = support::owner(name);
        db.a.ensure_identity(
            owner.tenant_key(),
            &owner.principal_key,
            &owner.issuer,
            &owner.subject,
            owner.principal_kind,
        )
        .await
        .unwrap();
    }
}
pub fn templates(new_default: bool) -> Templates {
    let old = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "f".repeat(64)
    ));
    let new = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "e".repeat(64)
    ));
    let default = if new_default {
        new.fingerprint()
    } else {
        old.fingerprint()
    };
    Templates::new(
        vec![
            NamedTemplate::new("development".into(), old).unwrap(),
            NamedTemplate::new("development".into(), new).unwrap(),
        ],
        Some(default),
    )
    .unwrap()
}
pub async fn application(
    db: &support::TestDb,
    new_default: bool,
) -> (Application, watch::Sender<CapacityHealth>) {
    application_on(db.a.clone(), new_default).await
}
pub async fn application_on(
    platform: veoveo_platform_store::PlatformStore,
    new_default: bool,
) -> (Application, watch::Sender<CapacityHealth>) {
    let store = ComputersStore::new(platform.clone(), Uuid::from_u128(100)).unwrap();
    store
        .install_capacity(
            None,
            CapacityPolicy {
                per_owner: 2,
                per_tenant: 4,
                provider: 4,
            },
        )
        .await
        .unwrap();
    let (health, receiver) = watch::channel(CapacityHealth {
        availability: CapacityAvailability::Available,
        observed_at: Instant::now(),
    });
    (
        Application::new(
            store,
            TaskRuntime::new(platform, "computers", "application"),
            templates(new_default),
            receiver,
            veoveo_computers_mcp::RuntimeAccess::unavailable(),
        )
        .unwrap(),
        health,
    )
}
