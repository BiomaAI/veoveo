use crate::contract::{FrameId, ViewId};

pub const LAYERS: &str = "view://layers";
pub const VIEWS: &str = "view://views";
pub const FRAMES: &str = "view://frames";
pub const LAYER_TEMPLATE: &str = "view://layer/{layer_id}";
pub const VIEW_TEMPLATE: &str = "view://view/{view_id}";
pub const FRAME_TEMPLATE: &str = "view://frame/{frame_id}";
pub const VIEW_SCENE_TEMPLATE: &str = "view://view/{view_id}/scene";
pub const TILE_TEMPLATE: &str = "view://tile/{tile_key}";
/// The 3D preview MCP App view; the slug segment must match the gateway's
/// ServerOwned `ui://{slug}/{page}` projection.
pub const PREVIEW_APP_URI: &str = "ui://view/preview.html";

pub fn layer(layer_id: &crate::contract::LayerId) -> String {
    format!("view://layer/{layer_id}")
}

pub fn view(view_id: &ViewId) -> String {
    format!("view://view/{view_id}")
}

pub fn frame(frame_id: &FrameId) -> String {
    format!("view://frame/{frame_id}")
}

pub fn view_scene(view_id: &ViewId) -> String {
    format!("view://view/{view_id}/scene")
}

pub fn tile(tile_key: &str) -> String {
    format!("view://tile/{tile_key}")
}

pub fn parse_view(uri: &str) -> Option<ViewId> {
    ViewId::new(uri.strip_prefix("view://view/")?).ok()
}

pub fn parse_view_scene(uri: &str) -> Option<ViewId> {
    ViewId::new(uri.strip_prefix("view://view/")?.strip_suffix("/scene")?).ok()
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
        let uri = view_scene(&view_id);
        assert_eq!(uri, "view://view/abc123/scene");
        assert_eq!(parse_view_scene(&uri), Some(view_id));
        assert_eq!(parse_view(&uri), None, "slash is not a valid view id byte");
        assert_eq!(parse_view_scene("view://view/abc123"), None);
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
}
