use crate::contract::{FrameId, ViewId};

pub const LAYERS: &str = "view://layers";
pub const VIEWS: &str = "view://views";
pub const FRAMES: &str = "view://frames";
pub const LAYER_TEMPLATE: &str = "view://layer/{layer_id}";
pub const VIEW_TEMPLATE: &str = "view://view/{view_id}";
pub const FRAME_TEMPLATE: &str = "view://frame/{frame_id}";

pub fn layer(layer_id: &crate::contract::LayerId) -> String {
    format!("view://layer/{layer_id}")
}

pub fn view(view_id: &ViewId) -> String {
    format!("view://view/{view_id}")
}

pub fn frame(frame_id: &FrameId) -> String {
    format!("view://frame/{frame_id}")
}

pub fn parse_view(uri: &str) -> Option<ViewId> {
    ViewId::new(uri.strip_prefix("view://view/")?).ok()
}

pub fn parse_frame(uri: &str) -> Option<FrameId> {
    FrameId::new(uri.strip_prefix("view://frame/")?).ok()
}

pub fn parse_layer(uri: &str) -> Option<crate::contract::LayerId> {
    crate::contract::LayerId::new(uri.strip_prefix("view://layer/")?).ok()
}
