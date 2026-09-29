//! Fixed View discovery declarations. Typed construction lives in the owning contract.
/// Well-known surface roots (contract C18, C19), checked against typed builders.
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
