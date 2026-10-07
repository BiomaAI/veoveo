use veoveo_knowledge_contract::ChunkSettings;
use veoveo_knowledge_mcp::{
    chunk,
    source::{SourcePage, enumeration_uri},
};
use veoveo_mcp_knowledge_extension::*;
use veoveo_types::ResourceTemplateUri;

#[test]
fn structure_unicode_and_overlap_preserve_source_text_within_embedding_bounds() {
    let settings = ChunkSettings::new("structure-v1", 8192, 8000).unwrap();
    let text = "🦀".repeat(16_384);
    // A nearly complete overlap would exceed 256 chunks: fail the member whole.
    assert!(chunk::ranges(&text, &settings).is_err());
    let settings = ChunkSettings::new("structure-v1", 8192, 100).unwrap();
    let ranges = chunk::ranges(&text, &settings).unwrap();
    assert!(
        ranges
            .iter()
            .all(|r| r.len() <= 16 * 1024 && text.get(r.clone()).is_some())
    );
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, text.len());
    assert!(
        ranges
            .windows(2)
            .all(|p| p[0].end > p[1].start && p[0].start < p[1].start)
    );

    let text = "# First\nαβγ\n# Second\nExample\n";
    let parts = chunk::ranges(text, &settings).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(text[parts[1].clone()].starts_with("# Second"));
    let text = r#"{"one":{"nested":"a,b,\"c"},"two":[1,2],"three":"空"}"#;
    let parts = chunk::ranges(text, &settings).unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts.iter().map(|r| &text[r.clone()]).collect::<String>(),
        text
    );
    assert!(text[parts[1].clone()].starts_with("\"two\":"));
}

#[test]
fn enumeration_checks_duplicates_and_escapes_opaque_cursors() {
    assert!(
        serde_json::from_value::<SourcePage>(serde_json::json!({"items":[],"nextCursor":"again"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<SourcePage>(
            serde_json::json!({"items":[{"uri":"map://a"},{"uri":"map://a"}]})
        )
        .is_err()
    );
    let descriptor = |template| {
        CollectionDescriptor::new(
            "fixture.records".parse().unwrap(),
            "record".parse().unwrap(),
            ResourceTemplateUri::new(template).unwrap(),
            Freshness::immutable(),
            ChangeSignal::Immutable,
            AccessModel::Profile,
            IndexingMode::Content,
        )
        .unwrap()
    };
    for template in [
        "fixture://records{?cursor}",
        "fixture://records",
        "fixture://records?kind=report",
    ] {
        let uri = enumeration_uri(&descriptor(template), Some("a+b /?&雪")).unwrap();
        assert_eq!(
            uri.components().unwrap().query_parameters()["cursor"],
            "a+b /?&雪"
        );
        if template.contains("kind=report") {
            assert_eq!(
                uri.components().unwrap().query_parameters()["kind"],
                "report"
            );
        }
    }
}

#[test]
fn source_page_current_fields_close_the_root_and_preserve_owner_item_fields() {
    let current = serde_json::json!({"items":[{"uri":"fixture://record/one", "ownerField":"source-owned"}], "limit":100, "nextCursor":"one"});
    let page: SourcePage = serde_json::from_value(current.clone()).unwrap();
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.next_cursor(), Some("one"));
    for mixed in [false, true] {
        let mut bad = current.clone();
        let value = bad["nextCursor"].clone();
        if !mixed {
            bad.as_object_mut().unwrap().remove("nextCursor");
        }
        bad["next_cursor"] = value;
        assert!(serde_json::from_value::<SourcePage>(bad.clone()).is_err());
        assert!(serde_json::from_slice::<SourcePage>(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for limit in [0, 101] {
        let mut bad = current.clone();
        bad["limit"] = limit.into();
        assert!(serde_json::from_value::<SourcePage>(bad).is_err());
    }
    let mut unknown = current;
    unknown["retry"] = true.into();
    assert!(serde_json::from_value::<SourcePage>(unknown).is_err());
}
