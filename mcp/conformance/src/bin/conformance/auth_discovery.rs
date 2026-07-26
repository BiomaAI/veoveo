use super::*;

#[derive(Debug, Deserialize)]
struct AuthDiscoveryMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
    bearer_methods_supported: Vec<String>,
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerDiscoveryMetadata {
    issuer: String,
    token_endpoint: String,
    #[serde(default)]
    jwks_uri: Option<String>,
    #[serde(default)]
    grant_types_supported: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default)]
    authorization_grant_profiles_supported: Vec<String>,
}

pub(super) struct AuthDiscoveryCheck<'a> {
    pub(super) endpoint_url: &'a str,
    pub(super) metadata_url: Option<&'a str>,
    pub(super) required_scopes: &'a [String],
    pub(super) required_extensions: &'a [String],
    pub(super) authorization_server_metadata_url: Option<&'a str>,
    pub(super) authorization_server_jwks_url: Option<&'a str>,
    pub(super) required_jwks_key_ids: &'a [String],
    pub(super) required_grant_types: &'a [String],
    pub(super) required_grant_profiles: &'a [String],
    pub(super) required_token_auth_methods: &'a [String],
}

pub(super) async fn cmd_auth_discovery(check: AuthDiscoveryCheck<'_>) -> Result<()> {
    let metadata_url = match check.metadata_url {
        Some(value) => value.to_string(),
        None => infer_protected_resource_metadata_url(check.endpoint_url)?,
    };
    let http = reqwest::Client::new();
    let metadata = http
        .get(&metadata_url)
        .send()
        .await?
        .error_for_status()?
        .json::<AuthDiscoveryMetadata>()
        .await?;
    if metadata.resource.is_empty() {
        return Err(anyhow!("protected-resource metadata has empty resource"));
    }
    if metadata.authorization_servers.is_empty() {
        return Err(anyhow!(
            "protected-resource metadata has no authorization servers"
        ));
    }
    if !metadata
        .bearer_methods_supported
        .iter()
        .any(|method| method == "header")
    {
        return Err(anyhow!(
            "protected-resource metadata does not support header bearer tokens"
        ));
    }
    for scope in check.required_scopes {
        if !metadata
            .scopes_supported
            .iter()
            .any(|candidate| candidate == scope)
        {
            return Err(anyhow!(
                "protected-resource metadata is missing required scope `{scope}`"
            ));
        }
    }
    for extension in check.required_extensions {
        if !metadata.extensions.contains_key(extension) {
            return Err(anyhow!(
                "protected-resource metadata is missing required extension `{extension}`"
            ));
        }
    }
    if let Some(authorization_server_metadata_url) = check.authorization_server_metadata_url {
        let authorization_server_metadata = http
            .get(authorization_server_metadata_url)
            .send()
            .await?
            .error_for_status()?
            .json::<AuthorizationServerDiscoveryMetadata>()
            .await?;
        if authorization_server_metadata.issuer.is_empty() {
            return Err(anyhow!("authorization-server metadata has empty issuer"));
        }
        if authorization_server_metadata.token_endpoint.is_empty() {
            return Err(anyhow!(
                "authorization-server metadata has empty token endpoint"
            ));
        }
        if authorization_server_metadata.jwks_uri.is_none() {
            return Err(anyhow!("authorization-server metadata has no jwks_uri"));
        }
        for grant_type in check.required_grant_types {
            if !authorization_server_metadata
                .grant_types_supported
                .iter()
                .any(|candidate| candidate == grant_type)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required grant type `{grant_type}`"
                ));
            }
        }
        for grant_profile in check.required_grant_profiles {
            if !authorization_server_metadata
                .authorization_grant_profiles_supported
                .iter()
                .any(|candidate| candidate == grant_profile)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required grant profile `{grant_profile}`"
                ));
            }
        }
        for auth_method in check.required_token_auth_methods {
            if !authorization_server_metadata
                .token_endpoint_auth_methods_supported
                .iter()
                .any(|candidate| candidate == auth_method)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required token auth method `{auth_method}`"
                ));
            }
        }
        if !check.required_jwks_key_ids.is_empty() {
            let jwks_url = check
                .authorization_server_jwks_url
                .or(authorization_server_metadata.jwks_uri.as_deref())
                .ok_or_else(|| anyhow!("authorization-server JWKS URL is required"))?;
            let jwks = http
                .get(jwks_url)
                .send()
                .await?
                .error_for_status()?
                .json::<JwkSet>()
                .await?;
            for key_id in check.required_jwks_key_ids {
                if !jwks
                    .keys
                    .iter()
                    .any(|key| key.common.key_id.as_deref() == Some(key_id.as_str()))
                {
                    return Err(anyhow!(
                        "authorization-server JWKS is missing required key id `{key_id}`"
                    ));
                }
            }
        }
    }

    let response = http.get(check.endpoint_url).send().await?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow!(
            "unauthenticated MCP endpoint returned {}, expected 401",
            response.status()
        ));
    }
    let challenge = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .ok_or_else(|| anyhow!("401 response is missing WWW-Authenticate"))?
        .to_str()?;
    if !challenge.starts_with("Bearer ") {
        return Err(anyhow!("WWW-Authenticate is not a Bearer challenge"));
    }
    if !challenge.contains("resource_metadata=") {
        return Err(anyhow!(
            "Bearer challenge is missing protected-resource metadata"
        ));
    }
    for scope in check.required_scopes {
        if !challenge.contains(scope) {
            return Err(anyhow!(
                "Bearer challenge is missing required scope `{scope}`"
            ));
        }
    }

    println!(
        "auth discovery ok: resource={}, authorization_servers={}, scopes={}, extensions={}",
        metadata.resource,
        metadata.authorization_servers.len(),
        metadata.scopes_supported.len(),
        metadata.extensions.len()
    );
    Ok(())
}

fn infer_protected_resource_metadata_url(endpoint_url: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(endpoint_url)?;
    let path = url.path().trim_end_matches('/');
    if !path.starts_with("/mcp/") {
        return Err(anyhow!(
            "cannot infer protected-resource metadata URL for non-gateway MCP path `{path}`"
        ));
    }
    url.set_path(&format!("/.well-known/oauth-protected-resource{path}"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}
