use rmcp::model::{Resource, ServerCapabilities, Tool};

use crate::models::{APP_MIME_TYPE, EXTENSION_ID, ResourceUiMeta, ToolUiMeta, UI_META_KEY};

/// The host-side capability declaration announced at `initialize`:
/// `capabilities.extensions["io.modelcontextprotocol/ui"]`. Declaring is
/// optional per the spec — servers degrade to text-only for hosts that
/// don't — so there is no rejection path for peers without it.
pub fn host_extension_capability() -> (String, rmcp::model::JsonObject) {
    let serde_json::Value::Object(declaration) = serde_json::json!({
        "mimeTypes": [APP_MIME_TYPE],
    }) else {
        unreachable!("host capability is an object literal");
    };
    (EXTENSION_ID.to_owned(), declaration)
}

pub fn server_declares_ui(capabilities: &ServerCapabilities) -> bool {
    capabilities
        .extensions
        .as_ref()
        .is_some_and(|extensions| extensions.contains_key(EXTENSION_ID))
}

pub fn is_app_resource(resource: &Resource) -> bool {
    resource.mime_type.as_deref() == Some(APP_MIME_TYPE)
}

/// The app resource's declared host-enforced UI policy, when valid.
pub fn resource_ui_meta(resource: &Resource) -> Option<ResourceUiMeta> {
    let ui = resource.meta.as_ref()?.0.get(UI_META_KEY)?;
    serde_json::from_value(ui.clone()).ok()
}

/// The tool's app link, when it has one.
pub fn tool_app_link(tool: &Tool) -> Option<ToolUiMeta> {
    let ui = tool.meta.as_ref()?.0.get(UI_META_KEY)?;
    serde_json::from_value(ui.clone()).ok()
}

/// Whether an app view may invoke this tool through the host bridge.
/// Tools without UI metadata are model-only.
pub fn tool_visible_to_app(tool: &Tool) -> bool {
    tool_app_link(tool).is_some_and(|link| link.visible_to_app())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UiVisibility;
    use crate::server::{
        app_resource, app_resource_with_meta, extend_capabilities, link_tool_to_app,
    };

    #[test]
    fn host_and_server_declarations_round_trip() {
        let mut capabilities = ServerCapabilities::default();
        assert!(!server_declares_ui(&capabilities));
        extend_capabilities(&mut capabilities);
        assert!(server_declares_ui(&capabilities));
        let (id, declaration) = host_extension_capability();
        assert_eq!(id, EXTENSION_ID);
        assert_eq!(
            declaration.get("mimeTypes"),
            Some(&serde_json::json!([APP_MIME_TYPE]))
        );
    }

    #[test]
    fn app_resources_and_tool_links_are_detected() {
        let resource = app_resource("ui://timeseries/forecast.html", "forecast-app");
        assert!(is_app_resource(&resource));
        assert_eq!(resource_ui_meta(&resource), Some(ResourceUiMeta::default()));
        let plain = Resource::new("timeseries://usage", "usage");
        assert!(!is_app_resource(&plain));

        let tool = Tool::new("forecast", "forecast", rmcp::object!({"type": "object"}));
        assert!(!tool_visible_to_app(&tool));
        let linked = link_tool_to_app(tool, "ui://timeseries/forecast.html", &[]);
        let link = tool_app_link(&linked).expect("link parses");
        assert_eq!(link.resource_uri, "ui://timeseries/forecast.html");
        assert!(tool_visible_to_app(&linked));
        let model_only = link_tool_to_app(
            Tool::new("forecast", "forecast", rmcp::object!({"type": "object"})),
            "ui://timeseries/forecast.html",
            &[UiVisibility::Model],
        );
        assert!(!tool_visible_to_app(&model_only));

        let networked = app_resource_with_meta(
            "ui://uav-sim/live.html",
            "uav-live",
            ResourceUiMeta {
                csp: Some(crate::UiCsp {
                    connect_domains: vec!["wss://stream.example.com".to_owned()],
                    ..crate::UiCsp::default()
                }),
                prefers_border: Some(false),
            },
        );
        assert_eq!(
            resource_ui_meta(&networked)
                .and_then(|metadata| metadata.csp)
                .expect("CSP parses")
                .connect_domains,
            vec!["wss://stream.example.com"]
        );
    }
}
