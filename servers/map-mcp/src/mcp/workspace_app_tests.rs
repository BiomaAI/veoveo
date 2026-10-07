use super::{MapMcp, ResourceDiscoveryAccess, discoverable_resources};
use crate::{contract::MapWorkspaceBasemap, uris};

fn basemap() -> MapWorkspaceBasemap {
    MapWorkspaceBasemap::open_free_map(
        "https://tiles.openfreemap.org/styles/positron",
        "https://tiles.openfreemap.org/styles/dark",
    )
    .unwrap()
}

#[test]
fn workspace_tools_exist_in_the_canonical_router() {
    let tools = MapMcp::full_tool_router().list_all();
    assert!(!tools.is_empty());
    for expected in super::WORKSPACE_TOOLS {
        assert!(
            tools.iter().any(|tool| tool.name.as_ref() == *expected),
            "workspace tool {expected} is absent from the canonical router"
        );
    }
}

const WORKSPACE_APP: &str = include_str!("../../assets/workspace-app.html");

#[test]
fn workspace_uses_sans_serif_typography() {
    assert!(WORKSPACE_APP.contains("--sans:"));
    for obsolete_family in [
        "--serif",
        "ui-serif",
        "Iowan Old Style",
        "Palatino",
        "Georgia",
    ] {
        assert!(
            !WORKSPACE_APP.contains(obsolete_family),
            "workspace retains obsolete serif family {obsolete_family}"
        );
    }
}

#[test]
fn workspace_applies_host_context_and_uses_only_the_mcp_bridge() {
    assert!(WORKSPACE_APP.contains("ui/initialize"));
    assert!(WORKSPACE_APP.contains("ui/notifications/host-context-changed"));
    assert!(WORKSPACE_APP.contains("resources/read"));
    assert!(WORKSPACE_APP.contains("tools/call"));
    assert!(!WORKSPACE_APP.contains("<script src="));
    assert!(!WORKSPACE_APP.contains("<link href="));
    for external_reference in [
        "src=\"http://",
        "src=\"https://",
        "href=\"http://",
        "href=\"https://",
        "url(http://",
        "url(https://",
        "@import",
    ] {
        assert!(
            !WORKSPACE_APP
                .to_ascii_lowercase()
                .contains(external_reference),
            "workspace contains external fetch reference {external_reference}"
        );
    }
}

#[test]
fn workspace_is_permission_aware() {
    assert!(WORKSPACE_APP.contains("map://workspace"));
    for capability in [
        "administration",
        "datasetRead",
        "featureRead",
        "featureWrite",
        "featurePublish",
    ] {
        assert!(WORKSPACE_APP.contains(capability));
    }
}

#[test]
fn workspace_preserves_admin_and_authoring_operations() {
    for tool in [
        "register_source",
        "start_acquisition",
        "activate_release",
        "register_mobility_profile",
        "create_feature_layer",
        "validate_feature_changes",
        "commit_feature_changes",
        "query_features",
        "query_source_features",
        "publish_feature_layer",
        "create_map_composition",
        "inspect_geopackage",
        "import_feature_layer",
    ] {
        assert!(WORKSPACE_APP.contains(tool), "workspace is missing {tool}");
    }
}

#[test]
fn workspace_is_a_persistent_hardware_map_with_bounded_synchronized_previews() {
    for marker in [
        "hardware-backed WebGL2",
        "WEBGL_debug_renderer_info",
        "swiftshader",
        "publicationId",
        "query_features",
        "query_source_features",
        "Persistent map",
        "Data preview",
        "preview cap reached",
        "lightStyleUrl",
        "darkStyleUrl",
        "subscriptions/listen",
        "maplibre-gl@6.6.0",
        "maplibre-worker.cjs",
        "renderedFeatureCount",
        "without painting any returned feature",
    ] {
        assert!(
            WORKSPACE_APP.contains(marker),
            "workspace is missing {marker}"
        );
    }
}

#[test]
fn one_workspace_is_discoverable_for_dataset_feature_or_admin_access() {
    for access in [
        ResourceDiscoveryAccess {
            admin: true,
            dataset_read: false,
            feature_read: false,
            spatial_derive: false,
        },
        ResourceDiscoveryAccess {
            admin: false,
            dataset_read: false,
            feature_read: true,
            spatial_derive: false,
        },
        ResourceDiscoveryAccess {
            admin: false,
            dataset_read: true,
            feature_read: false,
            spatial_derive: false,
        },
    ] {
        let apps = discoverable_resources(access, &basemap())
            .into_iter()
            .filter(|resource| resource.uri.starts_with("ui://"))
            .collect::<Vec<_>>();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].uri, uris::WORKSPACE_APP_URI);
    }
    let workspace = discoverable_resources(
        ResourceDiscoveryAccess {
            admin: false,
            dataset_read: false,
            feature_read: true,
            spatial_derive: false,
        },
        &basemap(),
    )
    .into_iter()
    .find(|resource| resource.uri == uris::WORKSPACE_APP_URI)
    .expect("map workspace is discoverable");
    let metadata = veoveo_mcp_apps_extension::resource_ui_meta(&workspace)
        .expect("map workspace UI metadata is valid");
    assert_eq!(metadata.prefers_border, None);
    let csp = metadata.csp.unwrap();
    assert_eq!(csp.connect_domains, ["https://tiles.openfreemap.org"]);
    assert_eq!(csp.resource_domains, ["https://tiles.openfreemap.org"]);
}
