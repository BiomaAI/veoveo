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
/// Resource `_meta` key declaring the exact always-on agents an App may
/// address through the Console's authenticated human-message bridge.
pub const AGENT_MESSAGE_TARGETS_META_KEY: &str = "io.veoveo/agent-message-targets";

/// Closed declaration for a view's generic agent-message targets. The host
/// validates every identifier and treats malformed metadata as no authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentMessageTargets(pub Vec<String>);

impl AgentMessageTargets {
    pub fn new(values: impl IntoIterator<Item = String>) -> Option<Self> {
        let mut values = values.into_iter().collect::<Vec<_>>();
        if values.is_empty()
            || values.len() > 32
            || values.iter().any(|value| !valid_agent_id(value))
        {
            return None;
        }
        values.sort();
        values.dedup();
        Some(Self(values))
    }
}

fn valid_agent_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

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
    /// Requests host framing when true. Hosts dedicate their content workspace
    /// to the App when this field is absent or false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefers_border: Option<bool>,
}

/// Content-security domains an app view declares. Most Veoveo apps are fully
/// self-contained. A live-data app may name exact installation-owned origins;
/// hosts validate and enforce the declaration rather than accepting arbitrary
/// wildcard or path-bearing sources.
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
        assert_eq!(
            AGENT_MESSAGE_TARGETS_META_KEY,
            "io.veoveo/agent-message-targets"
        );
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

    #[test]
    fn agent_message_targets_are_sorted_bounded_and_fail_closed() {
        let targets = AgentMessageTargets::new([
            "workflow-coordinator".to_owned(),
            "assistant.2".to_owned(),
            "workflow-coordinator".to_owned(),
        ])
        .expect("valid targets");
        assert_eq!(targets.0, ["assistant.2", "workflow-coordinator"]);
        assert!(AgentMessageTargets::new(Vec::<String>::new()).is_none());
        assert!(AgentMessageTargets::new(["../agent".to_owned()]).is_none());
        assert!(AgentMessageTargets::new(["agent:other".to_owned()]).is_none());
    }
}
