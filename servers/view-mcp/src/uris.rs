use crate::contract::{ContractError, FrameId, PreviewScenePolicy, SceneCompositionId, ViewId};

/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new("view")`; a unit test below
/// pins that equivalence.
pub const DOCS: &str = "view://docs";
pub const CONTRACT: &str = "view://contract";

pub const LAYERS: &str = "view://layers";
pub const COMPOSITIONS: &str = "view://compositions";
pub const VIEWS: &str = "view://views";
pub const FRAMES: &str = "view://frames";
pub const DOC_TEMPLATE: &str = "view://docs/{doc_id}";
pub const LAYER_TEMPLATE: &str = "view://layer/{layer_id}";
pub const COMPOSITION_TEMPLATE: &str = "view://composition/{composition_id}";
pub const VIEW_TEMPLATE: &str = "view://view/{view_id}";
pub const FRAME_TEMPLATE: &str = "view://frame/{frame_id}";
pub const VIEW_SCENE_TEMPLATE: &str =
    "view://view/{view_id}/scene{?width_px,height_px,max_screen_error_px}";
pub const TILE_TEMPLATE: &str = "view://tile/{tile_key}";
/// The 3D preview MCP App view; the slug segment must match the gateway's
/// ServerOwned `ui://{slug}/{page}` projection.
pub const PREVIEW_APP_URI: &str = "ui://view/preview.html";

pub fn doc(doc_id: &str) -> String {
    format!("view://docs/{doc_id}")
}

pub fn layer(layer_id: &crate::contract::LayerId) -> String {
    format!("view://layer/{layer_id}")
}

pub fn view(view_id: &ViewId) -> String {
    format!("view://view/{view_id}")
}

pub fn composition(composition_id: &SceneCompositionId) -> String {
    format!("view://composition/{composition_id}")
}

pub fn frame(frame_id: &FrameId) -> String {
    format!("view://frame/{frame_id}")
}

pub fn view_scene(view_id: &ViewId, policy: PreviewScenePolicy) -> String {
    format!(
        "view://view/{view_id}/scene?width_px={}&height_px={}&max_screen_error_px={}",
        policy.width_px, policy.height_px, policy.max_screen_error_px
    )
}

pub fn tile(tile_key: &str) -> String {
    format!("view://tile/{tile_key}")
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("view://docs/")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_view(uri: &str) -> Option<ViewId> {
    ViewId::new(uri.strip_prefix("view://view/")?).ok()
}

pub fn parse_composition(uri: &str) -> Option<SceneCompositionId> {
    SceneCompositionId::parse(uri.strip_prefix("view://composition/")?).ok()
}

pub fn parse_view_scene(uri: &str) -> Result<Option<(ViewId, PreviewScenePolicy)>, ContractError> {
    let Some(rest) = uri.strip_prefix("view://view/") else {
        return Ok(None);
    };
    let Some((view_id, query)) = rest.split_once("/scene?") else {
        return Ok(None);
    };
    let view_id = ViewId::new(view_id)?;
    let mut width_px = None;
    let mut height_px = None;
    let mut max_screen_error_px = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match name.as_ref() {
            "width_px" => &mut width_px,
            "height_px" => &mut height_px,
            "max_screen_error_px" => {
                if max_screen_error_px.is_some() {
                    return Err(ContractError::InvalidPreviewSceneUri);
                }
                max_screen_error_px = Some(
                    value
                        .parse::<f32>()
                        .map_err(|_| ContractError::InvalidPreviewSceneUri)?,
                );
                continue;
            }
            _ => return Err(ContractError::InvalidPreviewSceneUri),
        };
        if slot.is_some() {
            return Err(ContractError::InvalidPreviewSceneUri);
        }
        *slot = Some(
            value
                .parse::<u32>()
                .map_err(|_| ContractError::InvalidPreviewSceneUri)?,
        );
    }
    Ok(Some((
        view_id,
        PreviewScenePolicy {
            width_px: width_px.ok_or(ContractError::InvalidPreviewSceneUri)?,
            height_px: height_px.ok_or(ContractError::InvalidPreviewSceneUri)?,
            max_screen_error_px: max_screen_error_px
                .ok_or(ContractError::InvalidPreviewSceneUri)?,
        },
    )))
}

/// Tile keys are exactly 64 lowercase hex characters (sha256).
pub fn parse_tile(uri: &str) -> Option<String> {
    let key = uri.strip_prefix("view://tile/")?;
    (key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| key.to_owned())
}

pub fn parse_frame(uri: &str) -> Option<FrameId> {
    FrameId::new(uri.strip_prefix("view://frame/")?).ok()
}

pub fn parse_layer(uri: &str) -> Option<crate::contract::LayerId> {
    crate::contract::LayerId::new(uri.strip_prefix("view://layer/")?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_uris_round_trip_and_do_not_shadow_views() {
        let view_id = ViewId::new("abc123").unwrap();
        let policy = PreviewScenePolicy {
            width_px: 1280,
            height_px: 720,
            max_screen_error_px: 16.0,
        };
        let uri = view_scene(&view_id, policy);
        assert_eq!(
            uri,
            "view://view/abc123/scene?width_px=1280&height_px=720&max_screen_error_px=16"
        );
        assert_eq!(parse_view_scene(&uri).unwrap(), Some((view_id, policy)));
        assert_eq!(parse_view(&uri), None, "slash is not a valid view id byte");
        assert_eq!(parse_view_scene("view://view/abc123").unwrap(), None);
        assert!(parse_view_scene("view://view/abc123/scene?width_px=1280").is_err());
    }

    #[test]
    fn tile_keys_accept_only_64_lowercase_hex() {
        let key = "a".repeat(64);
        assert_eq!(parse_tile(&tile(&key)), Some(key.clone()));
        assert_eq!(parse_tile("view://tile/abc"), None);
        assert_eq!(parse_tile(&format!("view://tile/{}", "A".repeat(64))), None);
        assert_eq!(
            parse_tile(&format!("view://tile/{}/x", "a".repeat(64))),
            None
        );
    }

    #[test]
    fn well_known_uris_match_the_shared_conventions() {
        let conventions = veoveo_mcp_contract::ServerResourceUris::new("view");
        assert_eq!(DOCS, conventions.docs_root_uri());
        assert_eq!(CONTRACT, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc("agents"), conventions.doc_uri("agents"));
        assert_eq!(parse_doc("view://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("view://docs"), None);
        assert_eq!(parse_doc("view://docs/agents/extra"), None);
    }

    #[test]
    fn composition_uris_round_trip() {
        let id =
            SceneCompositionId::parse("composition-7a8ed02e-d576-548f-aa5b-4e10f5bfd179").unwrap();
        let uri = composition(&id);
        assert_eq!(parse_composition(&uri), Some(id));
        assert_eq!(parse_composition("view://composition/not-canonical"), None);
    }
}
