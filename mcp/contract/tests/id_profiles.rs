use std::any::TypeId;

use schemars::JsonSchema;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::{
    CompatibilityHelperId, IdentityProviderId, OAuthRefreshToken, PrincipalDisplayName,
    deployment::{DeploymentProfileId, DeploymentRequirementId},
};
use veoveo_types::{Identity, LocalToolName, ServerSlug};

#[test]
fn gateway_profiles_preserve_owner_admission_and_string_schemas() {
    let provider = IdentityProviderId::new("enterprise-idp").unwrap();
    assert_eq!(provider.identity_text(), "enterprise-idp");
    assert_eq!(serde_json::to_value(&provider).unwrap(), "enterprise-idp");
    assert!(IdentityProviderId::new("invalid/idp").is_err());
    assert!("invalid/idp".parse::<IdentityProviderId>().is_err());
    assert!(serde_json::from_str::<IdentityProviderId>(r#""invalid/idp""#).is_err());

    let tool = GatewayToolName::from_parts(
        &ServerSlug::new("media").unwrap(),
        &LocalToolName::new("render").unwrap(),
    )
    .unwrap();
    assert_eq!(tool.as_str(), "media__render");
    assert!(CompatibilityHelperId::new("media.models").is_ok());
    assert!(CompatibilityHelperId::new("media.models.extra").is_err());

    let schema = serde_json::to_value(schemars::schema_for!(IdentityProviderId)).unwrap();
    assert_eq!(schema["title"], "IdentityProviderId");
    assert_eq!(schema["type"], "string");
    assert_eq!(
        schema["description"],
        "Configured identity provider id used by gateway profiles."
    );
    assert!(schema.get("pattern").is_none());
    assert_ne!(
        IdentityProviderId::schema_id(),
        GatewayToolName::schema_id()
    );
}

#[test]
fn display_metadata_keeps_checked_text_without_becoming_an_identity() {
    let label = PrincipalDisplayName::new("Ana María").unwrap();
    assert_eq!(label.as_str(), "Ana María");
    assert_eq!(label.to_string(), "Ana María");
    assert_eq!(serde_json::to_value(&label).unwrap(), "Ana María");
    for value in ["", " padded", "control\n"] {
        assert!(PrincipalDisplayName::new(value).is_err());
        assert!(value.parse::<PrincipalDisplayName>().is_err());
        assert!(serde_json::from_value::<PrincipalDisplayName>(serde_json::json!(value)).is_err());
    }
}

#[test]
fn deployment_ids_keep_owner_rules_and_nominal_types() {
    let profile = DeploymentProfileId::new("connected_installation-1").unwrap();
    assert_eq!(profile.identity_text(), "connected_installation-1");
    assert_eq!(profile.as_ref(), "connected_installation-1");
    assert_eq!(
        "connected_installation-1"
            .parse::<DeploymentProfileId>()
            .unwrap(),
        profile
    );
    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        "connected_installation-1"
    );
    for value in ["", "Uppercase", "a/b", "padded "] {
        assert!(DeploymentProfileId::new(value).is_err());
        assert!(value.parse::<DeploymentRequirementId>().is_err());
        assert!(serde_json::from_value::<DeploymentProfileId>(serde_json::json!(value)).is_err());
    }
    assert!(
        DeploymentProfileId::new("Uppercase")
            .unwrap_err()
            .to_string()
            .starts_with("DeploymentProfileId ")
    );
    assert_ne!(
        TypeId::of::<DeploymentProfileId>(),
        TypeId::of::<DeploymentRequirementId>()
    );
    assert_ne!(
        DeploymentProfileId::schema_id(),
        DeploymentRequirementId::schema_id()
    );
    let schema = serde_json::to_value(schemars::schema_for!(DeploymentProfileId)).unwrap();
    assert_eq!(schema["title"], "DeploymentProfileId");
    assert_eq!(schema["type"], "string");
    assert!(schema.get("pattern").is_none());
}

#[test]
fn refresh_tokens_expose_wire_text_and_redact_formatters() {
    let raw = "refresh_token_with_distinctive_wire_material_1234567890";
    let token = OAuthRefreshToken::new(raw).unwrap();
    assert_eq!(token.as_str(), raw);
    assert_eq!(token.as_ref(), raw);
    assert_eq!(token.identity_text(), raw);
    assert_eq!(serde_json::to_value(&token).unwrap(), raw);
    assert_eq!(
        serde_json::from_value::<OAuthRefreshToken>(serde_json::json!(raw)).unwrap(),
        token
    );
    assert_eq!(String::from(token.clone()), raw);
    assert_eq!(token.to_string(), "[REDACTED]");
    assert_eq!(format!("{token:?}"), "OAuthRefreshToken([REDACTED])");
    let schema = serde_json::to_value(schemars::schema_for!(OAuthRefreshToken)).unwrap();
    assert_eq!(schema["title"], "OAuthRefreshToken");
    assert_eq!(schema["type"], "string");
    assert_eq!(
        schema["description"],
        "Opaque, rotating OAuth refresh token. Only its SHA-256 digest is persisted."
    );
}

#[test]
fn rejected_refresh_token_diagnostics_never_retain_input() {
    for raw in [
        "distinctive_rejected_secret_credential_1234567890!",
        "distinctive_short_secret!",
    ] {
        let errors = [
            OAuthRefreshToken::new(raw).unwrap_err(),
            raw.parse::<OAuthRefreshToken>().unwrap_err(),
            OAuthRefreshToken::try_from(raw.to_owned()).unwrap_err(),
            OAuthRefreshToken::parse_identity(raw).unwrap_err(),
        ];
        for error in errors {
            for diagnostic in [error.to_string(), format!("{error:?}")] {
                assert!(!diagnostic.contains(raw));
                assert!(!diagnostic.contains("distinctive"));
                assert!(diagnostic.contains("[REDACTED]"));
            }
        }
        let encoded = serde_json::to_string(raw).unwrap();
        let errors = [
            serde_json::from_value::<OAuthRefreshToken>(serde_json::json!(raw)).unwrap_err(),
            serde_json::from_str::<OAuthRefreshToken>(&encoded).unwrap_err(),
        ];
        for error in errors {
            for diagnostic in [error.to_string(), format!("{error:?}")] {
                assert!(!diagnostic.contains(raw));
                assert!(!diagnostic.contains("distinctive"));
                assert!(diagnostic.contains("[REDACTED]"));
            }
        }
    }
}
