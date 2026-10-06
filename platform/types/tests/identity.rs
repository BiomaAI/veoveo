#[path = "support/naming.rs"]
mod naming_baseline;
fn capture_schema<T: schemars::JsonSchema>(
    schemas: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) {
    schemas.insert(
        name.into(),
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
    );
}
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{fmt::Debug, str::FromStr};
use veoveo_types::{
    AccessSubject, DataLabelId, DelegationId, GroupId, InvocationMode, InvocationProvenance,
    OAuthClientId, PolicyVersion, PrincipalId, RoleId, TenantId, TokenIssuer, TokenSubject,
    WorkContextId,
};

fn assert_wire_profile<T>(accepted: &[&str], rejected: &[&str])
where
    T: FromStr + Serialize + DeserializeOwned + AsRef<str> + Debug + PartialEq,
    T::Err: Debug,
{
    for spelling in accepted {
        let value: T = spelling.parse().unwrap();
        assert_eq!(value.as_ref(), *spelling);
        assert_eq!(serde_json::to_value(&value).unwrap(), json!(spelling));
        assert_eq!(serde_json::from_value::<T>(json!(spelling)).unwrap(), value);
    }
    for spelling in rejected {
        assert!(spelling.parse::<T>().is_err(), "accepted {spelling:?}");
        assert!(serde_json::from_value::<T>(json!(spelling)).is_err());
    }
    for value in [Value::Null, json!(2), json!(["id"]), json!({"id": "value"})] {
        assert!(serde_json::from_value::<T>(value).is_err());
    }
}

#[test]
fn claim_identities_preserve_external_spelling() {
    // Claim IDs admit whitespace and Unicode without normalization or a byte limit.
    // Domain-specific IDs have their own lexical profiles.
    let long = "subject".repeat(200);
    let accepted = [
        "issuer#subject",
        "https://idp.example/subject/1",
        " Ops ",
        " ",
        "操作者",
        &long,
    ];
    let rejected = ["", "subject\n", "a\tb", "a\0b", "a\u{7f}b", "a\u{85}b"];
    assert_wire_profile::<TokenIssuer>(&accepted, &rejected);
    assert_wire_profile::<TokenSubject>(&accepted, &rejected);
    assert_wire_profile::<PrincipalId>(&accepted, &rejected);
    assert_wire_profile::<TenantId>(&accepted, &rejected);
    assert_wire_profile::<DelegationId>(&accepted, &rejected);
    assert_wire_profile::<GroupId>(&accepted, &rejected);
    assert_wire_profile::<RoleId>(&accepted, &rejected);
}

#[test]
fn oauth_client_identity_and_schema_preserve_unbounded_claim_text_admission() {
    assert_eq!(
        <OAuthClientId as schemars::JsonSchema>::schema_id(),
        "veoveo_types::platform_names::OAuthClientId"
    );
    let long = "client".repeat(200);
    assert_wire_profile::<OAuthClientId>(
        &["agent", " Ops ", " ", "操作者", &long],
        &["", "client\n", "a\tb", "a\0b", "a\u{7f}b", "a\u{85}b"],
    );
    let schema = serde_json::to_value(schemars::schema_for!(OAuthClientId)).unwrap();
    assert_eq!(schema["minLength"], 1);
    assert_eq!(schema["not"]["pattern"], r"[\u0000-\u001f\u007f-\u009f]");
    assert!(schema.get("maxLength").is_none());
}

#[test]
fn label_and_policy_tokens_preserve_the_repository_profile() {
    let accepted = ["cui", "vendor:label", "policy/v2", "機密", "a+b"];
    let rejected = [
        "",
        "two tokens",
        "x\n",
        "x\t",
        "x\0",
        "x\u{2003}y",
        "x\u{a0}y",
    ];
    assert_wire_profile::<DataLabelId>(&accepted, &rejected);
    assert_wire_profile::<PolicyVersion>(&accepted, &rejected);
}

#[test]
fn work_context_profile_is_a_path_identity() {
    let long = "a".repeat(1024);
    assert_wire_profile::<WorkContextId>(
        &["operations", "a-z_9", "0", "-", "_", &long],
        &[
            "",
            "Operations",
            "ops/one",
            "ops.one",
            "ops:one",
            " ops ",
            "é",
            "ops\n",
        ],
    );
    assert_eq!(
        WorkContextId::parse("").unwrap_err().to_string(),
        "invalid identifier \"\": must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
    );
    assert_eq!(
        WorkContextId::parse("Ops").unwrap_err().to_string(),
        "invalid identifier \"Ops\": must contain only lowercase ASCII letters, digits, hyphen, or underscore",
    );
    assert_eq!(
        PrincipalId::parse("").unwrap_err().to_string(),
        "invalid identifier \"\": must not be empty"
    );
    assert_eq!(
        DataLabelId::parse("a b").unwrap_err().to_string(),
        "invalid identifier \"a b\": must not contain whitespace or control characters"
    );
}

#[test]
fn subjects_preserve_their_tagged_wire_representation() {
    for (subject, wire) in [
        (
            AccessSubject::Principal(PrincipalId::parse("idp#actor").unwrap()),
            json!({"kind": "principal", "id": "idp#actor"}),
        ),
        (
            AccessSubject::Group(GroupId::parse("operators").unwrap()),
            json!({"kind": "group", "id": "operators"}),
        ),
    ] {
        assert_eq!(serde_json::to_value(&subject).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<AccessSubject>(wire).unwrap(),
            subject
        );
    }
    for wire in [
        json!({"kind": "principal", "id": ""}),
        json!({"kind": "group"}),
        json!({"kind": "role", "id": "operator"}),
    ] {
        assert!(serde_json::from_value::<AccessSubject>(wire).is_err());
    }
}

#[test]
fn provenance_preserves_attribution_and_required_delegation() {
    let actor = PrincipalId::parse("idp#actor").unwrap();
    for (value, mode, wire) in [
        (
            InvocationProvenance::Direct {
                initiator: actor.clone(),
            },
            InvocationMode::Direct,
            json!({"mode": "direct", "initiator": "idp#actor"}),
        ),
        (
            InvocationProvenance::Delegated {
                initiator: actor.clone(),
                delegation_id: DelegationId::parse("delegation/17").unwrap(),
            },
            InvocationMode::Delegated,
            json!({"mode": "delegated", "initiator": "idp#actor", "delegation_id": "delegation/17"}),
        ),
        (
            InvocationProvenance::Automated,
            InvocationMode::Automated,
            json!({"mode": "automated"}),
        ),
    ] {
        assert_eq!(value.mode(), mode);
        assert_eq!(
            value.initiator(),
            if mode == InvocationMode::Automated {
                None
            } else {
                Some(&actor)
            }
        );
        assert_eq!(serde_json::to_value(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<InvocationProvenance>(wire.clone()).unwrap(),
            value
        );
        assert_eq!(serde_json::to_value(mode).unwrap(), wire["mode"]);
        assert_eq!(
            serde_json::from_value::<InvocationMode>(wire["mode"].clone()).unwrap(),
            mode
        );
    }
    for wire in [
        json!({"mode": "direct"}),
        json!({"mode": "delegated", "initiator": "idp#actor"}),
        json!({"mode": "delegated", "initiator": "idp#actor", "delegation_id": ""}),
        json!({"mode": "direct", "initiator": ""}),
        json!({"mode": "unknown"}),
    ] {
        assert!(serde_json::from_value::<InvocationProvenance>(wire).is_err());
    }
}

#[test]
fn identity_schemas_match_the_pre_extraction_contract() {
    // Captured from the MCP-owned types before moving them into this crate.
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/identity_schemas.json")).unwrap();
    let mut schemas = serde_json::Map::new();

    capture_schema::<DataLabelId>(&mut schemas, stringify!(DataLabelId));
    capture_schema::<PrincipalId>(&mut schemas, stringify!(PrincipalId));
    capture_schema::<TenantId>(&mut schemas, stringify!(TenantId));
    capture_schema::<WorkContextId>(&mut schemas, stringify!(WorkContextId));
    capture_schema::<DelegationId>(&mut schemas, stringify!(DelegationId));
    capture_schema::<GroupId>(&mut schemas, stringify!(GroupId));
    capture_schema::<RoleId>(&mut schemas, stringify!(RoleId));
    capture_schema::<PolicyVersion>(&mut schemas, stringify!(PolicyVersion));
    capture_schema::<AccessSubject>(&mut schemas, stringify!(AccessSubject));
    capture_schema::<InvocationMode>(&mut schemas, stringify!(InvocationMode));
    capture_schema::<InvocationProvenance>(&mut schemas, stringify!(InvocationProvenance));
    assert_eq!(
        naming_baseline::constraints(Value::Object(schemas)),
        expected
    );
}

#[test]
fn access_subject_variants_reject_undeclared_fields() {
    for input in [
        json!({"kind":"principal","id":"issuer#user"}),
        json!({"kind":"group","id":"engineering"}),
    ] {
        assert!(serde_json::from_value::<AccessSubject>(input.clone()).is_ok());
        let mut extra = input;
        extra["undeclared"] = json!(true);
        assert!(
            serde_json::from_value::<AccessSubject>(extra)
                .unwrap_err()
                .to_string()
                .starts_with("unknown field `undeclared`")
        );
    }
}

#[derive(Deserialize)]
#[serde(
    rename = "AccessSubject",
    rename_all = "snake_case",
    tag = "kind",
    content = "id",
    deny_unknown_fields
)]
enum LegacySubject {
    Principal(PrincipalId),
    Group(GroupId),
}
impl From<LegacySubject> for AccessSubject {
    fn from(subject: LegacySubject) -> Self {
        match subject {
            LegacySubject::Principal(id) => Self::Principal(id),
            LegacySubject::Group(id) => Self::Group(id),
        }
    }
}

#[test]
fn subject_decoder_preserves_field_order_and_rejects_duplicate_missing_and_invalid_fields() {
    for wire in [
        r#"{"kind":"principal","id":"issuer#user"}"#,
        r#"{"id":"issuer#user","kind":"principal"}"#,
        r#"{"kind":"group","id":"engineering"}"#,
        r#"{"id":"engineering","kind":"group"}"#,
        r#"["principal","issuer#user"]"#,
        r#"["group","engineering"]"#,
        r#"{"kind":{"principal":null},"id":"issuer#user"}"#,
        r#"{"id":"engineering","kind":{"group":null}}"#,
    ] {
        let legacy: LegacySubject = serde_json::from_str(wire).unwrap();
        assert_eq!(
            serde_json::from_str::<AccessSubject>(wire).unwrap(),
            legacy.into()
        );
    }
    for wire in [
        r#"{"kind":"principal","kind":"group","id":"user"}"#,
        r#"{"kind":"group","id":"user","id":"other"}"#,
        r#"{"id":"user","id":"other","kind":"principal"}"#,
        r#"{"kind":"principal"}"#,
        r#"{"id":"user"}"#,
        r#"{}"#,
        r#"{"kind":"role","id":"user"}"#,
        r#"{"kind":42,"id":"user"}"#,
        r#"{"kind":"principal","id":""}"#,
        r#"{"kind":"group","id":"\u0000"}"#,
        r#"{"id":null,"kind":"principal"}"#,
        r#"{"kind":"group","id":{"secret":"sentinel"}}"#,
        r#"["principal"]"#,
        r#"[{"principal":null},"user"]"#,
        r#"[{"group":null},"engineering"]"#,
    ] {
        assert!(
            serde_json::from_str::<LegacySubject>(wire).is_err(),
            "{wire}"
        );
        assert!(
            serde_json::from_str::<AccessSubject>(wire).is_err(),
            "{wire}"
        );
    }
    for wire in [
        r#"{"extra":"secret-sentinel","kind":"principal","id":"user"}"#,
        r#"{"kind":"principal","id":"user","extra":"secret-sentinel"}"#,
        r#"{"extra":"secret-sentinel","id":"engineering","kind":"group"}"#,
        r#"{"id":"engineering","kind":"group","extra":"secret-sentinel"}"#,
    ] {
        let error = serde_json::from_str::<AccessSubject>(wire)
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("unknown field `extra`"), "{error}");
        assert!(!error.contains("secret-sentinel"));
    }
}

#[derive(Clone, Copy)]
enum BinarySubjectTag<'a> {
    Ordinal(u32),
    Text(&'a str),
    Bytes(&'a [u8]),
}

struct BinarySubject<'a> {
    tag: BinarySubjectTag<'a>,
    id: &'a str,
    field: u8,
}
impl<'de> serde::Deserializer<'de> for BinarySubject<'de> {
    type Error = serde::de::value::Error;
    fn is_human_readable(&self) -> bool {
        false
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom(
            "expected the subject struct profile",
        ))
    }
    fn deserialize_struct<V: serde::de::Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        assert_eq!(name, "AccessSubject");
        assert_eq!(fields, ["kind", "id"]);
        visitor.visit_seq(self)
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    }
}
impl<'de> serde::de::SeqAccess<'de> for BinarySubject<'de> {
    type Error = serde::de::value::Error;
    fn next_element_seed<T: serde::de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        let field = self.field;
        self.field += 1;
        match field {
            0 => seed.deserialize(BinarySubjectKind(self.tag)).map(Some),
            1 => seed
                .deserialize(serde::de::value::StrDeserializer::new(self.id))
                .map(Some),
            _ => Ok(None),
        }
    }
}
struct BinarySubjectKind<'a>(BinarySubjectTag<'a>);
impl<'de> serde::Deserializer<'de> for BinarySubjectKind<'de> {
    type Error = serde::de::value::Error;
    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected the subject tag profile"))
    }
    fn deserialize_identifier<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.0 {
            BinarySubjectTag::Ordinal(ordinal) => visitor.visit_u32(ordinal),
            BinarySubjectTag::Text(text) => visitor.visit_borrowed_str(text),
            BinarySubjectTag::Bytes(bytes) => visitor.visit_borrowed_bytes(bytes),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum ignored_any
    }
}

#[test]
fn subject_binary_struct_and_identifier_tag_profile_matches_legacy_decoder() {
    use BinarySubjectTag::{Bytes, Ordinal, Text};
    for (tag, id) in [
        (Ordinal(0), "issuer#user"),
        (Ordinal(1), "engineering"),
        (Text("principal"), "issuer#user"),
        (Text("group"), "engineering"),
        (Bytes(b"principal"), "issuer#user"),
        (Bytes(b"group"), "engineering"),
        (Ordinal(0), ""),
        (Ordinal(1), "\0"),
        (Ordinal(2), "user"),
        (Text("role"), "user"),
        (Bytes(b"role"), "user"),
        (Bytes(b"\xff"), "user"),
    ] {
        let wire = || BinarySubject { tag, id, field: 0 };
        let legacy = LegacySubject::deserialize(wire()).map(AccessSubject::from);
        let current = AccessSubject::deserialize(wire());
        assert_eq!(legacy.is_ok(), matches!(id, "issuer#user" | "engineering"));
        assert_eq!(current, legacy);
    }
}
