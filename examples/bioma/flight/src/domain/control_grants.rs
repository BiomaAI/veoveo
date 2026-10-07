//! Client wire validation for the UAV control-grant collection.
use super::*;
use veoveo_uav_sim_mcp::contract::{
    CollectionPage, ControlGrantId, VehicleControlGrant, VehicleControlPermission,
};

pub async fn find(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    grant_id: &ControlGrantId,
    principal_key: &str,
) -> Result<VehicleControlGrant> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        for _ in 0..100 {
            let visible = operator
                .call_tool(
                    "uav-sim__list_active_vehicle_control_grants",
                    serde_json::json!({ "sessionId": scenario.session_id, "cursor": cursor }),
                )
                .await?;
            let page: CollectionPage<VehicleControlGrant> = serde_json::from_value(visible)?;
            ensure!(
                page.limit == 100 && page.items.len() <= page.limit,
                "active grant response violates its 100-item page contract"
            );
            if let Some(grant) = page.items.into_iter().find(|grant| {
                &grant.grant_id == grant_id
                    && grant.principal_key == principal_key
                    && grant.vehicle_id == scenario.vehicle_id
                    && grant.session_id == scenario.session_id
            }) {
                ensure!(
                    [
                        VehicleControlPermission::Inspect,
                        VehicleControlPermission::Plan,
                        VehicleControlPermission::Execute,
                        VehicleControlPermission::Abort
                    ]
                    .iter()
                    .all(|required| grant.permissions.contains(required))
                        && grant.map_mobility_profile_uri == scenario.map_mobility_profile_uri,
                    "operator UAV control grant omits required permissions or Map profile"
                );
                return Ok(grant);
            }
            match page.next_cursor {
                Some(next) if !next.is_empty() && seen.insert(next.clone()) => cursor = Some(next),
                Some(_) => bail!("active grant cursor did not advance"),
                None => bail!("operator profile did not expose its active UAV control grant"),
            }
        }
        bail!("active grant qualification exceeded 100 pages")
    })
    .await
    .context("active grant qualification exceeded 60 seconds")?
}
