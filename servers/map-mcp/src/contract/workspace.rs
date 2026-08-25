use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Presentation-safe MapLibre Style basemap configuration owned by Map MCP.
///
/// The style URLs are returned to the sandboxed App and therefore must not
/// contain credentials. The supported profile keeps both styles and every
/// style dependency on the same origin, which the App resource declares
/// through MCP Apps CSP metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MapWorkspaceBasemap {
    pub id: String,
    pub title: String,
    pub light_style_url: String,
    pub dark_style_url: String,
}

impl MapWorkspaceBasemap {
    pub fn open_free_map(
        light_style_url: impl Into<String>,
        dark_style_url: impl Into<String>,
    ) -> Result<Self, String> {
        let basemap = Self {
            id: "open_free_map".to_owned(),
            title: "OpenFreeMap".to_owned(),
            light_style_url: light_style_url.into(),
            dark_style_url: dark_style_url.into(),
        };
        basemap.validate()?;
        Ok(basemap)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 64 || !self.id.bytes().all(valid_slug_byte) {
            return Err("basemap id must be a 1 to 64 byte lowercase slug".to_owned());
        }
        if self.title.is_empty()
            || self.title.len() > 128
            || self.title.chars().any(char::is_control)
        {
            return Err("basemap title must contain 1 to 128 non-control characters".to_owned());
        }
        self.origin()?;
        Ok(())
    }

    pub fn origin(&self) -> Result<String, String> {
        let light_origin = style_origin("light", &self.light_style_url)?;
        let dark_origin = style_origin("dark", &self.dark_style_url)?;
        if light_origin != dark_origin {
            return Err("light and dark basemap styles must use the same exact origin".to_owned());
        }
        Ok(light_origin)
    }
}

fn style_origin(variant: &str, style_url: &str) -> Result<String, String> {
    if style_url.contains(['\n', '\r', '\t']) {
        return Err(format!(
            "{variant} basemap style URL must not contain control whitespace"
        ));
    }
    let url = url::Url::parse(style_url)
        .map_err(|error| format!("{variant} basemap style URL is invalid: {error}"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err(format!(
            "{variant} basemap style URL must use credential-free HTTPS with an explicit host"
        ));
    }
    if url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err(format!(
            "{variant} basemap style URL must not contain credentials, a query, or a fragment"
        ));
    }
    Ok(url.origin().ascii_serialization())
}

fn valid_slug_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
}

/// Caller-specific capabilities for the single Map MCP workspace App.
///
/// The App reads this resource before rendering controls. Tool and resource
/// handlers remain the authorization boundary; these booleans only let the
/// view present the surface the current caller can actually use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MapWorkspaceAccess {
    pub administration: bool,
    pub dataset_read: bool,
    pub feature_read: bool,
    pub feature_write: bool,
    pub feature_publish: bool,
    pub basemap: MapWorkspaceBasemap,
}

#[cfg(test)]
mod tests {
    use super::MapWorkspaceBasemap;

    #[test]
    fn style_basemap_requires_credential_free_same_origin_https_urls() {
        let basemap = MapWorkspaceBasemap::open_free_map(
            "https://tiles.openfreemap.org/styles/positron",
            "https://tiles.openfreemap.org/styles/dark",
        )
        .expect("valid MapLibre style URLs");
        assert_eq!(basemap.origin().unwrap(), "https://tiles.openfreemap.org");
        for invalid in [
            "http://tiles.test/style.json",
            "https://user@tiles.test/style.json",
            "https://tiles.test/style.json?key=secret",
            "https://tiles.test/style.json#fragment",
        ] {
            assert!(
                MapWorkspaceBasemap::open_free_map(invalid, "https://tiles.test/dark").is_err()
            );
            assert!(
                MapWorkspaceBasemap::open_free_map("https://tiles.test/light", invalid).is_err()
            );
        }
        assert!(
            MapWorkspaceBasemap::open_free_map(
                "https://light.test/style.json",
                "https://dark.test/style.json"
            )
            .is_err()
        );
    }
}
