use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputersStore, Reservation, session_grants::SessionGrantPolicy,
};
use veoveo_mcp_contract::*;
use veoveo_platform_store::RecordId;

pub const PROVIDER: Uuid = Uuid::from_u128(72);
pub const LIMITS: SessionGrantPolicy = SessionGrantPolicy {
    max_grants: 2,
    absolute_seconds: 120,
    idle_seconds: 30,
};

pub fn control() -> GatewayControlPlane {
    let mut control = super::policy::control();
    let mut read = control.policies[0].rules[0].clone();
    read.id = PolicyRuleId::new("computer-read").unwrap();
    read.actions = [GatewayAction::ResourcesRead].into_iter().collect();
    read.tools.clear();
    control.policies[0].rules.push(read.clone());
    read.id = PolicyRuleId::new("computer-attach").unwrap();
    read.actions = [GatewayAction::ComputerAttach].into_iter().collect();
    control.policies[0].rules.push(read);
    control
}
pub async fn ready(
    db: &super::TestDb,
    actor: &ComputerActor,
) -> (ComputersStore, ComputersStore, Uuid) {
    super::policy::install(&db.a, control()).await;
    let a = ComputersStore::new(db.a.clone(), PROVIDER).unwrap();
    let b = ComputersStore::new(db.b.clone(), PROVIDER).unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 4,
            per_tenant: 8,
            provider: 8,
        },
    )
    .await
    .unwrap();
    a.install_session_grant_policy(None, LIMITS).await.unwrap();
    let computer = a
        .reserve(
            actor.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: super::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    db.a.client().query("UPDATE ONLY $computer SET phase = 'ready', provider_resource_id = 'fixture-resource', process_id = 'fixture-process';")
        .bind(("computer", RecordId::new("computer", surrealdb::types::Uuid::from(computer.computer_id))))
        .await.unwrap().check().unwrap();
    (a, b, computer.computer_id)
}
