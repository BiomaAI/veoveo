use super::*;

fn entry() -> Entry {
    Entry {
        path: "owner.rs",
        name: "allowed",
        function: "allowed",
        kind: Kind::Declarative,
    }
}
fn found(source: &str) -> Vec<(String, Definition)> {
    definitions(source)
        .unwrap()
        .into_iter()
        .map(|definition| ("owner.rs".into(), definition))
        .collect()
}

#[test]
fn nested_definitions_are_checked_while_comments_literals_and_invocations_are_ignored() {
    let source = r###"
        // macro_rules! ignored {}
        const FIXTURE: &str = r#"macro_rules! ignored {} #[proc_macro] fn ignored() {}"#;
        third_party::invoke!(macro_rules! tokens_are_not_source {});
        mod nested { fn local() { macro_rules! forbidden { () => {}; } } }
    "###;
    let definitions = definitions(source).unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].name, "forbidden");
    assert!(qualify(&found(source), &[entry()]).is_err());
}
#[test]
fn exact_path_name_kind_function_and_single_definition_are_required() {
    let good = found("macro_rules! allowed { () => {}; }");
    assert!(qualify(&good, &[entry()]).is_ok());
    assert!(qualify(&[], &[entry()]).is_err());
    assert!(qualify(&[good[0].clone(), good[0].clone()], &[entry()]).is_err());
    let mut bad = good.clone();
    bad[0].0 = "moved.rs".into();
    assert!(qualify(&bad, &[entry()]).is_err());
    let mut bad = good.clone();
    bad[0].1.name = "renamed".into();
    assert!(qualify(&bad, &[entry()]).is_err());
    let mut bad = good.clone();
    bad[0].1.kind = Kind::Function;
    assert!(qualify(&bad, &[entry()]).is_err());
    let mut bad = good;
    bad[0].1.function = "other".into();
    assert!(qualify(&bad, &[entry()]).is_err());
}
#[test]
fn all_proc_entrypoint_kinds_and_conditional_attributes_are_detected() {
    let definitions = definitions(
        r#"
        #[proc_macro] pub fn call() {}
        #[proc_macro_derive(Model, attributes(owner))] pub fn derive_model() {}
        #[proc_macro_attribute] pub fn attribute() {}
        #[cfg_attr(feature="active", proc_macro_derive(Conditional))] pub fn conditional() {}
        #[cfg_attr(any(), proc_macro)] pub fn conditional_call() {}
        #[cfg_attr(any(), cfg_attr(any(), proc_macro_attribute))] pub fn conditional_attribute() {}
        #[cfg(any())] fn hidden() {macro_rules! hidden_definition { () => {}; }}
    "#,
    )
    .unwrap();
    assert_eq!(definitions.len(), 7);
    assert_eq!(definitions[1].name, "Model");
    assert_eq!(definitions[1].function, "derive_model");
    assert_eq!(definitions[2].kind, Kind::Attribute);
    assert_eq!(definitions[3].name, "Conditional");
    assert_eq!(definitions[4].kind, Kind::Function);
    assert_eq!(definitions[5].kind, Kind::Attribute);
}
#[test]
fn malformed_source_or_procedural_declaration_is_a_failure() {
    assert!(definitions("fn broken( {").is_err());
    assert!(definitions("#[proc_macro_derive] fn invalid() {}").is_err());
}
#[test]
fn current_repository_catalog_is_complete() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (count, found) = scan(&root).unwrap();
    assert!(count > 0);
    qualify(&found, CATALOG).unwrap();
}
