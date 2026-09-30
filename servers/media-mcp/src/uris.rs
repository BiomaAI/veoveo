//! Fixed declarations shared by the public resource contract and hosted discovery.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("media").expect("declared server resource scheme")
    });

pub const MODELS_URI: &str = "media://models";
pub const STUDIO_APP_URI: &str = "ui://media/studio.html";
pub const MODEL_TEMPLATE: &str = crate::contract::MediaModelUri::TEMPLATE;
pub const ARTIFACT_TEMPLATE: &str = "media://artifact/{artifact_id}";

pub const DOCS_URI: &str = "media://docs";
pub const CONTRACT_URI: &str = "media://contract";
pub const DOC_TEMPLATE: &str = "media://docs/{doc_id}";
