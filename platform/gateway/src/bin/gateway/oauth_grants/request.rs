use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct TokenRequest {
    pub(crate) grant_type: String,
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) scope: Option<String>,
    #[serde(default)]
    pub(crate) resource: Option<String>,
    #[serde(default)]
    pub(crate) work_context: Option<String>,
    #[serde(default)]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) redirect_uri: Option<String>,
    #[serde(default)]
    pub(crate) code_verifier: Option<String>,
    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    #[serde(default)]
    pub(crate) client_assertion_type: Option<String>,
    #[serde(default)]
    pub(crate) client_assertion: Option<String>,
    #[serde(default)]
    pub(crate) assertion: Option<String>,
}
