use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

pub const PLANS_ROOT_URI: &str = "optimization://plans";
pub const PLAN_TEMPLATE: &str = "optimization://plan/{plan_id}";
pub const ARTIFACT_TEMPLATE: &str = "optimization://artifact/{artifact_id}";
pub const USAGE_ROOT_URI: &str = "optimization://usage";
pub const USAGE_TASK_TEMPLATE: &str = "optimization://usage/task/{task_id}";

fn optimization_uris() -> ServerResourceUris {
    ServerResourceUris::new("optimization")
}

pub fn artifact_uri(artifact_id: ArtifactId) -> String {
    optimization_uris().artifact_uri(artifact_id)
}

pub fn plan_uri(plan_id: &crate::contract::PlanId) -> String {
    format!("optimization://plan/{plan_id}")
}

pub fn parse_plan_uri(uri: &str) -> Option<crate::contract::PlanId> {
    let id = uri.strip_prefix("optimization://plan/")?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    crate::contract::PlanId::parse(id.to_owned()).ok()
}

pub fn usage_task_uri(task_id: &str) -> String {
    optimization_uris().usage_task_uri(task_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    optimization_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    optimization_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_uri_round_trips() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri, format!("optimization://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(&uri), Some(artifact_id));
        assert_eq!(parse_artifact_uri("optimization://artifact/nope"), None);
    }

    #[test]
    fn plan_uri_round_trips() {
        let plan_id = crate::contract::PlanId::from_stable_key(b"plan");
        let uri = plan_uri(&plan_id);
        assert_eq!(parse_plan_uri(&uri), Some(plan_id));
        assert_eq!(parse_plan_uri("optimization://plan/nope"), None);
    }

    #[test]
    fn usage_task_uri_round_trips() {
        let uri = usage_task_uri("task-1");
        assert_eq!(uri, "optimization://usage/task/task-1");
        assert_eq!(parse_usage_task_uri(&uri), Some("task-1"));
        assert_eq!(parse_usage_task_uri("optimization://usage/task/a/b"), None);
    }
}
