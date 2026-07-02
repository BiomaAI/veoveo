use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicDeployment {
    base_url: String,
    host_authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPublicEndpoint {
    public_url: String,
    mount_path: String,
}

impl PublicDeployment {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let (base_url, host_authority) = normalize_base_url(base_url.as_ref())?;
        Ok(Self {
            base_url,
            host_authority,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn host_authority(&self) -> &str {
        &self.host_authority
    }

    pub fn server(&self, server_slug: impl AsRef<str>) -> Result<ServerPublicEndpoint> {
        let server_slug = normalize_server_slug(server_slug.as_ref())?;
        let mount_path = format!("/{server_slug}");
        let public_url = format!("{}{}", self.base_url, mount_path);
        Ok(ServerPublicEndpoint {
            public_url,
            mount_path,
        })
    }
}

impl ServerPublicEndpoint {
    pub fn public_url(&self) -> &str {
        &self.public_url
    }

    pub fn mount_path(&self) -> &str {
        &self.mount_path
    }

    pub fn path(&self, child: &str) -> String {
        let child = child.trim_matches('/');
        if child.is_empty() {
            self.mount_path.clone()
        } else {
            format!("{}/{}", self.mount_path, child)
        }
    }

    pub fn url(&self, child: &str) -> String {
        let child = child.trim_matches('/');
        if child.is_empty() {
            self.public_url.clone()
        } else {
            format!("{}/{}", self.public_url, child)
        }
    }
}

fn normalize_base_url(input: &str) -> Result<(String, String)> {
    let value = input.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return Err(anyhow!("missing PUBLIC_BASE_URL"));
    }
    let authority = if let Some(rest) = value.strip_prefix("http://") {
        rest
    } else if let Some(rest) = value.strip_prefix("https://") {
        rest
    } else {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must start with http:// or https://"
        ));
    };
    if value.contains(['?', '#']) || value.chars().any(char::is_whitespace) {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must not contain whitespace, query, or fragment"
        ));
    }
    if authority.is_empty() {
        return Err(anyhow!("PUBLIC_BASE_URL must include a host"));
    }
    if authority.contains('/') {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must be an origin and must not include a path"
        ));
    }
    if authority.contains('@') {
        return Err(anyhow!("PUBLIC_BASE_URL must not include userinfo"));
    }
    let host_authority = authority.to_string();
    Ok((value, host_authority))
}

fn normalize_server_slug(input: &str) -> Result<String> {
    let value = input.trim();
    validate_path_segment(value, "server slug")?;
    Ok(value.to_string())
}

fn validate_path_segment(value: &str, name: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("{name} must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(anyhow!(
            "{name} must contain only lowercase ASCII letters, digits, hyphen, or underscore"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_server_public_endpoint_under_domain() {
        let deployment =
            PublicDeployment::new("https://veoveo.bioma.ai/").expect("valid deployment");
        let media = deployment.server("media").expect("valid server");

        assert_eq!(deployment.base_url(), "https://veoveo.bioma.ai");
        assert_eq!(deployment.host_authority(), "veoveo.bioma.ai");
        assert_eq!(media.mount_path(), "/media");
        assert_eq!(media.public_url(), "https://veoveo.bioma.ai/media");
        assert_eq!(media.path("mcp"), "/media/mcp");
        assert_eq!(
            media.url("webhooks"),
            "https://veoveo.bioma.ai/media/webhooks"
        );
    }

    #[test]
    fn base_url_can_have_arbitrary_subdomain_depth() {
        let deployment = PublicDeployment::new("https://deep.staging.enterprise.example.com")
            .expect("valid deployment");
        let media = deployment.server("media").expect("valid server");

        assert_eq!(
            deployment.base_url(),
            "https://deep.staging.enterprise.example.com"
        );
        assert_eq!(
            deployment.host_authority(),
            "deep.staging.enterprise.example.com"
        );
        assert_eq!(media.mount_path(), "/media");
        assert_eq!(
            media.public_url(),
            "https://deep.staging.enterprise.example.com/media"
        );
    }

    #[test]
    fn preserves_explicit_public_port_for_host_validation() {
        let deployment =
            PublicDeployment::new("https://veoveo.bioma.ai:8443").expect("valid deployment");

        assert_eq!(deployment.base_url(), "https://veoveo.bioma.ai:8443");
        assert_eq!(deployment.host_authority(), "veoveo.bioma.ai:8443");
    }

    #[test]
    fn rejects_base_url_paths() {
        let err = PublicDeployment::new("https://veoveo.bioma.ai/base")
            .expect_err("base URL path should fail");

        assert!(err.to_string().contains("must not include a path"));
    }
}
