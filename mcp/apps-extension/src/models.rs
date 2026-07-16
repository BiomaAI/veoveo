use serde::{Deserialize, Serialize};

/// Extension identifier from the ext-apps specification. Note: `…/ui`, not
/// `…/apps` — the repository name and the identifier differ.
pub const EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
/// Stable ext-apps specification release this implementation is pinned to.
pub const SPEC_VERSION: &str = "2026-01-26";
/// Required MIME type for app view resources.
pub const APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
/// `_meta` key under which UI metadata nests on tools and resources.
pub const UI_META_KEY: &str = "ui";

/// `_meta.ui` on a tool: links the tool to the app view that renders it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUiMeta {
    pub resource_uri: String,
    /// Who may invoke the tool; defaults to both when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Vec<UiVisibility>>,
}

impl ToolUiMeta {
    pub fn visible_to_app(&self) -> bool {
        self.visibility
            .as_ref()
            .is_none_or(|visibility| visibility.contains(&UiVisibility::App))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiVisibility {
    Model,
    App,
}

/// `_meta.ui` on an app view resource.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUiMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csp: Option<UiCsp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefers_border: Option<bool>,
}

/// Content-security domains an app view declares. Veoveo apps are fully
/// self-contained, so first-party servers leave every list empty and hosts
/// apply a deny-all frame CSP; the field exists to faithfully parse
/// third-party app resources.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCsp {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connect_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_uri_domains: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_pinned_spec_release() {
        assert_eq!(EXTENSION_ID, "io.modelcontextprotocol/ui");
        assert_eq!(SPEC_VERSION, "2026-01-26");
        assert_eq!(APP_MIME_TYPE, "text/html;profile=mcp-app");
        assert_eq!(UI_META_KEY, "ui");
    }

    #[test]
    fn tool_ui_meta_serializes_the_nested_camel_case_shape() {
        let meta = ToolUiMeta {
            resource_uri: "ui://timeseries/forecast.html".to_owned(),
            visibility: Some(vec![UiVisibility::Model, UiVisibility::App]),
        };
        assert_eq!(
            serde_json::to_value(&meta).expect("serializes"),
            serde_json::json!({
                "resourceUri": "ui://timeseries/forecast.html",
                "visibility": ["model", "app"],
            })
        );
    }

    #[test]
    fn visibility_defaults_to_app_visible() {
        let meta: ToolUiMeta =
            serde_json::from_value(serde_json::json!({"resourceUri": "ui://x/y.html"}))
                .expect("parses");
        assert!(meta.visible_to_app());
        let model_only: ToolUiMeta = serde_json::from_value(
            serde_json::json!({"resourceUri": "ui://x/y.html", "visibility": ["model"]}),
        )
        .expect("parses");
        assert!(!model_only.visible_to_app());
    }
}
