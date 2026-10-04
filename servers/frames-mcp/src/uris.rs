//! Fixed declarations shared by the typed contract and hosted discovery.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::parse("frames").expect("declared server resource scheme")
    });

pub const WORKSPACE_APP_URI: &str = "ui://frames/workspace.html";
pub const WORLD_TEMPLATE: &str = "frames://world/{world_id}";
pub const WORLD_REVISION_TEMPLATE: &str = "frames://world/{world_id}/revision/{revision_id}";
pub const WORLD_FRAME_TEMPLATE: &str =
    "frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}";
pub const ARTIFACT_TEMPLATE: &str = "frames://artifact/{artifact_id}";

pub const DOCS_URI: &str = "frames://docs";
pub const CONTRACT_URI: &str = "frames://contract";
pub const DOC_TEMPLATE: &str = "frames://docs/{doc_id}";
