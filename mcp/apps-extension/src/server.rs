use rmcp::model::{Meta, Resource, ResourceContents, ServerCapabilities, Tool};

use crate::models::{
    APP_MIME_TYPE, EXTENSION_ID, ResourceUiMeta, ToolUiMeta, UI_META_KEY, UiVisibility,
};

/// Declares the apps extension in advertised server capabilities so hosts
/// can discover app-capable servers without scanning tool metadata.
pub fn extend_capabilities(capabilities: &mut ServerCapabilities) {
    capabilities
        .extensions
        .get_or_insert_default()
        .insert(EXTENSION_ID.to_owned(), extension_declaration());
}

fn extension_declaration() -> rmcp::model::JsonObject {
    let serde_json::Value::Object(declaration) = serde_json::json!({
        "mimeTypes": [APP_MIME_TYPE],
    }) else {
        unreachable!("extension declaration is an object literal");
    };
    declaration
}

/// An app view resource listing: correct MIME plus default `_meta.ui`.
pub fn app_resource(uri: &str, name: &str) -> Resource {
    app_resource_with_meta(uri, name, ResourceUiMeta::default())
}

/// An app view resource listing with an explicit, host-enforced UI policy.
pub fn app_resource_with_meta(uri: &str, name: &str, metadata: ResourceUiMeta) -> Resource {
    Resource::new(uri, name)
        .with_mime_type(APP_MIME_TYPE)
        .with_meta(ui_meta(&metadata))
}

/// The readable contents of an app view: a self-contained HTML document.
pub fn app_html_contents(uri: &str, html: &str) -> ResourceContents {
    ResourceContents::text(html, uri).with_mime_type(APP_MIME_TYPE)
}

/// Attaches `_meta.ui` to a listed tool, linking it to its app view.
pub fn link_tool_to_app(tool: Tool, resource_uri: &str, visibility: &[UiVisibility]) -> Tool {
    let link = ToolUiMeta {
        resource_uri: resource_uri.to_owned(),
        visibility: (!visibility.is_empty()).then(|| visibility.to_vec()),
    };
    tool.with_meta(ui_meta(&link))
}

fn ui_meta<T: serde::Serialize>(value: &T) -> Meta {
    let mut meta = Meta::new();
    meta.insert(
        UI_META_KEY.to_owned(),
        serde_json::to_value(value).expect("ui meta shapes serialize"),
    );
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_declare_the_extension_with_supported_mime_types() {
        let mut capabilities = ServerCapabilities::default();
        extend_capabilities(&mut capabilities);
        let declared = capabilities
            .extensions
            .expect("extensions declared")
            .remove(EXTENSION_ID)
            .expect("apps extension declared");
        assert_eq!(
            serde_json::Value::Object(declared),
            serde_json::json!({"mimeTypes": [APP_MIME_TYPE]})
        );
    }

    #[test]
    fn app_resources_carry_the_app_mime_type_and_ui_meta() {
        let resource = app_resource("ui://timeseries/forecast.html", "forecast-app");
        assert_eq!(resource.mime_type.as_deref(), Some(APP_MIME_TYPE));
        assert!(
            resource
                .meta
                .as_ref()
                .is_some_and(|meta| meta.0.contains_key(UI_META_KEY))
        );
    }

    #[test]
    fn app_contents_are_html_text_under_the_app_profile() {
        let contents = app_html_contents("ui://timeseries/forecast.html", "<!doctype html>");
        let ResourceContents::TextResourceContents {
            mime_type,
            text,
            uri,
            ..
        } = contents
        else {
            panic!("app contents must be text");
        };
        assert_eq!(mime_type.as_deref(), Some(APP_MIME_TYPE));
        assert_eq!(text, "<!doctype html>");
        assert_eq!(uri, "ui://timeseries/forecast.html");
    }

    #[test]
    fn linked_tools_nest_the_resource_uri_under_meta_ui() {
        let tool = Tool::new(
            "forecast",
            "forecast a series",
            rmcp::object!({"type": "object"}),
        );
        let linked = link_tool_to_app(
            tool,
            "ui://timeseries/forecast.html",
            &[UiVisibility::Model, UiVisibility::App],
        );
        let meta = linked.meta.expect("tool meta attached");
        assert_eq!(
            meta.0.get(UI_META_KEY).cloned().expect("ui key"),
            serde_json::json!({
                "resourceUri": "ui://timeseries/forecast.html",
                "visibility": ["model", "app"],
            })
        );
    }
}
