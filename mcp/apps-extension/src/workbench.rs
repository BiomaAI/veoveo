use serde::Serialize;

/// One canonical resource surfaced by a server-owned operational App.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchResource<'a> {
    pub label: &'a str,
    pub uri: &'a str,
}

/// One server-owned tool surfaced by an operational App.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchTool<'a> {
    pub label: &'a str,
    pub name: &'a str,
    /// A JSON object shown as the editable direct-launch starting point.
    pub arguments_json: &'a str,
}

/// Typed input for the shared host-neutral operational App shell.
///
/// Servers own these values and therefore retain their domain vocabulary,
/// resources, tools, and authorization boundary. The shared shell owns only
/// MCP Apps transport and generic presentation behavior.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchApp<'a> {
    pub app_id: &'a str,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub empty_message: &'a str,
    pub resources: &'a [WorkbenchResource<'a>],
    pub tools: &'a [WorkbenchTool<'a>],
}

/// Render a self-contained MCP App document for a server-owned workbench.
pub fn workbench_app_html(config: &WorkbenchApp<'_>) -> String {
    let config = serde_json::to_string(config)
        .expect("static workbench configuration must serialize")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    include_str!("workbench.html").replace("__VEOVEO_APP_CONFIG__", &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_app_is_self_contained_and_uses_only_mcp_bridge_operations() {
        let html = workbench_app_html(&WorkbenchApp {
            app_id: "example-library",
            title: "Library",
            subtitle: "Governed example objects",
            empty_message: "No examples are visible.",
            resources: &[WorkbenchResource {
                label: "Examples",
                uri: "example://index",
            }],
            tools: &[WorkbenchTool {
                label: "Inspect",
                name: "inspect",
                arguments_json: r#"{"id":""}"#,
            }],
        });
        assert!(html.contains("example://index"));
        assert!(html.contains("tools/call"));
        assert!(html.contains("resources/read"));
        assert!(html.contains("tasks/get"));
        assert!(!html.contains("__VEOVEO_APP_CONFIG__"));
        assert!(!html.contains("<script src="));
    }
}
