#![cfg(feature = "runner")]
//! Private function versions and deferred effects use the upstream parser, not SQL text matching.
use veoveo_modules::runner::prepare;
use veoveo_modules::*;
fn module(
    name: &str,
    layer: ModuleLayer,
    bodies: Vec<&'static str>,
    requirements: Vec<LaneRequirement>,
) -> ModuleSetup {
    ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new(name).unwrap()),
            OwnershipClaim::TablePrefix(TablePrefix::new(format!("{name}_")).unwrap()),
            OwnershipClaim::FunctionPrefix(FunctionPrefix::new(format!("fn::{name}::")).unwrap()),
        ])
        .execution(
            LaneExecution::new(
                ExecutionImage::new("fixture").unwrap(),
                ExecutionCommand::new(vec!["fixture".into()]).unwrap(),
            )
            .unwrap(),
        )
        .requires(requirements.clone())
        .lane(
            MigrationLane::new(
                bodies
                    .into_iter()
                    .enumerate()
                    .map(|(version, sql)| {
                        assert!(
                            surrealdb_syn::parse_with_settings(
                                sql.as_bytes(),
                                surrealdb_syn::ParserSettings::default(),
                                async |parser, stack| {
                                    let ast = parser.parse_query(stack).await?;
                                    parser.assert_finished()?;
                                    Ok(ast)
                                }
                            )
                            .is_ok(),
                            "fixture must be native grammar: {sql}"
                        );
                        Migration::new(
                            MigrationVersion::new(version as u32),
                            MigrationName::new(format!("body_{version}")).unwrap(),
                            sql,
                        )
                        .unwrap()
                        .with_requirements(requirements.clone())
                        .unwrap()
                    })
                    .collect(),
            )
            .unwrap(),
        )
        .build()
        .unwrap()
}
fn admitted(sql: &'static str) -> bool {
    let registry =
        ModuleRegistry::new(vec![module("own", ModuleLayer::Kernel, vec![sql], vec![])]).unwrap();
    let result = prepare(registry.select(vec![]).unwrap());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result.is_ok()
}
#[test]
fn local_calls_inspect_complete_callee_bodies_and_native_parameters() {
    for sql in [
        "DEFINE FUNCTION fn::own::leaf($value: string) -> string { RETURN string::uppercase($value); }; DEFINE FUNCTION fn::own::caller($value: string) -> string { RETURN fn::own::leaf($value); }; RETURN fn::own::caller('value');",
        "DEFINE FUNCTION fn::own::caller($value: string) -> string { RETURN fn::own::leaf($value); }; DEFINE FUNCTION fn::own::leaf($value: string) -> string { RETURN string::uppercase($value); }; RETURN fn::own::caller('value');",
        "DEFINE FUNCTION fn::own::write($id: record<own>) -> bool { UPDATE ONLY $id SET value=1; RETURN true; }; fn::own::write(own:one);",
        "DEFINE FUNCTION fn::own::read($input: object) -> bool { RETURN $input.enabled = true; }; RETURN fn::own::read({enabled:true});",
        "DEFINE FUNCTION fn::own::read($rows: array<object>) { FOR $row IN $rows { LET $payload: object=$row.payload; RETURN $payload.enabled; }; };",
        "DEFINE FUNCTION fn::own::read($input: any) -> bool { RETURN IF type::is_object($input) THEN $input.enabled=true ELSE false END; };",
        "DEFINE TABLE own; DEFINE FIELD payload ON own TYPE object; DEFINE INDEX nested ON own FIELDS payload.key; DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; };",
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        "DEFINE FUNCTION fn::own::write() { DELETE foreign; }; RETURN fn::own::write();",
        "DEFINE FUNCTION fn::own::write() { UPDATE own SET nested=(DELETE foreign); };",
        "DEFINE FUNCTION fn::own::leaf($value: string) -> bool { RETURN true; }; RETURN fn::own::leaf(<string>(DELETE own));",
        "RETURN fn::own::future(); DEFINE FUNCTION fn::own::future() -> bool { RETURN true; };",
        "DEFINE FUNCTION fn::own::caller() { RETURN fn::own::missing(); };",
        "DEFINE FUNCTION fn::own::cycle() { RETURN fn::own::cycle(); };",
        "DEFINE FUNCTION fn::own::first() { RETURN fn::own::second(); }; DEFINE FUNCTION fn::own::second() { RETURN fn::own::first(); };",
        "DEFINE FUNCTION fn::own::read($input: any) { RETURN $input.link; };",
        "DEFINE FUNCTION fn::own::read($input: object) { RETURN $input.link.secret; };",
        "DEFINE FUNCTION fn::own::read($input: record<own>) { RETURN $input.secret; };",
        "DEFINE FUNCTION fn::own::read($rows: array<record<own>>) { FOR $row IN $rows { RETURN $row.secret; }; };",
        "DEFINE FUNCTION fn::own::read($rows: array<any>) { FOR $row IN $rows { RETURN $row.secret; }; };",
        "DEFINE FUNCTION fn::own::read($input: object) { LET $payload: any=$input.payload; RETURN $payload.secret; };",
        "RETURN string::len(<string>(DELETE own));",
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}
#[test]
fn optional_calls_require_readonly_callees_and_introducing_minimum() {
    for (base_layer, consumer_layer, body, dependency, minimum, expected) in [
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            "DEFINE FUNCTION fn::base::read() -> bool { RETURN (SELECT * FROM base) != []; };",
            true,
            true,
            true,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            "DEFINE FUNCTION fn::base::read() -> bool { DELETE base; RETURN true; };",
            true,
            true,
            false,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            "DEFINE FUNCTION fn::base::read() -> bool { RETURN true; };",
            false,
            false,
            false,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            "DEFINE FUNCTION fn::base::read() -> bool { RETURN true; };",
            true,
            false,
            false,
        ),
        (
            ModuleLayer::Kernel,
            ModuleLayer::Optional,
            "DEFINE FUNCTION fn::base::read() -> bool { RETURN true; };",
            true,
            true,
            false,
        ),
    ] {
        let requirement = LaneRequirement::AtLeast {
            module: ModuleName::new("base").unwrap(),
            version: MigrationVersion::new(0),
        };
        let base = module("base", base_layer, vec![body], vec![]);
        let consumer = module(
            "consumer",
            consumer_layer,
            vec!["RETURN fn::base::read();"],
            if minimum {
                vec![requirement.clone()]
            } else {
                vec![]
            },
        );
        let consumer = if dependency && !minimum {
            ModuleSetup::builder(consumer.name().clone(), consumer.layer())
                .ownership(consumer.ownership().to_vec())
                .execution(consumer.execution().clone())
                .lane(consumer.lane().clone())
                .requires(vec![LaneRequirement::Satisfied(
                    ModuleName::new("base").unwrap(),
                )])
                .build()
                .unwrap()
        } else {
            consumer
        };
        let registry = ModuleRegistry::new(vec![base, consumer]).unwrap();
        assert_eq!(
            prepare(
                registry
                    .select(vec![ModuleName::new("consumer").unwrap()])
                    .unwrap()
            )
            .is_ok(),
            expected
        );
    }
}
#[test]
fn surviving_readonly_children_reject_writer_overwrites_and_removals() {
    for middle in [
        "DEFINE TABLE own PERMISSIONS FOR select WHERE fn::own::leaf();",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE bool ASSERT fn::own::leaf();",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE bool VALUE fn::own::leaf();",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE bool DEFAULT fn::own::leaf();",
        "DEFINE TABLE own; DEFINE EVENT event ON own WHEN fn::own::leaf() THEN true;",
        "DEFINE FUNCTION fn::own::caller() -> bool { RETURN fn::own::leaf(); } PERMISSIONS WHERE fn::own::leaf();",
        "DEFINE FUNCTION fn::own::caller() -> bool { RETURN fn::own::leaf(); };",
        "DEFINE TABLE own; DEFINE EVENT event ON own WHEN true THEN fn::own::leaf();",
    ] {
        let sql = Box::leak(format!("DEFINE FUNCTION fn::own::leaf() -> bool {{ RETURN true; }}; {middle} DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool {{ UPDATE own SET value=1; RETURN true; }};").into_boxed_str());
        assert!(!admitted(sql), "surviving readonly caller: {sql}");
    }
    assert!(!admitted(
        "DEFINE FUNCTION fn::own::leaf() { RETURN true; }; DEFINE FUNCTION fn::own::caller() { RETURN fn::own::leaf(); }; REMOVE FUNCTION fn::own::leaf;"
    ));
    assert!(admitted(
        "DEFINE FUNCTION fn::own::leaf() { RETURN true; }; DEFINE FUNCTION fn::own::caller() { RETURN fn::own::leaf(); }; REMOVE FUNCTION fn::own::caller; REMOVE FUNCTION fn::own::leaf;"
    ));
    assert!(admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own PERMISSIONS FOR select WHERE fn::own::leaf(); DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool { RETURN false; };"
    ));
    assert!(!admitted(
        "DEFINE FUNCTION fn::own::caller() { RETURN fn::own::leaf(); }; RETURN fn::own::caller(); DEFINE FUNCTION fn::own::leaf() { RETURN true; };"
    ));
}
#[test]
fn scalar_children_and_owned_views_keep_reads_and_mutations_separate() {
    for sql in [
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE array<string> VALUE $value.distinct() ASSERT $value.all(|$item: any| string::len($item)>0);",
        "DEFINE TABLE own; DEFINE FIELD digest ON own TYPE string ASSERT $value.len() = 64;",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE uuid ASSERT $value != $this.previous;",
        "DEFINE TABLE own_daily TYPE ANY SCHEMALESS AS SELECT state, count() AS count FROM own GROUP BY state;",
        "DEFINE FUNCTION fn::own::read($id: record<own>) -> bool { RETURN record::exists($id); };",
        "DEFINE FUNCTION fn::own::write() { LET $id: record<own> = type::record('own', ['part', 1]); CREATE ONLY $id; };",
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE array<string> ASSERT $value.all(|$item: any| (DELETE own));",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE record VALUE $value.secret;",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE string ASSERT $this.link.secret = $value;",
        "DEFINE TABLE own_daily TYPE ANY SCHEMALESS AS SELECT count((DELETE own)) AS count FROM own GROUP ALL;",
        "DEFINE TABLE own_daily TYPE ANY SCHEMALESS AS SELECT count() AS count FROM foreign GROUP ALL;",
        "DEFINE TABLE own; DEFINE FIELD value ON own TYPE record ASSERT record::exists($value);",
        "DEFINE FUNCTION fn::own::read($value: object) { RETURN record::exists($value.target); };",
        "DEFINE FUNCTION fn::own::read($id: record<own>) { RETURN record::exists(foreign:one); };",
        "DEFINE FUNCTION fn::own::read($name: string) { LET $id: record<own> = type::record($name, 'id'); RETURN record::exists($id); };",
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}

#[test]
fn altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers() {
    assert!(!admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own; ALTER TABLE own PERMISSIONS FOR select WHERE fn::own::leaf(); DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool { UPDATE own SET state='changed'; RETURN true; };"
    ));
    assert!(!admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own; ALTER TABLE own PERMISSIONS FOR select WHERE fn::own::leaf(); REMOVE FUNCTION fn::own::leaf;"
    ));
    assert!(admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own PERMISSIONS FOR select WHERE fn::own::leaf(); ALTER TABLE own PERMISSIONS NONE; DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool { UPDATE own SET state='changed'; RETURN true; };"
    ));
    for middle in [
        "DEFINE TABLE own PERMISSIONS FOR select WHERE fn::own::leaf(); DEFINE TABLE IF NOT EXISTS own PERMISSIONS FULL;",
        "DEFINE TABLE own; DEFINE FIELD state ON own TYPE bool ASSERT fn::own::leaf(); DEFINE FIELD IF NOT EXISTS state ON own TYPE bool;",
        "DEFINE TABLE own; DEFINE EVENT event ON own WHEN fn::own::leaf() THEN true; DEFINE EVENT IF NOT EXISTS event ON own WHEN true THEN true;",
    ] {
        let sql = Box::leak(format!("DEFINE FUNCTION fn::own::leaf() -> bool {{ RETURN true; }}; {middle} DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool {{ UPDATE own SET state=true; RETURN true; }};").into_boxed_str());
        assert!(
            !admitted(sql),
            "conditional definition cannot erase stored callback: {sql}"
        );
    }
    assert!(!admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own; DEFINE INDEX counter ON own COUNT WHERE fn::own::leaf(); DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool { UPDATE own SET state=true; RETURN true; };"
    ));
    assert!(admitted(
        "DEFINE FUNCTION fn::own::leaf() -> bool { RETURN true; }; DEFINE TABLE own; DEFINE INDEX counter ON own COUNT WHERE fn::own::leaf(); REMOVE INDEX counter ON own; DEFINE FUNCTION OVERWRITE fn::own::leaf() -> bool { UPDATE own SET state=true; RETURN true; };"
    ));
}

#[test]
fn optional_minimum_tracks_introduction_across_safe_overwrites() {
    let requirement = LaneRequirement::AtLeast {
        module: ModuleName::new("base").unwrap(),
        version: MigrationVersion::new(0),
    };
    let base = module(
        "base",
        ModuleLayer::Optional,
        vec![
            "DEFINE FUNCTION fn::base::read() -> bool { RETURN true; };",
            "DEFINE FUNCTION OVERWRITE fn::base::read() -> bool { RETURN false; };",
        ],
        vec![],
    );
    let consumer = module(
        "consumer",
        ModuleLayer::Optional,
        vec![
            "DEFINE FUNCTION fn::consumer::read() -> bool { RETURN fn::base::read(); }; RETURN fn::consumer::read();",
        ],
        vec![requirement],
    );
    let registry = ModuleRegistry::new(vec![base, consumer]).unwrap();
    prepare(
        registry
            .select(vec![ModuleName::new("consumer").unwrap()])
            .unwrap(),
    )
    .unwrap();
}
