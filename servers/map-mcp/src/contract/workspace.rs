use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const XYZ_TOKENS: [&str; 3] = ["{z}", "{x}", "{y}"];

/// Presentation-safe XYZ basemap configuration owned by Map MCP.
///
/// The tile template is returned to the sandboxed App and therefore must not
/// contain credentials. The App resource declares the template's exact HTTPS
/// origin through MCP Apps CSP metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MapWorkspaceBasemap {
    pub id: String,
    pub title: String,
    pub tile_url_template: String,
    pub attribution: String,
    pub minimum_zoom: u8,
    pub maximum_zoom: u8,
    pub tile_size: u16,
}

impl MapWorkspaceBasemap {
    pub fn open_street_map(tile_url_template: impl Into<String>) -> Result<Self, String> {
        let basemap = Self {
            id: "open_street_map".to_owned(),
            title: "OpenStreetMap".to_owned(),
            tile_url_template: tile_url_template.into(),
            attribution: "© OpenStreetMap contributors".to_owned(),
            minimum_zoom: 0,
            maximum_zoom: 19,
            tile_size: 256,
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
        if self.attribution.is_empty()
            || self.attribution.len() > 1_024
            || self.attribution.chars().any(char::is_control)
        {
            return Err(
                "basemap attribution must contain 1 to 1024 non-control characters".to_owned(),
            );
        }
        if self.minimum_zoom > self.maximum_zoom || self.maximum_zoom > 24 {
            return Err("basemap zooms must be ordered within 0..=24".to_owned());
        }
        if self.tile_size != 256 && self.tile_size != 512 {
            return Err("basemap tile size must be 256 or 512 pixels".to_owned());
        }
        for token in XYZ_TOKENS {
            if self.tile_url_template.matches(token).count() != 1 {
                return Err(format!(
                    "basemap tile URL template must contain exactly one `{token}` token"
                ));
            }
        }
        if self.tile_url_template.contains(['\n', '\r', '\t']) {
            return Err("basemap tile URL template must not contain control whitespace".to_owned());
        }
        self.origin()?;
        Ok(())
    }

    pub fn origin(&self) -> Result<String, String> {
        let mut probe = self.tile_url_template.clone();
        for token in XYZ_TOKENS {
            probe = probe.replace(token, "0");
        }
        let url = url::Url::parse(&probe)
            .map_err(|error| format!("basemap tile URL template is invalid: {error}"))?;
        if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
            return Err(
                "basemap tile URL template must use credential-free HTTPS with an explicit host"
                    .to_owned(),
            );
        }
        if url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
            return Err(
                "basemap tile URL template must not contain credentials, a query, or a fragment"
                    .to_owned(),
            );
        }
        Ok(url.origin().ascii_serialization())
    }
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
    fn xyz_basemap_requires_a_credential_free_https_template() {
        let basemap =
            MapWorkspaceBasemap::open_street_map("https://tile.openstreetmap.org/{z}/{x}/{y}.png")
                .expect("valid XYZ template");
        assert_eq!(basemap.origin().unwrap(), "https://tile.openstreetmap.org");
        assert!(MapWorkspaceBasemap::open_street_map("http://tiles.test/{z}/{x}/{y}.png").is_err());
        assert!(
            MapWorkspaceBasemap::open_street_map("https://user@tiles.test/{z}/{x}/{y}.png")
                .is_err()
        );
        assert!(
            MapWorkspaceBasemap::open_street_map("https://tiles.test/{z}/{x}/{y}.png?key=secret")
                .is_err()
        );
        assert!(MapWorkspaceBasemap::open_street_map("https://tiles.test/{z}/{x}.png").is_err());
    }
}
