use veoveo_types::{IdentifierError, ResourceScheme, ResourceUri, ScopeName};

#[test]
fn scope_names_preserve_external_vocabularies_and_reject_invalid_tokens() {
    for name in ["map:feature:read", "openid", "new-server:custom-operation"] {
        let scope = ScopeName::parse(name).unwrap();
        assert_eq!(scope.as_str(), name);
        assert_eq!(
            serde_json::to_value(&scope).unwrap(),
            serde_json::json!(name)
        );
        assert_eq!(
            serde_json::from_value::<ScopeName>(serde_json::json!(name)).unwrap(),
            scope
        );
    }
    for invalid in [
        "",
        "two scopes",
        "read\nwrite",
        "scope\t",
        "x\0y",
        "\u{2003}read",
    ] {
        assert!(ScopeName::parse(invalid).is_err(), "accepted {invalid:?}");
        assert!(serde_json::from_value::<ScopeName>(serde_json::json!(invalid)).is_err());
    }
    assert_eq!(
        ScopeName::parse("").unwrap_err().to_string(),
        "invalid identifier \"\": must not be empty"
    );
}

#[test]
fn scheme_profile_and_reference_wire_spelling_are_preserved() {
    for valid in ["example", "uav-sim", "vendor.v2", "vendor+resource"] {
        assert_eq!(ResourceScheme::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in ["", "Map", "2map", "map_read", "map:read", "map/read"] {
        assert!(
            ResourceScheme::parse(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    for wire in [
        "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        "ui://map/workspace.html",
        "map://datasets?cursor=abcd",
        "media://model/provider/model",
        "https://localhost:8781/mcp/operator",
        "https://example.com/read?part=one&part=two#section",
        "file:///workspace/readme.md",
        "example://items/a%2Fb?cursor=a%2Bb",
    ] {
        let uri = ResourceUri::new(wire).unwrap();
        assert_eq!(uri.as_str(), wire);
        assert_eq!(
            serde_json::to_string(&uri).unwrap(),
            serde_json::to_string(wire).unwrap()
        );
        assert_eq!(
            serde_json::from_value::<ResourceUri>(serde_json::json!(wire)).unwrap(),
            uri
        );
    }
    for invalid in [
        "",
        "map:",
        "map://",
        "Map://catalog",
        "map://a b",
        "map://a\nb",
        "relative/path",
        "map://dataset/{dataset_id}/release/{release_id}",
        "map://datasets{?cursor}",
        "map://items/bad%",
        "map://items/bad%2",
        "map://items/bad%ZZ",
        "map://items/café",
        "https://[broken]/resource",
    ] {
        assert!(ResourceUri::new(invalid).is_err(), "accepted {invalid:?}");
        assert!(serde_json::from_value::<ResourceUri>(serde_json::json!(invalid)).is_err());
    }
}

#[test]
fn extraction_preserves_string_schemas_and_validation_error_shape() {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(ScopeName)).unwrap()["description"],
        "OAuth/OIDC scope value. It must not contain whitespace or control characters."
    );
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(ResourceScheme)).unwrap()["description"],
        "Server-owned resource URI scheme, for example `media`."
    );
    for schema in [
        schemars::schema_for!(ScopeName),
        schemars::schema_for!(ResourceScheme),
        schemars::schema_for!(ResourceUri),
    ] {
        let schema = serde_json::to_value(schema).unwrap();
        assert_eq!(schema["type"], "string");
    }
    assert_eq!(
        IdentifierError::new("id", "must be a UUIDv7").to_string(),
        "invalid identifier \"id\": must be a UUIDv7"
    );
}
