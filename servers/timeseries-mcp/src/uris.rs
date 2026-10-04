//! Fixed declarations shared by the typed contract and hosted discovery.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::parse("timeseries").expect("declared server resource scheme")
    });

pub const DOCS_URI: &str = "timeseries://docs";
pub const CONTRACT_URI: &str = "timeseries://contract";
pub const DOC_TEMPLATE: &str = "timeseries://docs/{doc_id}";
pub const ARTIFACT_TEMPLATE: &str = "timeseries://artifact/{artifact_id}";
/// The forecast app view. The first path segment is the server slug; the
/// gateway's ServerOwned projection rewrites it to the mounted slug, so the
/// URI is stable end to end.
pub const FORECAST_APP_URI: &str = "ui://timeseries/forecast.html";
