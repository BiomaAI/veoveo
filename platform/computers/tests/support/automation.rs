use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputersStore, api::*, automation_grants::AutomationGrantPolicy,
};
use veoveo_mcp_contract::{GatewayControlPlane, InvocationProvenance, LocalToolName};
use veoveo_platform_store::PrincipalKind;
pub const POLICY: AutomationGrantPolicy = AutomationGrantPolicy {
    max_grants: 2,
    maximum_lifetime_seconds: 3600,
    maximum_execution_seconds: 60,
    maximum_output_bytes: 65536,
};

pub fn control() -> GatewayControlPlane {
    let mut control = super::interactive::control();
    for tool in ["execute", "grant_automation", "revoke_automation"] {
        let tool = LocalToolName::new(tool).unwrap();
        control.servers[0].tools.push(tool.clone());
        control.policies[0].rules[0].tools.insert(tool);
    }
    control
}
pub async fn setup(
    db: &super::TestDb,
) -> (
    ComputersStore,
    ComputersStore,
    ComputerActor,
    ComputerActor,
    Uuid,
) {
    let owner = ComputerActor::from_verified(&super::browser::identity(db, "alice").await).unwrap();
    let (a, b, computer) = super::interactive::ready(db, &owner).await;
    super::policy::install(&db.a, control()).await;
    a.install_automation_grant_policy(None, POLICY)
        .await
        .unwrap();
    let mut service = super::owner("service");
    service.principal_kind = PrincipalKind::Service;
    service.authority.provenance = InvocationProvenance::Automated;
    db.a.ensure_identity(
        service.tenant_key(),
        &service.principal_key,
        &service.issuer,
        &service.subject,
        service.principal_kind,
    )
    .await
    .unwrap();
    let agent = super::authenticated(&service);
    (a, b, owner, agent, computer)
}
pub fn input(computer: Uuid) -> IssueAutomationGrantInput {
    IssueAutomationGrantInput {
        computer_id: computer,
        request_id: Uuid::now_v7(),
        principal_id: "https://computers.test#service".into(),
        oauth_client_id: "service".into(),
        name: "Build agent".into(),
        permissions: [AutomationPermission::Read, AutomationPermission::Execute].into(),
        execution_limits: Some(AutomationExecutionLimits {
            maximum_seconds: 30,
            maximum_output_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        }),
        expires_at: Utc::now() + TimeDelta::minutes(30),
    }
}
