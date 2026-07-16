use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::GatewayControlPlaneError;

pub const MAX_BRANDING_NAME_CHARS: usize = 120;
pub const MAX_BRANDING_PRODUCT_LABEL_CHARS: usize = 60;
pub const MAX_BRANDING_LOGO_BYTES: usize = 128 * 1024;

/// Installation-level white-label branding surfaced by operator-facing
/// clients such as the console. Configured per installation in the control
/// plane so a rebrand is a config revision, never an image rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InstallationBranding {
    /// Installation display name shown in window titles and chrome.
    pub name: String,
    /// Short product line rendered under the name (e.g. "Operations").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_label: Option<String>,
    /// Inline logo: either a complete `<svg…>` document or a `data:image/*`
    /// URI. Served same-origin to the browser; never a remote URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Accent color as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
}

impl InstallationBranding {
    pub fn validate(&self) -> Result<(), GatewayControlPlaneError> {
        let invalid = |field: &'static str, reason: String| {
            GatewayControlPlaneError::InvalidBranding { field, reason }
        };
        let name = self.name.trim();
        if name.is_empty() {
            return Err(invalid("name", "must not be empty".to_owned()));
        }
        if name.chars().count() > MAX_BRANDING_NAME_CHARS {
            return Err(invalid(
                "name",
                format!("must not exceed {MAX_BRANDING_NAME_CHARS} characters"),
            ));
        }
        if let Some(label) = &self.product_label {
            if label.trim().is_empty() {
                return Err(invalid("product_label", "must not be empty".to_owned()));
            }
            if label.chars().count() > MAX_BRANDING_PRODUCT_LABEL_CHARS {
                return Err(invalid(
                    "product_label",
                    format!("must not exceed {MAX_BRANDING_PRODUCT_LABEL_CHARS} characters"),
                ));
            }
        }
        if let Some(logo) = &self.logo {
            if logo.len() > MAX_BRANDING_LOGO_BYTES {
                return Err(invalid(
                    "logo",
                    format!(
                        "is {} bytes, exceeding the {MAX_BRANDING_LOGO_BYTES} byte limit",
                        logo.len()
                    ),
                ));
            }
            let trimmed = logo.trim_start();
            if !trimmed.starts_with("data:image/") && !trimmed.starts_with("<svg") {
                return Err(invalid(
                    "logo",
                    "must be an inline `<svg…>` document or a `data:image/*` URI".to_owned(),
                ));
            }
        }
        if let Some(accent) = &self.accent_color {
            let is_hex_color = accent.len() == 7
                && accent.starts_with('#')
                && accent[1..].chars().all(|digit| digit.is_ascii_hexdigit());
            if !is_hex_color {
                return Err(invalid(
                    "accent_color",
                    format!("`{accent}` must be a `#rrggbb` hex color"),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branding() -> InstallationBranding {
        InstallationBranding {
            name: "Acme Logistics".to_owned(),
            product_label: Some("Operations".to_owned()),
            logo: Some("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_owned()),
            accent_color: Some("#1c6e4b".to_owned()),
        }
    }

    #[test]
    fn accepts_complete_branding() {
        branding().validate().expect("branding should validate");
    }

    #[test]
    fn rejects_empty_name() {
        let mut invalid = branding();
        invalid.name = "  ".to_owned();
        assert!(matches!(
            invalid.validate(),
            Err(GatewayControlPlaneError::InvalidBranding { field: "name", .. })
        ));
    }

    #[test]
    fn rejects_remote_logo() {
        let mut invalid = branding();
        invalid.logo = Some("https://cdn.example.com/logo.svg".to_owned());
        assert!(matches!(
            invalid.validate(),
            Err(GatewayControlPlaneError::InvalidBranding { field: "logo", .. })
        ));
    }

    #[test]
    fn rejects_oversized_logo() {
        let mut invalid = branding();
        invalid.logo = Some(format!(
            "<svg>{}</svg>",
            "x".repeat(MAX_BRANDING_LOGO_BYTES)
        ));
        assert!(matches!(
            invalid.validate(),
            Err(GatewayControlPlaneError::InvalidBranding { field: "logo", .. })
        ));
    }

    #[test]
    fn rejects_malformed_accent_color() {
        let mut invalid = branding();
        invalid.accent_color = Some("green".to_owned());
        assert!(matches!(
            invalid.validate(),
            Err(GatewayControlPlaneError::InvalidBranding {
                field: "accent_color",
                ..
            })
        ));
    }

    #[test]
    fn accepts_data_uri_logo() {
        let mut valid = branding();
        valid.logo = Some("data:image/png;base64,iVBORw0KGgo=".to_owned());
        valid.validate().expect("data URI logo should validate");
    }
}
