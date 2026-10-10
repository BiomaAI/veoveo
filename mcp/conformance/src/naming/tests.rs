use super::*;
use crate::schema_evidence::{
    NamingEvidence, OwnerSchemaEvidence, SchemaObservation, ToolNameProjection,
};
use schemars::{JsonSchema, json_schema};
use serde::Serialize;
use serde_json::json;
use veoveo_types::{NamingLabel, NamingProfile, ScalarGrammar};

fn inspect(value: Value) -> Result<NamingInspection> {
    let mut result = NamingInspection::default();
    result.schema(
        "fixture",
        &Schema::try_from(value)?,
        None,
        SchemaEvidenceOrigin::Remote,
    )?;
    Ok(result)
}
fn marker(grammar: ScalarGrammar) -> Value {
    serde_json::to_value(
        NamingProfile::new(NamingRole::Scalar {
            profile: ScalarNaming::builtin(grammar),
        })
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn whole_match_lexical_classes_do_not_classify_human_annotations() {
    for text in ["a", "field1", "camelCase", "xY9"] {
        assert!(dto_field_name(text));
    }
    for text in ["_meta", "snake_case", "Upper", "x-y", "x\n", "é"] {
        assert!(!dto_field_name(text));
    }
    for text in ["read", "read_2", "a_b0"] {
        assert!(controlled_name(text));
    }
    for text in [
        "_read",
        "read_",
        "read__all",
        "read-1",
        "xY",
        "read\n",
        "1read",
    ] {
        assert!(!controlled_name(text));
    }
    inspect(json!({"type":"string","title":"Human Title", "description":"Human prose", "examples":["Human example"]})).unwrap();
}
#[test]
fn complete_graph_predicates_siblings_and_name_containers_are_checked() {
    let good = json!({"type":"object","$defs":{"id":{"type":"string"},"$id":{"type":"integer"}},
        "properties":{"localName":{"type":"string","enum":["read_all"]}},
        "if":{"required":["localName"]},"then":{"properties":{"anotherField":{"type":"integer"}}}});
    inspect(good.clone()).unwrap();
    for keyword in ["allOf", "anyOf", "oneOf"] {
        let mut v = good.clone();
        v[keyword] = json!([{"properties":{"bad_field":{"type":"string"}}}]);
        assert!(inspect(v).is_err(), "{keyword}");
    }
    for keyword in [
        "not",
        "if",
        "then",
        "else",
        "contains",
        "items",
        "additionalProperties",
        "unevaluatedProperties",
        "contentSchema",
    ] {
        let mut v = good.clone();
        v[keyword] = json!({"properties":{"bad_field":{"type":"string"}}});
        assert!(inspect(v).is_err(), "{keyword}");
    }
    let mut bad = good;
    bad["dependentRequired"] = json!({"localName":["bad_field"]});
    assert!(inspect(bad).is_err());
}
#[test]
fn local_recursive_references_keep_their_field_context_and_refuse_nonprogress() {
    inspect(json!({"type":"object","properties":{"child":{"$ref":"#"}}})).unwrap();
    assert!(inspect(json!({"$ref":"#"})).is_err());
    assert!(inspect(json!({"type":"object","$defs":{"Node":{"type":"object","properties":{"bad_field":{"type":"string"}}}}, "properties":{"child":{"$ref":"#/$defs/Node"}}})).is_err());
    for reference in ["https://foreign.example/schema", "#/$defs/missing"] {
        assert!(inspect(json!({"$ref":reference})).is_err());
    }
    for keyword in [
        "$dynamicRef",
        "$recursiveRef",
        "$dynamicAnchor",
        "$recursiveAnchor",
    ] {
        let mut v = json!({"type":"string"});
        v[keyword] = json!("#");
        assert!(inspect(v).is_err());
    }
}
#[test]
fn scalar_roles_are_narrow_and_uniform_alternatives_preserve_siblings() {
    let scope = marker(ScalarGrammar::ScopeToken);
    inspect(json!({"type":"string","enum":["owner:read"],"ai.veoveo/naming-profile":scope}))
        .unwrap();
    assert!(inspect(json!({"type":"object","properties":{"bad_field":{"type":"string"}},"ai.veoveo/naming-profile":scope})).is_err());
    inspect(json!({"enum":["owner:read"],"anyOf":[{"type":"string","ai.veoveo/naming-profile":scope},{"type":"null"}]})).unwrap();
    assert!(inspect(json!({"type":"object","properties":{"scope":{"type":"string","enum":["owner:read"],"ai.veoveo/naming-profile":scope},"bad_field":{"type":"string"}}})).is_err());
    let mut unsupported = scope.clone();
    unsupported["revision"] = json!(99);
    assert!(inspect(json!({"type":"string","ai.veoveo/naming-profile":unsupported})).is_err());
}
#[test]
fn dictionary_key_association_never_exempts_mapped_value_fields() {
    let generator = schemars::SchemaGenerator::default();
    let key = json_schema!({"type":"string","enum":["owner:read"]});
    let key =
        veoveo_types::scalar_schema(key, ScalarNaming::builtin(ScalarGrammar::ScopeToken)).unwrap();
    let map = json_schema!({"type":"object","properties":{"owner:read":{"type":"object","properties":{"camelField":{"type":"integer"}}}},"additionalProperties":false});
    let root = veoveo_types::dictionary_schema_with_key(&generator, map, key).unwrap();
    inspect(root.as_value().clone()).unwrap();
    let mut bad = root.as_value().clone();
    bad["properties"]["owner:read"]["properties"]["bad_field"] = json!({"type":"string"});
    assert!(inspect(bad).is_err());
    let mut bad = root.as_value().clone();
    bad["properties"] = json!({"other:read":{"type":"integer"}});
    assert!(inspect(bad).is_err());
    let open = veoveo_types::dictionary_schema_with_key(
        &generator,
        json_schema!({"type":"object","additionalProperties":true}),
        json_schema!({"type":"string"}),
    )
    .unwrap();
    inspect(open.as_value().clone()).unwrap();
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IndependentOwner {
    observed_count: u32,
}
#[test]
fn owner_evidence_preserves_origins_requires_real_selected_values_and_closes_schema() {
    let label = NamingLabel::new("independent owner product").unwrap();
    let source = OwnerSchemaEvidence::generated::<IndependentOwner>(label.clone());
    assert_eq!(source.origin(), SchemaEvidenceOrigin::SourceOnly);
    let bodies = [source];
    let evidence = NamingEvidence {
        required_observations: [label.clone()].into(),
        bodies: &bodies,
        ..Default::default()
    };
    assert!(!evidence.has_required_observations());
    assert!(evidence.validate().is_err());
    let observed = OwnerSchemaEvidence::generated::<IndependentOwner>(label.clone()).observe(
        SchemaObservation::from_serializable(&IndependentOwner { observed_count: 1 }).unwrap(),
    );
    let mut inspection = NamingInspection::default();
    inspection.owner(&observed).unwrap();
    assert_eq!(
        inspection.roots()[0].origin,
        SchemaEvidenceOrigin::OwnerObserved
    );
    assert_eq!(inspection.roots()[0].observations, 1);
    let wrong = OwnerSchemaEvidence::generated::<IndependentOwner>(label)
        .observe(SchemaObservation::from_serializable(&json!({"observedCount":"wrong"})).unwrap());
    assert!(NamingInspection::default().owner(&wrong).is_err());
}
#[test]
fn discovery_keeps_gateway_projection_and_prompts_independent_and_rejects_zero_evidence() {
    use rmcp::model::Tool;
    let mut tool=Tool::new("owner__read","Human description",json!({"type":"object","properties":{"fieldName":{"type":"string"}},"additionalProperties":false}).as_object().unwrap().clone());
    tool.output_schema = Some(std::sync::Arc::new(
        json!({"type":"object","properties":{"resultUri":{"type":"string"}}})
            .as_object()
            .unwrap()
            .clone(),
    ));
    assert!(
        inspect_discovery(
            &[tool.clone()],
            &[],
            &[],
            &[],
            None,
            &NamingEvidence::default()
        )
        .is_err()
    );
    let evidence = NamingEvidence {
        tool_names: ToolNameProjection::Gateway,
        ..Default::default()
    };
    let result = inspect_discovery(&[tool.clone()], &[], &[], &[], None, &evidence).unwrap();
    assert_eq!(result.roots().len(), 2);
    tool.output_schema = None;
    assert!(inspect_discovery(&[tool], &[], &[], &[], None, &evidence).is_err());
    assert!(
        inspect_discovery(&[], &[], &[], &[], None, &NamingEvidence::default())
            .unwrap()
            .roots()
            .is_empty()
    );
}
#[test]
fn schema_and_aggregate_budgets_refuse_before_unbounded_work() {
    assert!(
        inspect(json!({"description":"x".repeat(crate::tool_schema::MAX_SCHEMA_BYTES+1)})).is_err()
    );
    let mut v = json!({"type":"string"});
    for _ in 0..65 {
        v = json!({"allOf":[v]});
    }
    assert!(inspect(v).is_err());
    let mut inspection = NamingInspection {
        started: Instant::now() - NAMING_DEADLINE,
        ..Default::default()
    };
    assert!(
        inspection
            .schema(
                "late",
                &json_schema!({"type":"string"}),
                None,
                SchemaEvidenceOrigin::Remote
            )
            .is_err()
    );
    let root = json_schema!({"description":"x".repeat(800_000)});
    let mut inspection = NamingInspection::default();
    let mut rejected = false;
    for _ in 0..50 {
        if inspection
            .schema("large", &root, None, SchemaEvidenceOrigin::Remote)
            .is_err()
        {
            rejected = true;
            break;
        }
    }
    assert!(rejected);
}

#[test]
fn report_requires_selected_observations_and_keeps_mixed_source_review_honest() {
    let empty = NamingEvidence::default();
    for complete in [false, true] {
        let result = check_discovery(complete, &[], &[], &[], &[], None, &empty);
        assert_eq!(result.status, crate::CheckStatus::Incomplete);
    }
    let label = NamingLabel::new("independent-observation").unwrap();
    let source = [OwnerSchemaEvidence::generated::<IndependentOwner>(
        label.clone(),
    )];
    let evidence = NamingEvidence {
        required_observations: vec![label.clone()],
        bodies: &source,
        ..Default::default()
    };
    assert_eq!(
        check_discovery(true, &[], &[], &[], &[], None, &evidence).status,
        crate::CheckStatus::Incomplete
    );
    let observed = [
        OwnerSchemaEvidence::generated::<IndependentOwner>(label.clone()).observe(
            SchemaObservation::from_serializable(&IndependentOwner { observed_count: 7 }).unwrap(),
        ),
    ];
    let evidence = NamingEvidence {
        required_observations: vec![label],
        bodies: &observed,
        ..Default::default()
    };
    let result = check_discovery(true, &[], &[], &[], &[], None, &evidence);
    assert_eq!(result.status, crate::CheckStatus::Passed);
    let facts = result.evidence.unwrap();
    assert_eq!(facts["mode"], "mixed");
    assert_eq!(facts["outcome"], "review_required");
    assert_eq!(facts["roots"][0]["origin"], "owner_observed");
    assert!(!facts.to_string().contains("observedCount"));
}

#[test]
fn captured_generator_definitions_and_encoded_names_use_actual_owner_context() {
    #[derive(JsonSchema)]
    #[schemars(rename = "Key A%é")]
    #[allow(dead_code)]
    enum Key {
        A,
    }
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Body {
        selected_key: Key,
    }
    for path in [
        "#/$defs/",
        "#/components/schemas/",
        "#/components/id/owner~1schemas/",
    ] {
        let mut settings = schemars::generate::SchemaSettings::default();
        settings.definitions_path = path.into();
        let schema = settings.into_generator().into_root_schema_for::<Body>();
        // Generated enum controlled values must follow the owner's declared wire.
        assert!(
            NamingInspection::default()
                .schema(
                    "owner",
                    &schema,
                    Some(path),
                    SchemaEvidenceOrigin::SourceOnly
                )
                .is_err()
        );
        #[derive(JsonSchema)]
        #[schemars(rename = "Key A%é")]
        #[serde(rename_all = "snake_case")]
        #[allow(dead_code)]
        enum GoodKey {
            A,
        }
        #[derive(JsonSchema)]
        #[serde(rename_all = "camelCase")]
        #[allow(dead_code)]
        struct GoodBody {
            selected_key: GoodKey,
        }
        let mut settings = schemars::generate::SchemaSettings::default();
        settings.definitions_path = path.into();
        let schema = settings.into_generator().into_root_schema_for::<GoodBody>();
        NamingInspection::default()
            .schema(
                "owner",
                &schema,
                Some(path),
                SchemaEvidenceOrigin::SourceOnly,
            )
            .unwrap();
    }
}

#[test]
fn adopted_tool_metadata_and_unqualified_prompt_names_are_inspected() {
    use rmcp::model::{Prompt, Tool};
    let mut tool = Tool::new(
        "read",
        "Human",
        json!({"type":"object","additionalProperties":false})
            .as_object()
            .unwrap()
            .clone(),
    );
    tool.output_schema = Some(std::sync::Arc::new(
        json!({"type":"object","properties":{"resultUri":{"type":"string"}}})
            .as_object()
            .unwrap()
            .clone(),
    ));
    tool.meta = Some(json!({"ui":{"resourceUri":"ui://owner/view","visibility":["app"]},"ai.vendor/opaque":{"external_field":true}}).as_object().unwrap().clone().into());
    let prompt = Prompt::new("read_summary", Some("Human"), None);
    inspect_discovery(
        &[tool.clone()],
        &[],
        &[],
        std::slice::from_ref(&prompt),
        None,
        &NamingEvidence::default(),
    )
    .unwrap();
    let mut wrong = prompt;
    wrong.name = "owner__read_summary".into();
    assert!(
        inspect_discovery(
            &[tool.clone()],
            &[],
            &[],
            &[wrong],
            None,
            &NamingEvidence::default()
        )
        .is_err()
    );
    tool.meta = Some(
        json!({"ui":{"resourceUri":"ui://owner/view","visibility":["Upper"]}})
            .as_object()
            .unwrap()
            .clone()
            .into(),
    );
    assert!(inspect_discovery(&[tool], &[], &[], &[], None, &NamingEvidence::default()).is_err());
}

#[test]
fn owning_catalog_and_profile_schemas_inspect_without_exempting_declaration_dtos() {
    for schema in [
        schemars::schema_for!(veoveo_mcp_contract::docs::catalog::RequirementCatalog),
        schemars::schema_for!(veoveo_mcp_contract::docs::ComplianceProfile),
        crate::hosted_server_conformance_profile_schema(),
    ] {
        let mut inspection = NamingInspection::default();
        inspection
            .schema(
                "declaration",
                &schema,
                None,
                SchemaEvidenceOrigin::SourceOnly,
            )
            .unwrap();
        assert!(inspection.roots()[0].source_review_required);
        let mut wrong = schema;
        wrong.insert(
            "properties".into(),
            json!({"wrong_field":{"type":"string"}}),
        );
        assert!(
            NamingInspection::default()
                .schema(
                    "declaration",
                    &wrong,
                    None,
                    SchemaEvidenceOrigin::SourceOnly
                )
                .is_err()
        );
    }
}

#[test]
fn actual_schema_reference_branch_node_and_aggregate_root_limits_are_enforced() {
    let many_nodes = json!({"examples":[vec![false; crate::tool_schema::MAX_SCHEMA_NODES]]});
    assert!(
        inspect(many_nodes)
            .err()
            .expect("schema must reject")
            .to_string()
            .contains("node count")
    );
    let branches =
        json!({"allOf":vec![json!({"type":"string"});crate::tool_schema::MAX_SCHEMA_BRANCHES+1]});
    assert!(
        inspect(branches)
            .err()
            .expect("schema must reject")
            .to_string()
            .contains("composition branch")
    );
    let fields: serde_json::Map<String, Value> = (0..=crate::tool_schema::MAX_SCHEMA_REFERENCES)
        .map(|i| (format!("field{i}"), json!({"$ref":"#/$defs/Scalar"})))
        .collect();
    assert!(
        inspect(json!({"type":"object","properties":fields,"$defs":{"Scalar":{"type":"string"}}}))
            .err()
            .expect("schema must reject")
            .to_string()
            .contains("reference count")
    );
    let mut inspection = NamingInspection::default();
    let scalar = json_schema!({"type":"string"});
    for _ in 0..MAX_NAMING_ROOTS {
        inspection
            .schema("scalar", &scalar, None, SchemaEvidenceOrigin::Remote)
            .unwrap();
    }
    assert!(
        inspection
            .schema("overflow", &scalar, None, SchemaEvidenceOrigin::Remote)
            .expect_err("schema must reject")
            .to_string()
            .contains("aggregate roots")
    );
}

#[test]
fn declared_subtree_exceptions_preserve_surrounding_fields_and_reference_limits() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("fixture-owner").unwrap(),
        },
        profile: NamingLabel::new("upstream-property-shape").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("the explicitly selected upstream subtree").unwrap(),
    };
    for role in [
        NamingRole::Jwt {
            declaration: declaration.clone(),
        },
        NamingRole::Frozen {
            declaration: declaration.clone(),
        },
        NamingRole::External {
            declaration: declaration.clone(),
        },
    ] {
        let marker = serde_json::to_value(NamingProfile::new(role).unwrap()).unwrap();
        let external = json!({"type":"object","properties":{"external_field":{"type":"string","enum":["ExternalValue"]}},"ai.veoveo/naming-profile":marker});
        let result = inspect(external.clone()).unwrap();
        assert!(result.roots()[0].source_review_required);
        inspect(json!({"type":"object","properties":{"externalBody":external.clone(),"camelField":{"type":"string"}}})).unwrap();
        assert!(inspect(json!({"type":"object","properties":{"externalBody":external.clone(),"bad_field":{"type":"string"}}})).is_err());
        assert!(inspect(json!({"anyOf":[external.clone(),{"type":"object","properties":{"bad_field":{"type":"string"}}}]})).is_err());
        let mut foreign = external;
        foreign["$ref"] = json!("https://external.example/schema");
        assert!(inspect(foreign).is_err());
    }
}

#[test]
fn exception_edges_never_mask_unclassified_siblings_or_second_uses() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("external owner").unwrap(),
        },
        profile: NamingLabel::new("external subtree").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("external subtree").unwrap(),
    };
    let profile =
        serde_json::to_value(NamingProfile::new(NamingRole::External { declaration }).unwrap())
            .unwrap();
    let external = json!({"type":"object","properties":{"external_field":{"type":"integer"}},"ai.veoveo/naming-profile":profile});
    for keyword in ["allOf", "anyOf", "oneOf"] {
        let positive =
            json!({keyword:[external.clone(),{"properties":{"localField":{"type":"integer"}}}]});
        inspect(positive).unwrap();
        assert!(
            inspect(
                json!({keyword:[external.clone(),{"properties":{"bad_field":{"type":"integer"}}}]})
            )
            .is_err(),
            "{keyword}"
        );
    }
    for keyword in ["if", "then", "else", "not"] {
        assert!(inspect(json!({"$defs":{"External":external.clone()},"$ref":"#/$defs/External",keyword:{"properties":{"bad_field":{"type":"integer"}}}})).is_err(), "{keyword}");
    }
    inspect(json!({"$defs":{"External":external.clone()},"$ref":"#/$defs/External","properties":{"localField":{"type":"integer"}}})).unwrap();
    assert!(inspect(json!({"$defs":{"External":external},"$ref":"#/$defs/External","properties":{"bad_field":{"type":"integer"}}})).is_err());
    let classified =
        json!({"$ref":"#/$defs/Raw","ai.veoveo/naming-profile":marker(ScalarGrammar::ScopeToken)});
    let raw = json!({"type":"string","enum":["owner:read"]});
    inspect(json!({"type":"object","$defs":{"Raw":raw.clone()},"properties":{"scope":classified.clone()}})).unwrap();
    assert!(inspect(json!({"type":"object","$defs":{"Raw":raw.clone()},"properties":{"scope":classified,"other":{"$ref":"#/$defs/Raw"}}})).is_err());
    assert!(inspect(json!({"type":"object","$defs":{"Unused":raw}})).is_err());
}

#[test]
fn inherited_dictionary_keys_and_controlled_object_literals_use_actual_roles() {
    let generator = schemars::SchemaGenerator::default();
    let key = veoveo_types::scalar_schema(
        json_schema!({"type":"string","enum":["owner:read"]}),
        ScalarNaming::builtin(ScalarGrammar::ScopeToken),
    )
    .unwrap();
    let map = veoveo_types::dictionary_schema_with_key(&generator, json_schema!({"type":"object","properties":{"owner:read":{"type":"integer"}},"additionalProperties":false}), key).unwrap();
    let base = json!({"$defs":{"Map":map},"$ref":"#/$defs/Map","required":["owner:read"]});
    inspect(base.clone()).unwrap();
    for keyword in ["properties", "dependentSchemas"] {
        let mut bad = base.clone();
        bad[keyword] = json!({"other:read":{"type":"integer"}});
        assert!(inspect(bad).is_err(), "{keyword}");
    }
    let mut bad = base;
    bad["required"] = json!(["other:read"]);
    assert!(inspect(bad).is_err());
    for keyword in ["const", "enum"] {
        let value = if keyword == "enum" {
            json!([{ "bad_field": 1 }])
        } else {
            json!({ "bad_field": 1 })
        };
        assert!(
            inspect(json!({"type":"object",keyword:value})).is_err(),
            "{keyword}"
        );
        let value = if keyword == "enum" {
            json!([{ "localField": [{ "nestedField":1 }] }])
        } else {
            json!({ "localField": [{ "nestedField":1 }] })
        };
        inspect(json!({"type":"object",keyword:value})).unwrap();
    }
    assert!(
        inspect(json!({"type":"object","dependentSchemas":{"bad_field":{"type":"object"}}}))
            .is_err()
    );
}

#[test]
fn adopted_metadata_visits_actual_knowledge_apps_and_capability_producers() {
    use rmcp::model::{Resource, ResourceTemplate, Tool};
    let mut tool = Tool::new(
        "search",
        "Search",
        serde_json::from_value::<rmcp::model::JsonObject>(
            json!({"type":"object","additionalProperties":false}),
        )
        .unwrap(),
    );
    tool.output_schema = Some(std::sync::Arc::new(
        serde_json::from_value::<rmcp::model::JsonObject>(
            json!({"type":"object","additionalProperties":false}),
        )
        .unwrap(),
    ));
    let collection = veoveo_mcp_knowledge_extension::docs::collection(
        &veoveo_types::ServerSlug::parse("independent").unwrap(),
        &veoveo_types::ResourceScheme::parse("example").unwrap(),
    );
    veoveo_mcp_knowledge_extension::server::attach_search(
        &mut tool,
        &veoveo_mcp_knowledge_extension::SearchDeclaration::new(vec![
            collection.collection().clone(),
        ])
        .unwrap(),
    );
    let mut template = ResourceTemplate::new("example://docs/{document}", "Documents");
    veoveo_mcp_knowledge_extension::server::attach_collection(&mut template, &collection);
    let mut resource = Resource::new("example://app/main", "App");
    let dependencies: Vec<veoveo_gateway_contract::AppResourceDependency> = serde_json::from_value(json!([{
        "appResource":"example://app/main", "server":"independent", "scheme":"example", "uriPrefix":"example://docs/", "requiredScope":"owner:read", "operations":["read"]
    }])).unwrap();
    resource.meta.get_or_insert_default().insert(
        veoveo_gateway_contract::APP_RESOURCE_DEPENDENCIES_META_KEY.into(),
        serde_json::to_value(&dependencies).unwrap(),
    );
    let (key, declaration) = veoveo_mcp_apps_extension::host_extension_capability();
    let capabilities = [(key, declaration)].into_iter().collect();
    let evidence = NamingEvidence::default();
    inspect_discovery(
        &[tool.clone()],
        &[resource.clone()],
        &[template.clone()],
        &[],
        Some(&capabilities),
        &evidence,
    )
    .unwrap();
    let mut bad = template.clone();
    bad.meta.as_mut().unwrap()[veoveo_mcp_knowledge_extension::EXTENSION_ID]["changeSignal"] =
        json!("invented");
    assert!(inspect_discovery(&[tool.clone()], &[], &[bad], &[], None, &evidence).is_err());
    let mut bad = tool.clone();
    bad.meta.as_mut().unwrap()[veoveo_mcp_knowledge_extension::EXTENSION_ID]["role"] =
        json!("Search");
    assert!(inspect_discovery(&[bad], &[], &[], &[], None, &evidence).is_err());
    let mut bad = resource;
    bad.meta.as_mut().unwrap()[veoveo_gateway_contract::APP_RESOURCE_DEPENDENCIES_META_KEY][0]["operations"] =
        json!(["Read"]);
    assert!(inspect_discovery(&[], &[bad], &[], &[], None, &evidence).is_err());
    let mut bad = template;
    let metadata = bad
        .meta
        .as_mut()
        .unwrap()
        .get_mut(veoveo_mcp_knowledge_extension::EXTENSION_ID)
        .unwrap()
        .as_object_mut()
        .unwrap();
    let signal = metadata.remove("changeSignal").unwrap();
    metadata.insert("change_signal".into(), signal);
    assert!(inspect_discovery(&[], &[], &[bad], &[], None, &evidence).is_err());
    tool.meta.as_mut().unwrap().insert(
        "com.vendor/open".into(),
        json!({"opaque_vendor_field":"OpenValue"}),
    );
    inspect_discovery(&[tool], &[], &[], &[], None, &evidence).unwrap();
}

#[test]
fn selected_observations_and_discovery_are_charged_to_the_original_budget() {
    #[derive(Serialize, JsonSchema)]
    struct Body {
        values: Vec<u64>,
    }
    let body = OwnerSchemaEvidence::generated::<Body>(NamingLabel::new("body").unwrap())
        .observe(SchemaObservation::from_serializable(&Body { values: vec![1, 2] }).unwrap())
        .observe(SchemaObservation::from_serializable(&Body { values: vec![3, 4] }).unwrap());
    let mut inspection = NamingInspection::default();
    inspection.owner(&body).unwrap();
    assert_eq!(inspection.roots()[0].observations, 2);
    let large = OwnerSchemaEvidence::generated::<Body>(NamingLabel::new("large").unwrap()).observe(
        SchemaObservation::from_serializable(&Body {
            values: vec![1; MAX_NAMING_WORK],
        })
        .unwrap(),
    );
    assert!(NamingInspection::default().owner(&large).is_err());
    let mut deep = json!(1);
    for _ in 0..65 {
        deep = json!([deep]);
    }
    assert!(NamingInspection::default().value_work(&deep, 0).is_err());
    let mut nearly_finished = NamingInspection {
        work: MAX_NAMING_WORK - 3,
        ..Default::default()
    };
    assert!(nearly_finished.owner(&body).is_err());
    let empty = NamingEvidence::default();
    let late = super::discovery::inspect_discovery_since(
        Instant::now() - NAMING_DEADLINE,
        &[],
        &[],
        &[],
        &[],
        None,
        &empty,
    );
    assert!(late.is_err());
    let body = [body];
    let many = NamingEvidence {
        required_observations: (0..=MAX_NAMING_ROOTS)
            .map(|i| NamingLabel::new(format!("body{i}")).unwrap())
            .collect(),
        bodies: &body,
        ..Default::default()
    };
    assert!(inspect_discovery(&[], &[], &[], &[], None, &many).is_err());
    let mut metadata = json!(1);
    for _ in 0..65 {
        metadata = json!([metadata]);
    }
    let mut resource = rmcp::model::Resource::new("example://body", "Body");
    resource
        .meta
        .get_or_insert_default()
        .insert("com.vendor/open".into(), metadata);
    assert!(inspect_discovery(&[], &[resource], &[], &[], None, &empty).is_err());
    let report = schemars::schema_for!(crate::ConformanceReport);
    NamingInspection::default()
        .schema("report", &report, None, SchemaEvidenceOrigin::SourceOnly)
        .unwrap();
}

#[test]
fn definition_use_contexts_cover_draft7_custom_paths_and_captured_key_references() {
    for (document, path, reference) in [
        (
            json!({"$defs":{"Raw":{"type":"string","enum":["owner:read"]}}}),
            "#/$defs/",
            "#/$defs/Raw",
        ),
        (
            json!({"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"Raw":{"type":"string","enum":["owner:read"]}}}),
            "#/definitions/",
            "#/definitions/Raw",
        ),
        (
            json!({"components":{"owner/schemas":{"Raw":{"type":"string","enum":["owner:read"]}}}}),
            "#/components/owner~1schemas/",
            "#/components/owner~1schemas/Raw",
        ),
    ] {
        let scope =
            json!({"$ref":reference,"ai.veoveo/naming-profile":marker(ScalarGrammar::ScopeToken)});
        let mut positive = document.clone();
        positive["type"] = json!("object");
        positive["properties"] = json!({"scope":scope.clone()});
        NamingInspection::default()
            .schema(
                "classified",
                &Schema::try_from(positive.clone()).unwrap(),
                Some(path),
                SchemaEvidenceOrigin::Remote,
            )
            .unwrap();
        positive["properties"]["plain"] = json!({"$ref":reference});
        assert!(
            NamingInspection::default()
                .schema(
                    "second use",
                    &Schema::try_from(positive).unwrap(),
                    Some(path),
                    SchemaEvidenceOrigin::Remote
                )
                .is_err(),
            "{path}"
        );
        assert!(
            NamingInspection::default()
                .schema(
                    "unused",
                    &Schema::try_from(document.clone()).unwrap(),
                    Some(path),
                    SchemaEvidenceOrigin::Remote
                )
                .is_err(),
            "{path}"
        );
        let mut dictionary = document;
        dictionary["type"] = json!("object");
        dictionary["properties"] =
            json!({"owner:read":{"type":"object","properties":{"localField":{"type":"integer"}}}});
        dictionary["additionalProperties"] = json!(false);
        dictionary[veoveo_types::naming::NAMING_PROFILE_KEY] = serde_json::to_value(
            NamingProfile::new(NamingRole::Dictionary {
                key_schema: Schema::try_from(scope).unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
        NamingInspection::default()
            .schema(
                "captured key",
                &Schema::try_from(dictionary.clone()).unwrap(),
                Some(path),
                SchemaEvidenceOrigin::Remote,
            )
            .unwrap();
        dictionary["properties"]["owner:read"]["properties"] =
            json!({"bad_field":{"type":"integer"}});
        assert!(
            NamingInspection::default()
                .schema(
                    "mapped DTO",
                    &Schema::try_from(dictionary).unwrap(),
                    Some(path),
                    SchemaEvidenceOrigin::Remote
                )
                .is_err(),
            "{path}"
        );
    }
    let open = json!({"$defs":{"Map":{"type":"object","additionalProperties":true,"ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(NamingRole::Dictionary { key_schema: json_schema!({"type":"string"}) }).unwrap()).unwrap()}},"$ref":"#/$defs/Map","const":{"user-key":{"provider_payload":1}}});
    inspect(open).unwrap();
    let provider = json!({"type":"object","properties":{"providerPayload":{}},"const":{"providerPayload":{"provider_key":1}}});
    inspect(provider).unwrap();
}

#[test]
fn same_instance_dictionary_and_nested_literal_roles_remain_local() {
    let generator = schemars::SchemaGenerator::default();
    let key = veoveo_types::scalar_schema(
        json_schema!({"type":"string","enum":["owner:read"]}),
        ScalarNaming::builtin(ScalarGrammar::ScopeToken),
    )
    .unwrap();
    let map = veoveo_types::dictionary_schema_with_key(&generator, json_schema!({"type":"object","allOf":[{"type":"object","properties":{"owner:read":{"type":"integer"}},"additionalProperties":false}]}), key).unwrap();
    inspect(serde_json::to_value(&map).unwrap()).unwrap();
    let mut graph = json!({"type":"object","$defs":{"Map":map},"properties":{"values":{"$ref":"#/$defs/Map"}},"const":{"values":{"owner:read":1}}});
    inspect(graph.clone()).unwrap();
    graph["const"]["values"] = json!({"foreign:read":1});
    assert!(inspect(graph).is_err());
    assert!(inspect(json!({"type":"object","properties":{"state":{"type":"string"}},"const":{"state":"bad-value"}})).is_err());
    inspect(json!({"type":"object","properties":{"payload":true},"const":{"payload":{"external_field":"bad-value"}}})).unwrap();
    let mut result = NamingInspection::default();
    let mut baseline = NamingInspection::default();
    baseline
        .schema(
            "budget",
            &json_schema!({"type":"object","dependentRequired":{"valid":[]}}),
            None,
            SchemaEvidenceOrigin::Remote,
        )
        .unwrap();
    result.work = MAX_NAMING_WORK - baseline.work;
    assert!(result.schema("budget", &json_schema!({"type":"object","dependentRequired":{"valid":["first","second","third","fourth"]}}), None, SchemaEvidenceOrigin::Remote).is_err());
}

#[test]
fn nested_external_literal_and_scalar_reference_preserve_owner_roles() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("external owner").unwrap(),
        },
        profile: NamingLabel::new("external subtree").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("external subtree").unwrap(),
    };
    let marker =
        serde_json::to_value(NamingProfile::new(NamingRole::External { declaration }).unwrap())
            .unwrap();
    inspect(json!({"type":"object","properties":{"payload":{"type":"object","properties":{"external_field":{"type":"integer"}},"ai.veoveo/naming-profile":marker}},"const":{"payload":{"external_field":1}}})).unwrap();
    let scope = marker_for_scope();
    inspect(json!({"type":"object","$defs":{"Scope":scope},"properties":{"scope":{"$ref":"#/$defs/Scope"}},"const":{"scope":"owner:read"}})).unwrap();
}
fn marker_for_scope() -> Value {
    json!({"type":"string","enum":["owner:read"],"ai.veoveo/naming-profile":marker(ScalarGrammar::ScopeToken)})
}

#[test]
fn literal_constraint_sources_do_not_reclassify_ref_arrays_or_foreign_siblings() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("external owner").unwrap(),
        },
        profile: NamingLabel::new("external subtree").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("external subtree").unwrap(),
    };
    let external = json!({"type":"object","properties":{"external_field":{"type":"integer"}},"ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(NamingRole::External { declaration }).unwrap()).unwrap()});
    for wrapper in [
        json!({"$ref":"#/$defs/Scopes"}),
        json!({"allOf":[{"$ref":"#/$defs/Scopes"}]}),
        json!({"anyOf":[{"$ref":"#/$defs/Scopes"},{"type":"null"}]}),
        json!({"oneOf":[{"$ref":"#/$defs/Scopes"},{"type":"null"}]}),
    ] {
        inspect(json!({"type":"object","$defs":{"Scopes":{"type":"array","items":marker_for_scope()}},"properties":{"scopes":wrapper},"const":{"scopes":["owner:read"]}})).unwrap();
    }
    inspect(json!({"$defs":{"External":external},"$ref":"#/$defs/External","properties":{"localField":{"type":"integer"}},"const":{"external_field":1,"localField":1}})).unwrap();
    assert!(inspect(json!({"type":"object","properties":{"localField":{"type":"array","items":{"type":"object","properties":{"bad_field":{"type":"integer"}}}}},"const":{"localField":[{"bad_field":1}]}})).is_err());
}

#[test]
fn productive_recursive_dictionary_memo_uses_captured_role_identity() {
    let profile = serde_json::to_value(
        NamingProfile::new(NamingRole::Dictionary {
            key_schema: json_schema!({"type":"string"}),
        })
        .unwrap(),
    )
    .unwrap();
    let mut graph = json!({"$defs":{"Map":{"type":"object","additionalProperties":{"$ref":"#/$defs/Map"},"ai.veoveo/naming-profile":profile}},"$ref":"#/$defs/Map"});
    inspect(graph.clone()).unwrap();
    graph["$defs"]["Map"]["additionalProperties"] =
        json!({"type":"object","properties":{"bad_field":{"type":"integer"}}});
    assert!(inspect(graph).is_err());
    assert!(
        inspect(json!({"$defs":{"Loop":{"$ref":"#/$defs/Loop"}},"$ref":"#/$defs/Loop"})).is_err()
    );
}

#[test]
fn complete_literal_constraint_source_matrix_uses_maintained_applicability() {
    let scope = marker_for_scope();
    let key = json!({"type":"string","pattern":"^[a-z]+:read$","ai.veoveo/naming-profile":marker(ScalarGrammar::ScopeToken)});
    let dictionary = json!({"type":"object","patternProperties":{"^[a-z]+:read$":scope.clone()},"additionalProperties":false,"ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(NamingRole::Dictionary { key_schema: Schema::try_from(key).unwrap() }).unwrap()).unwrap()});
    inspect(json!({"type":"object","$defs":{"Map":dictionary},"properties":{"values":{"$ref":"#/$defs/Map"}},"const":{"values":{"owner:read":"owner:read"}}})).unwrap();
    let then = json!({"type":"object","properties":{"kind":{"type":"string"},"payload":{"type":"string"}},"if":{"properties":{"kind":{"const":"selected"}}},"then":{"properties":{"payload":scope.clone()}},"else":{"properties":{"payload":{"type":"string","enum":["ordinary"]}}},"const":{"kind":"selected","payload":"owner:read"}});
    inspect(then.clone()).unwrap();
    let mut other = then;
    other["const"] = json!({"kind":"other","payload":"ordinary"});
    inspect(other.clone()).unwrap();
    other["const"]["payload"] = json!("owner:read");
    assert!(inspect(other).is_err());
    for keyword in ["anyOf", "oneOf"] {
        assert!(inspect(json!({keyword:[{"type":"object","required":["valid"],"properties":{"valid":{"type":"integer"}}},{"type":"null"}],"const":{"bad_field":1}})).is_err());
    }
    for keyword in ["dependentSchemas", "dependencies"] {
        let mut graph = json!({"type":"object","properties":{"trigger":{"type":"boolean"},"payload":{"type":"string"}},keyword:{"trigger":{"properties":{"payload":scope.clone()}}},"const":{"trigger":true,"payload":"owner:read"}});
        if keyword == "dependencies" {
            graph["$schema"] = json!("http://json-schema.org/draft-07/schema#");
        }
        inspect(graph).unwrap();
    }
    for array in [
        json!({"type":"array","prefixItems":[scope.clone()],"items":false}),
        json!({"type":"array","contains":scope.clone(),"unevaluatedItems":false}),
        json!({"$schema":"http://json-schema.org/draft-07/schema#","type":"array","items":[scope.clone()],"additionalItems":false}),
    ] {
        let mut graph = array;
        graph["const"] = json!(["owner:read"]);
        inspect(graph.clone()).unwrap();
        graph["const"] = json!(["bad-value"]);
        assert!(inspect(graph).is_err());
    }
    let composed = json!({"type":"object","allOf":[{"properties":{"payload":scope.clone()}}],"unevaluatedProperties":false,"const":{"payload":"owner:read"}});
    inspect(composed).unwrap();
    // A matching pattern and an explicit property both constrain the value.
    assert!(inspect(json!({"type":"object","properties":{"payload":{"type":"string"}},"patternProperties":{"^payload$":{"type":"string"}},"const":{"payload":"bad-value"}})).is_err());
}

#[test]
fn literal_full_applicator_roles_footprints_and_legacy_guards() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("external owner").unwrap(),
        },
        profile: NamingLabel::new("selected upstream subtree").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("the declared payload").unwrap(),
    };
    let scope = marker_for_scope();
    for role in [
        NamingRole::External {
            declaration: declaration.clone(),
        },
        NamingRole::Jwt {
            declaration: declaration.clone(),
        },
        NamingRole::Frozen {
            declaration: declaration.clone(),
        },
    ] {
        let external = json!({"type":"object","required":["external_field"],"properties":{"external_field":{"type":"string"}},"ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(role).unwrap()).unwrap()});
        for wrapper in [
            json!({"$ref":"#/$defs/Array"}),
            json!({"allOf":[{"$ref":"#/$defs/Array"}]}),
            json!({"anyOf":[{"$ref":"#/$defs/Array"},{"type":"null"}]}),
            json!({"oneOf":[{"$ref":"#/$defs/Array"},{"type":"null"}]}),
        ] {
            inspect(json!({"$defs":{"Array":{"type":"array","items":external.clone()}},"allOf":[wrapper],"const":[{"external_field":"ExternalValue"}]})).unwrap();
        }
        let mut prefix = json!({"type":"array","prefixItems":[external.clone()],"items":{"type":"string"},"const":[{"external_field":"ExternalValue"},"ordinary"]});
        inspect(prefix.clone()).unwrap();
        prefix["const"][1] = json!("bad-value");
        assert!(inspect(prefix).is_err());
        let contains = json!({"type":"array","contains":external.clone(),"minContains":1,"maxContains":1,"const":[{"external_field":"ExternalValue"},"ordinary"]});
        inspect(contains.clone()).unwrap();
        let mut bad = contains;
        bad["const"][1] = json!({"bad_field":1});
        assert!(inspect(bad).is_err());
        let content = json!({"type":"string","contentSchema":external.clone(),"const":"ordinary"});
        inspect(content).unwrap();
        assert!(
            inspect(json!({"type":"string","contentSchema":external,"const":"ExternalValue"}))
                .is_err()
        );
    }
    let remaining = json!({"type":"object","allOf":[{"properties":{"known":{"type":"integer"}}}],"unevaluatedProperties":scope.clone(),"const":{"known":1,"leftover":"owner:read"}});
    inspect(remaining).unwrap();
    let remaining_items = json!({"type":"array","anyOf":[{"prefixItems":[{"type":"integer","const":999}]},{"type":"array"}],"unevaluatedItems":scope.clone(),"const":["owner:read"]});
    inspect(remaining_items).unwrap();
    let dependency = json!({"type":"object","properties":{"trigger":{"type":"boolean"}},"dependentSchemas":{"trigger":{"properties":{"payload":scope.clone()}}},"unevaluatedProperties":{"type":"string"},"const":{"trigger":true,"payload":"owner:read"}});
    inspect(dependency.clone()).unwrap();
    let mut inactive = dependency;
    inactive["const"] = json!({"payload":"owner:read"});
    assert!(inspect(inactive).is_err());
    let predicate = json!({"type":"object","not":{"type":"object","required":["external_field"],"ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(NamingRole::External { declaration }).unwrap()).unwrap()},"const":{"localField":1}});
    inspect(predicate.clone()).unwrap();
    let mut bad = predicate;
    bad["const"] = json!({"bad_field":1});
    assert!(inspect(bad).is_err());
    let named = json!({"type":"object","propertyNames":{"type":"string","enum":["localField"]},"additionalProperties":{"type":"string"},"const":{"localField":"ordinary"}});
    inspect(named.clone()).unwrap();
    let mut bad = named;
    bad["const"]["localField"] = json!("owner:read");
    assert!(inspect(bad).is_err());
    for legacy in [
        json!({"dependencies":{"trigger":{"$ref":"https://foreign.example/schema"}}}),
        json!({"dependencies":{"trigger":{"$id":"https://foreign.example/schema","type":"object"}}}),
        json!({"items":[{"type":"integer"}],"additionalItems":{"$ref":"https://foreign.example/schema"}}),
    ] {
        let mut graph = legacy;
        graph["$schema"] = json!("http://json-schema.org/draft-07/schema#");
        assert!(inspect(graph).is_err());
    }
    assert!(inspect(json!({"dependencies":{"trigger":{"type":"object"}}})).is_err());
    assert!(inspect(json!({"$schema":"http://json-schema.org/draft-07/schema#","prefixItems":[{"type":"string"}]})).is_err());
    let fake = json!({"type":"object","properties":{"providerPayload":true},"const":{"providerPayload":{"$ref":"https://foreign.example/schema","ai.veoveo/naming-profile":{"kind":"made_up"}}}});
    inspect(fake).unwrap();
    assert!(
        inspect(json!({"type":"object","const":{"$ref":"https://foreign.example/schema"}}))
            .is_err()
    );
    let mut exhausted = NamingInspection {
        started: Instant::now() - NAMING_DEADLINE,
        ..Default::default()
    };
    assert!(
        exhausted
            .schema(
                "expired",
                &Schema::try_from(json!({"type":"array","contains":scope,"const":["owner:read"]}))
                    .unwrap(),
                None,
                SchemaEvidenceOrigin::Remote
            )
            .is_err()
    );
}

#[test]
fn detached_unevaluated_item_refs_and_indexed_large_footprints_remain_bounded() {
    let scope =
        json!({"type":"string","ai.veoveo/naming-profile":marker(ScalarGrammar::ScopeToken)});
    inspect(json!({"$defs":{"Scope":scope},"type":"object","properties":{"payload":{"type":"array","unevaluatedItems":{"$ref":"#/$defs/Scope"}}},"const":{"payload":["owner:read"]}})).unwrap();
    let values: serde_json::Map<String, Value> =
        (0..10000).map(|i| (format!("a{i}"), json!(i))).collect();
    let result =
        inspect(json!({"type":"object","unevaluatedProperties":{"type":"integer"},"const":values}))
            .unwrap();
    assert!(result.work < MAX_NAMING_WORK / 2);
    let result = inspect(json!({"type":"array","unevaluatedItems":{"type":"integer"},"const":(0..10000).collect::<Vec<_>>()})).unwrap();
    assert!(result.work < MAX_NAMING_WORK / 2);
}
#[test]
fn draft7_ignored_ref_siblings_cannot_supply_payload_exemptions() {
    let declaration = veoveo_types::NamingDeclaration {
        authority: veoveo_types::NamingAuthority::Owner {
            module: NamingLabel::new("fixture-owner").unwrap(),
        },
        profile: NamingLabel::new("upstream-shape").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("selected upstream subtree").unwrap(),
    };
    for role in [
        NamingRole::External {
            declaration: declaration.clone(),
        },
        NamingRole::Frozen {
            declaration: declaration.clone(),
        },
        NamingRole::Jwt { declaration },
    ] {
        let ext = json!({"type":"object","ai.veoveo/naming-profile":serde_json::to_value(NamingProfile::new(role).unwrap()).unwrap()});
        let mut graph = json!({"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"Base":{"type":"object","properties":{"payload":{"type":"object"}}}},"type":"object","properties":{"value":{"$ref":"#/definitions/Base","allOf":[{"properties":{"payload":ext}}]}},"const":{"value":{"payload":{"bad_field":1}}}});
        assert!(inspect(graph.clone()).is_err());
        for uri in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
            "https://json-schema.org/draft/2019-09/schema",
        ] {
            graph["$schema"] = json!(uri);
            assert!(inspect(graph.clone()).is_err());
        }
        graph["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
        inspect(graph).unwrap();
    }
}

#[test]
fn nested_schema_dialect_rebasing_refuses_before_literal_preparation() {
    assert!(inspect(json!({"type":"object","properties":{"payload":{"$schema":"http://json-schema.org/draft-07/schema#","type":"object"}}})).is_err());
}
