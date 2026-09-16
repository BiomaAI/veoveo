//! Exact App tool admission over the gateway-projected, caller-visible catalog.
use rmcp::model::{Resource, Tool};
use veoveo_mcp_contract::{APP_TOOL_DEPENDENCIES_META_KEY, AppToolDependency, LocalToolName};

use crate::{is_app_resource, tool_app_link};

fn imports(resource: &Resource) -> Vec<AppToolDependency> {
    resource
        .meta
        .as_ref()
        .and_then(|meta| meta.0.get(APP_TOOL_DEPENDENCIES_META_KEY))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// Callers supply a resource and tools read through current MCP authority.
/// App-supplied metadata must never enter this projection.
pub fn resolve_app_tool<'a>(
    resource: &Resource,
    tools: &'a [Tool],
    alias: &str,
) -> Option<&'a Tool> {
    LocalToolName::new(alias).ok()?;
    if alias.contains("__") {
        return None;
    }
    if !is_app_resource(resource) {
        return None;
    }
    let (owner, page) = resource.uri.strip_prefix("ui://")?.split_once('/')?;
    if owner.is_empty() || page.is_empty() || resource.uri.contains("..") {
        return None;
    }
    let imported = imports(resource)
        .into_iter()
        .filter(|dependency| dependency.app_resource.as_str() == resource.uri)
        .find_map(|dependency| {
            dependency
                .tools
                .iter()
                .find(|import| import.name.as_str() == alias)
                .map(|import| format!("{}__{}", dependency.server, import.target_tool))
        });
    let target = imported
        .clone()
        .unwrap_or_else(|| format!("{owner}__{alias}"));
    tools.iter().find(|tool| {
        tool.name == target
            && (imported.is_some()
                || tool_app_link(tool)
                    .is_some_and(|link| link.visible_to_app() && link.resource_uri == resource.uri))
    })
}

/// Reauthorize an already resolved intent just before dispatch and during recovery.
pub fn app_allows_tool(resource: &Resource, tools: &[Tool], target: &str) -> bool {
    if !is_app_resource(resource) {
        return false;
    }
    let Some((owner, _)) = resource
        .uri
        .strip_prefix("ui://")
        .and_then(|uri| uri.split_once('/'))
    else {
        return false;
    };
    let Some(tool) = tools.iter().find(|tool| tool.name == target) else {
        return false;
    };
    (target.starts_with(&format!("{owner}__"))
        && tool_app_link(tool)
            .is_some_and(|link| link.visible_to_app() && link.resource_uri == resource.uri))
        || imports(resource).iter().any(|dependency| {
            dependency.app_resource.as_str() == resource.uri
                && dependency.tools.iter().any(|import| {
                    format!("{}__{}", dependency.server, import.target_tool) == target
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiVisibility, app_resource, link_tool_to_app};

    #[test]
    fn imported_aliases_require_exact_view_and_current_visible_target() {
        let mut view = app_resource("ui://frames/edit.html", "Edit");
        view.meta.as_mut().unwrap().insert(APP_TOOL_DEPENDENCIES_META_KEY.into(), serde_json::json!([{
            "app_resource":"ui://frames/edit.html", "server":"time", "required_scope":"time:read",
            "tools":[{"name":"clock", "target_tool":"now"}]
        }]));
        let target = Tool::new("time__now", "Time", rmcp::model::JsonObject::new());
        let tools = [target];
        assert_eq!(
            resolve_app_tool(&view, &tools, "clock").unwrap().name,
            "time__now"
        );
        assert!(app_allows_tool(&view, &tools, "time__now"));
        assert!(resolve_app_tool(&view, &tools, "now").is_none());
        assert!(resolve_app_tool(&view, &[], "clock").is_none());
        view.uri = "ui://frames/other.html".into();
        assert!(!app_allows_tool(&view, &tools, "time__now"));
        assert!(resolve_app_tool(&view, &tools, "clock").is_none());
    }

    #[test]
    fn tools_require_the_exact_view_and_app_visibility() {
        let view = app_resource("ui://media/create.html", "Create");
        let other = app_resource("ui://media/other.html", "Other");
        let tool = link_tool_to_app(
            Tool::new("media__run", "Run", rmcp::model::JsonObject::new()),
            &view.uri,
            &[UiVisibility::App],
        );
        assert!(resolve_app_tool(&view, std::slice::from_ref(&tool), "run").is_some());
        assert!(app_allows_tool(
            &view,
            std::slice::from_ref(&tool),
            "media__run"
        ));
        assert!(resolve_app_tool(&view, std::slice::from_ref(&tool), "media__run").is_none());
        assert!(!app_allows_tool(
            &other,
            std::slice::from_ref(&tool),
            "media__run"
        ));
        assert!(!app_allows_tool(&view, &[], "media__run"));
    }
}
