use std::error::Error;

use iri_string::template::simple_context::{SimpleContext, Value};
use veoveo_types::{ResourceTemplateError, ResourceTemplateUri, ResourceUri};

#[test]
fn declarations_preserve_full_rfc6570_syntax_and_wire_spelling() {
    for wire in [
        "example://root",
        "example://root/{id}",
        "example://root/{+path}",
        "example://root{.labels*}",
        "example://root{/path*}",
        "example://root{;fields*}",
        "example://root{?query,cursor}{&more*}",
        "example://root/{id:4}{#fragment}",
        "example://root/{x.y,%61,id,id}",
        "example://root/literal:0/{id:9999}",
    ] {
        let template = ResourceTemplateUri::new(wire).unwrap();
        assert_eq!(template.as_str(), wire);
        assert_eq!(serde_json::to_value(&template).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<ResourceTemplateUri>(wire.into()).unwrap(),
            template
        );
    }
    let template = ResourceTemplateUri::new("example://root/{x.y,%61,id,id}").unwrap();
    assert_eq!(
        template.variables().collect::<Vec<_>>(),
        ["x.y", "%61", "id", "id"]
    );
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(ResourceTemplateUri)).unwrap()["type"],
        "string"
    );
}

#[test]
fn invalid_templates_are_rejected_at_construction_and_deserialization() {
    for wire in [
        "",
        "relative/{id}",
        "example:opaque/{id}",
        "EXAMPLE://root/{id}",
        "example://",
        "{scheme}://root/{id}",
        "example://root/{",
        "example://root/}",
        "example://root/{}",
        "example://root/{{id}}",
        "example://root/{id name}",
        "example://root/{id:0}",
        "example://root/{a..b}",
        "example://root/{a.}",
        "example://root/{+a..b}",
        "example://root/{id:0001}",
        "example://root/{id:65536}",
        "example://root/{id:99999}",
        "example://root/{id:10000}",
        "example://root/{id:2*}",
        "example://root/{=id}",
        "example://root/%GG/{id}",
        "example://root/{%GG}",
        "example://root/a b/{id}",
        "example://root/{id}\n",
        "example://root/café/{id}",
    ] {
        assert!(ResourceTemplateUri::new(wire).is_err(), "{wire:?}");
        assert!(
            serde_json::from_value::<ResourceTemplateUri>(wire.into()).is_err(),
            "{wire:?}"
        );
    }
}

#[test]
fn expansion_handles_reserved_characters_unicode_lists_maps_and_modifiers() {
    let mut context = SimpleContext::new();
    context.insert("id", "a/b + café?");
    context.insert("path", "America/New_York");
    context.insert("list", Value::List(vec!["a/b".into(), "café".into()]));
    context.insert(
        "fields",
        Value::Assoc(vec![
            ("one".into(), "a b".into()),
            ("two".into(), "c+d".into()),
        ]),
    );
    for (wire, expected) in [
        (
            "example://root/{id}",
            "example://root/a%2Fb%20%2B%20caf%C3%A9%3F",
        ),
        ("example://root/{+path}", "example://root/America/New_York"),
        ("example://root/{path:7}", "example://root/America"),
        ("example://root{/list*}", "example://root/a%2Fb/caf%C3%A9"),
        (
            "example://root/x{.list*}",
            "example://root/x.a%2Fb.caf%C3%A9",
        ),
        (
            "example://root/{;fields*}",
            "example://root/;one=a%20b;two=c%2Bd",
        ),
        (
            "example://root{?fields*}",
            "example://root?one=a%20b&two=c%2Bd",
        ),
        (
            "example://root?first=yes{&id}",
            "example://root?first=yes&id=a%2Fb%20%2B%20caf%C3%A9%3F",
        ),
        ("example://root{?undefined}", "example://root"),
    ] {
        let template = ResourceTemplateUri::new(wire).unwrap();
        assert_eq!(
            template.expand(&context).unwrap().as_str(),
            expected,
            "{wire}"
        );
    }
}

#[test]
fn expansion_enforces_the_concrete_profile_after_substitution() {
    let mut context = SimpleContext::new();
    context.insert("secret", "private-fixture-value");
    context.insert("host", "user:private-fixture-value@host");
    context.insert("path", "../other");
    context.insert("id", Value::List(vec!["one".into(), "two".into()]));
    for wire in [
        "example://root{#secret}",
        "example://{+host}/root",
        "example://root/{+path}",
        "example://root{?id*}",
    ] {
        assert!(
            matches!(
                ResourceTemplateUri::new(wire).unwrap().expand(&context),
                Err(ResourceTemplateError::Concrete(_))
            ),
            "{wire}"
        );
    }
    let mismatch = ResourceTemplateUri::new("example://root/{id:2}")
        .unwrap()
        .expand(&context)
        .unwrap_err();
    assert_eq!(mismatch, ResourceTemplateError::Expansion);
    let invalid = ResourceTemplateUri::new("example://private-fixture-value/{").unwrap_err();
    for error in [
        invalid,
        mismatch,
        ResourceTemplateUri::new("example://root{#secret}")
            .unwrap()
            .expand(&context)
            .unwrap_err(),
    ] {
        assert!(!format!("{error:?} {error}").contains("private-fixture-value"));
        if let Some(source) = error.source() {
            assert!(!source.to_string().contains("private-fixture-value"));
        }
    }
}

#[test]
fn templates_require_their_own_type_at_wire_admission() {
    let wire = "example://root/{+path}{?cursor}";
    assert!(serde_json::from_value::<ResourceUri>(wire.into()).is_err());
    assert!(ResourceTemplateUri::new(wire).is_ok());
}

#[test]
fn scalar_maps_preserve_rfc_names_encoding_and_undefined_variables() {
    use std::collections::BTreeMap;
    let variables = BTreeMap::from([
        ("id".into(), "a/b+ café?".into()),
        ("cursor".into(), "page&token=foo".into()),
        ("%61".into(), "escaped-name".into()),
        ("a".into(), "plain-name".into()),
        ("unused".into(), "not referenced".into()),
    ]);
    for (template, expected) in [
        (
            "example://items/{id}{?cursor}",
            "example://items/a%2Fb%2B%20caf%C3%A9%3F?cursor=page%26token%3Dfoo",
        ),
        ("example://items/{%61}", "example://items/escaped-name"),
        ("example://items/{a}", "example://items/plain-name"),
        ("example://items{?missing}", "example://items"),
    ] {
        assert_eq!(
            ResourceTemplateUri::new(template)
                .unwrap()
                .expand_scalars(&variables)
                .unwrap()
                .as_str(),
            expected
        );
    }
    let variables = BTreeMap::from([("id".into(), "secret\0".into())]);
    let error = ResourceTemplateUri::new("example://items/{id}")
        .unwrap()
        .expand_scalars(&variables)
        .unwrap_err();
    assert!(!error.to_string().contains("secret"));
}
