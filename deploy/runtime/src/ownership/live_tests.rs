//! Isolated ConfigMap releases exercise the real Helm/Kubernetes read boundary.
use super::*;
use crate::{
    compile::CompiledUnit,
    helm_bundle::{ChartMetadata, CompiledHelmRelease},
    process::status_checked,
};
use veoveo_deploy_contract::components::*;
use veoveo_extension_contract::{ArtifactDigest, SourceRevision};

pub(crate) struct Namespace {
    pub(crate) context: String,
    pub(crate) name: String,
}

impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = status_checked(
            "kubectl",
            [
                "--context",
                &self.context,
                "delete",
                "namespace",
                &self.name,
                "--ignore-not-found",
                "--wait=true",
                "--timeout=60s",
            ],
            &[],
            None,
        );
    }
}

pub(crate) fn component(namespace: &str, name: &str, objects: &[&str]) -> CompiledComponent {
    let mut objects = objects.iter().map(|name| {
        let mut object = serde_json::json!({"apiVersion":"v1", "kind":"ConfigMap", "metadata":{"namespace":namespace,"name":name},"data":{"setting":"public-test"}});
        if name.starts_with("hook-") { object["metadata"]["annotations"] = serde_json::json!({"helm.sh/hook":"pre-install,pre-upgrade,pre-rollback"}); }
        object
    }).collect::<Vec<_>>();
    crate::helm_bundle::own_hooks(&mut objects, namespace, name).unwrap();
    let inventory = ObjectScopes::default()
        .rendered(&objects, namespace, &BTreeSet::new())
        .unwrap();
    let source = ComponentSource {
        name: name.into(),
        repository: format!("https://example.invalid/{name}"),
        revision: SourceRevision::new("a".repeat(40)).unwrap(),
    };
    let configuration = InstallationSnapshot {
        source: ComponentSource {
            name: "installation".into(),
            repository: "https://example.invalid/installation".into(),
            revision: SourceRevision::new("b".repeat(40)).unwrap(),
        },
        profile: "deployment.json".into(),
    };
    let target = AtomicTarget::HelmRelease {
        namespace: namespace.into(),
        name: name.into(),
    };
    let inputs = BTreeSet::from([ComponentInput::Chart {
        source: source.clone(),
        coordinate: format!("source://{name}/chart"),
        digest: ArtifactDigest::new(format!("sha256:{}", "c".repeat(64))).unwrap(),
    }]);
    let prepared = PreparedAtomicUnit {
        component: name.to_owned().try_into().unwrap(),
        source: source.clone(),
        configuration: configuration.clone(),
        target: target.clone(),
        inputs: inputs.clone(),
        objects: inventory.clone(),
        tool_scope: AtomicToolScope::Exact,
    };
    let declaration = DeploymentComponent {
        id: prepared.component.clone(),
        role: ComponentRole::Workload,
        source,
        configuration,
        namespaces: BTreeSet::from([namespace.into()]),
        targets: BTreeSet::from([target]),
        permitted_objects: inventory
            .into_iter()
            .map(|object| object.identity)
            .collect(),
        inputs,
        dependencies: BTreeSet::new(),
        extension_release: None,
    };
    let locked = lock_component(declaration, vec![prepared.clone()]).unwrap();
    let helm = CompiledHelmRelease::prepare(
        &ChartMetadata {
            api_version: "v2".into(),
            name: name.into(),
            version: "1.0.0".into(),
            app_version: None,
        },
        &objects,
        60,
    )
    .unwrap();
    CompiledComponent {
        locked,
        units: vec![CompiledUnit {
            helm: Some(helm),
            installation_input: None,
            prepared,
            objects,
        }],
    }
}

pub(crate) fn snapshot(context: &str, namespace: &str) -> serde_json::Value {
    let bytes = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "configmaps",
            "-o",
            "json",
        ],
        None,
    )
    .unwrap();
    let objects: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    serde_json::Value::Array(objects["items"].as_array().unwrap().iter().map(|object| serde_json::json!({"name":object["metadata"]["name"],"uid":object["metadata"]["uid"],"resourceVersion":object["metadata"]["resourceVersion"]})).collect())
}

#[test]
#[ignore = "requires VEOVEO_OWNERSHIP_TEST_CONTEXT; creates and deletes isolated ConfigMap Helm releases"]
fn live_preflight_rejects_historical_transfer_and_raw_adoption_without_mutation() {
    let context =
        std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live test context");
    let name = format!(
        "veoveo-ownership-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    status_checked(
        "kubectl",
        ["--context", &context, "create", "namespace", &name],
        &[],
        None,
    )
    .unwrap();
    let namespace = Namespace { context, name };
    let old = [
        component(
            &namespace.name,
            "platform",
            &["platform", "retired", "hook-retired"],
        ),
        component(&namespace.name, "extension", &["extension"]),
    ];
    for component in &old {
        component.units[0]
            .helm
            .as_ref()
            .unwrap()
            .install(
                &namespace.context,
                &namespace.name,
                component.locked.declaration.id.as_str(),
            )
            .unwrap();
    }
    let catalog = old
        .iter()
        .map(|component| component.locked.clone())
        .collect::<Vec<_>>();
    validate_live_ownership(&namespace.context, &catalog, &old).unwrap();
    let before = snapshot(&namespace.context, &namespace.name);
    let revisions = ["platform", "extension"].map(|name| {
        release_metadata(&namespace.context, &namespace.name, name)
            .unwrap()
            .unwrap()
    });
    for retired in ["retired", "hook-retired"] {
        let next = [
            component(&namespace.name, "platform", &["platform"]),
            component(&namespace.name, "extension", &["extension", retired]),
        ];
        let catalog = next
            .iter()
            .map(|component| component.locked.clone())
            .collect::<Vec<_>>();
        let error = validate_live_ownership(&namespace.context, &catalog, &next).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("outside component platform ownership"),
            "{error:#}"
        );
    }
    let mut raw = component(&namespace.name, "platform", &["platform"]);
    let target = AtomicTarget::ManifestSet {
        name: "public-resources".into(),
    };
    raw.units[0].prepared.target = target.clone();
    raw.units[0].helm = None;
    raw.locked.declaration.targets = BTreeSet::from([target]);
    raw.locked =
        lock_component(raw.locked.declaration, vec![raw.units[0].prepared.clone()]).unwrap();
    let error =
        validate_live_ownership(&namespace.context, &[raw.locked.clone()], &[raw]).unwrap_err();
    assert!(format!("{error:#}").contains("overlaps an existing Helm owner"));
    assert_eq!(snapshot(&namespace.context, &namespace.name), before);
    for release in revisions {
        assert_eq!(
            release_metadata(&namespace.context, &namespace.name, &release.name)
                .unwrap()
                .as_ref(),
            Some(&release)
        );
    }
    println!(
        "Live ownership preflight: historical transfer and raw adoption rejected; ConfigMap identities, versions and Helm revisions unchanged."
    );
    let context = namespace.context.clone();
    let name = namespace.name.clone();
    drop(namespace);
    let remaining = output_checked(
        "kubectl",
        [
            "--context",
            &context,
            "get",
            "namespace",
            &name,
            "--ignore-not-found",
            "--output=name",
        ],
        None,
    )
    .unwrap();
    assert!(
        remaining.iter().all(u8::is_ascii_whitespace),
        "live test namespace was not removed"
    );
}
