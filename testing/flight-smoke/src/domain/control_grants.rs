//! Client wire validation for the UAV control-grant collection.
use super::*;
use serde::Serialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantPage {
    items: Vec<ControlGrant>,
    limit: usize,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ControlGrant {
    grant_id: String,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    permissions: BTreeSet<Permission>,
    pub(super) map_mobility_profile_uri: MapMobilityProfileUri,
    allow_planning_advisory: bool,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    created_by: String,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<String>,
    revision: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Permission {
    Inspect,
    Plan,
    Execute,
    Abort,
}

pub(super) async fn find(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    grant_id: &str,
    principal_key: &str,
) -> Result<ControlGrant> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        for _ in 0..100 {
            let visible = operator
                .call_tool(
                    "uav-sim__list_active_vehicle_control_grants",
                    serde_json::json!({ "session_id": scenario.session_id, "cursor": cursor }),
                )
                .await?;
            let page: GrantPage = serde_json::from_value(visible)?;
            ensure!(
                page.limit == 100 && page.items.len() <= page.limit,
                "active grant response violates its 100-item page contract"
            );
            if let Some(grant) = page.items.into_iter().find(|grant| {
                grant.grant_id == grant_id
                    && grant.principal_key == principal_key
                    && grant.vehicle_id == scenario.vehicle_id
                    && grant.session_id == scenario.session_id
            }) {
                ensure!(
                    [
                        Permission::Inspect,
                        Permission::Plan,
                        Permission::Execute,
                        Permission::Abort
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
