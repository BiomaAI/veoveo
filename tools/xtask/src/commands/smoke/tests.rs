use super::*;
use veoveo_testing_support::{
    descriptor::{BuildProfile, PackageName},
    framework::{admit_exact_arguments, libtest_summary},
};
#[test]
#[cfg(target_os = "linux")]
fn actual_compiler_artifacts_bind_external_root_native_listing_execution_and_exact_cases() {
    let source = tempfile::tempdir().unwrap();
    let root = source.path();
    let component = root.join("probe-owner");
    std::fs::create_dir_all(component.join("src")).unwrap();
    std::fs::create_dir(component.join("tests")).unwrap();
    std::fs::write(component.join("Cargo.toml"),"[package]\nname='veoveo-smoke-artifact-probe'\nversion='1.0.0'\nedition='2024'\n[[bin]]\nname='probe'\npath='src/main.rs'\n[[test]]\nname='probe-cases'\npath='tests/cases.rs'\n").unwrap();
    let native = r#"#[link(name="receipt_probe")] unsafe extern "C" { fn receipt_probe()->u32; }"#;
    std::fs::write(component.join("src/main.rs"),format!("{native}\nfn main(){{assert_eq!(unsafe{{receipt_probe()}},7);println!(\"native binary\");}}\n")).unwrap();
    std::fs::write(component.join("tests/cases.rs"),format!("{native}\n#[test] fn first_case(){{println!(\"owner report\");println!(\"test different_case ... ok\");println!(\"test result: ok. 99 passed; 0 failed; 0 ignored\");assert_eq!(unsafe{{receipt_probe()}},7);}}\n#[test] fn second_case(){{assert_eq!(unsafe{{receipt_probe()}},7);}}\n#[test] #[ignore] fn skipped_case(){{}}\n#[test] #[ignore] fn failed_case(){{panic!(\"intentional framework control\");}}\n")).unwrap();
    std::fs::write(component.join("build.rs"),r###"
use std::{path::PathBuf,process::Command};
fn main(){
 let manifest=PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
 std::fs::write(manifest.parent().unwrap().join("build-effect"),"actual build started").unwrap();
 let out=PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
 let source=out.join("receipt_probe.rs");
 std::fs::write(&source,"#[unsafe(no_mangle)] pub extern \"C\" fn receipt_probe()->u32 {7}\n").unwrap();
 assert!(Command::new(std::env::var_os("RUSTC").unwrap()).args(["--crate-type","cdylib","--edition","2024"]).arg(&source).arg("-o").arg(out.join("libreceipt_probe.so")).status().unwrap().success());
 println!("cargo:rustc-link-search=native={}",out.display());
}
"###).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver='3'\nmembers=['probe-owner']\n",
    )
    .unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    assert!(!target.starts_with(root));
    let budget = crate::discovery::budget::Budget::new(&target, 180, 3).unwrap();
    let mut lock = Command::new("cargo");
    lock.args(["generate-lockfile", "--offline"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target);
    budget.output(lock).unwrap();
    let metadata = cargo::inventory(root, &budget).unwrap();
    let selection = |target: &str| CargoSelection {
        owner: "probe-owner".into(),
        package: PackageName::parse("veoveo-smoke-artifact-probe").unwrap(),
        target: target.into(),
        features: BTreeSet::new(),
        default_features: false,
        profile: BuildProfile::Dev,
    };
    let bin = selection("probe");
    let test = selection("probe-cases");
    discovery::admit_cargo(root, &metadata, &bin, "bin").unwrap();
    discovery::admit_cargo(root, &metadata, &test, "test").unwrap();
    let mut scenario: veoveo_testing_support::descriptor::Scenario = serde_json::from_value(serde_json::json!({
        "id":"independent-artifact-probe", "description":"Compiler-produced native harness", "target":{"kind":"cargo_binary","selection":bin},
        "arguments":[], "prerequisites":[], "requirements":{"network":false,"credentials":false,"clusterMutation":false,"billedEffects":false,"nvidia":false,"headedGraphics":false,"explanation":"Native local fixture"},
        "deadlineSeconds":180,"cleanupSeconds":3
    })).unwrap();
    crate::discovery::languages::admit(root, Path::new("probe-owner"), &scenario).unwrap();
    std::fs::create_dir(root.join("composition")).unwrap();
    assert!(crate::discovery::languages::admit(root, Path::new("composition"), &scenario).is_err());
    scenario.execution_owner = Some(serde_json::from_value(serde_json::json!({"owner":"probe-owner","reason":"Runs this independently owned native harness."})).unwrap());
    crate::discovery::languages::admit(root, Path::new("composition"), &scenario).unwrap();
    // A valid independent root cannot exempt the native prerequisite's own manifest/source.
    let prerequisite = root.join("prerequisite");
    std::fs::create_dir(&prerequisite).unwrap();
    std::fs::write(
        prerequisite.join("Cargo.toml"),
        "[package]\nname='independent-prerequisite'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::write(prerequisite.join("probe.rs"), "fn main() {}\n").unwrap();
    let prerequisite_selection = CargoSelection {
        owner: "prerequisite".into(),
        package: PackageName::parse("independent-prerequisite").unwrap(),
        target: "independent-probe".into(),
        features: BTreeSet::new(),
        default_features: false,
        profile: BuildProfile::Dev,
    };
    scenario.prerequisites.push(Preparation::CargoBinary {
        selection: prerequisite_selection.clone(),
    });
    crate::discovery::languages::admit(root, Path::new("composition"), &scenario).unwrap();
    let marker = root.join("build-effect");
    let foreign = root.join("foreign-manifest.toml");
    std::fs::write(
        &foreign,
        "[package]\nname='independent-prerequisite'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::remove_file(prerequisite.join("Cargo.toml")).unwrap();
    std::os::unix::fs::symlink(&foreign, prerequisite.join("Cargo.toml")).unwrap();
    assert!(crate::discovery::languages::admit(root, Path::new("composition"), &scenario).is_err());
    assert!(!marker.exists());
    std::fs::remove_file(prerequisite.join("Cargo.toml")).unwrap();
    std::fs::write(
        prerequisite.join("Cargo.toml"),
        std::fs::read(&foreign).unwrap(),
    )
    .unwrap();
    std::fs::write(prerequisite.join("Cargo.toml"), "[package]\nname='independent-prerequisite'\nversion='1.0.0'\nedition='2024'\n[[bin]]\nname='independent-probe'\npath='probe.rs'\n").unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver='3'\nmembers=['probe-owner','prerequisite']\n",
    )
    .unwrap();
    let mut relock = Command::new("cargo");
    relock
        .args(["generate-lockfile", "--offline"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target);
    budget.output(relock).unwrap();
    let mut prerequisite_metadata = cargo::inventory(root, &budget).unwrap();
    discovery::admit_cargo(root, &prerequisite_metadata, &prerequisite_selection, "bin").unwrap();
    let prerequisite_package = prerequisite_metadata
        .packages
        .iter_mut()
        .find(|package| package.name == "independent-prerequisite")
        .unwrap();
    prerequisite_package.targets[0].src_path = prerequisite.clone();
    assert!(
        discovery::admit_cargo(root, &prerequisite_metadata, &prerequisite_selection, "bin")
            .is_err()
    );
    assert!(!marker.exists());
    let foreign_source = root.join("foreign-probe.rs");
    std::fs::write(&foreign_source, "fn main() {}\n").unwrap();
    std::fs::remove_file(prerequisite.join("probe.rs")).unwrap();
    std::os::unix::fs::symlink(&foreign_source, prerequisite.join("probe.rs")).unwrap();
    prerequisite_metadata
        .packages
        .iter_mut()
        .find(|p| p.name == "independent-prerequisite")
        .unwrap()
        .targets[0]
        .src_path = prerequisite.join("probe.rs");
    assert!(
        discovery::admit_cargo(root, &prerequisite_metadata, &prerequisite_selection, "bin")
            .is_err()
    );
    assert!(!marker.exists());
    std::fs::remove_file(prerequisite.join("probe.rs")).unwrap();
    std::fs::write(prerequisite.join("probe.rs"), "fn main() {}\n").unwrap();
    scenario.prerequisites.clear();
    // This owner path belongs to the fixture source, not a forged executable path.
    let groups = features::plan(
        root,
        &metadata,
        &budget,
        vec![(bin.clone(), false), (test.clone(), true)],
    )
    .unwrap();
    let mut manifest = ArtifactManifest {
        format: ArtifactFormat::V1,
        repository: root.into(),
        target_root: metadata.target_directory.clone(),
        entries: vec![],
    };
    let mut selected = BTreeMap::new();
    for group in &groups {
        prepare_group(
            &budget,
            root,
            &metadata,
            group,
            &mut selected,
            &mut manifest,
        )
        .unwrap();
    }
    manifest.admit(root).unwrap();
    assert!(marker.is_file());
    let entry = manifest.selected(&test, NativeTargetKind::Test).unwrap();
    assert!(!entry.runtime_libraries.is_empty());
    exact_rust_case(&budget, entry, "first_case").unwrap();
    for (case, extra, expected) in [
        ("first_case", None::<&str>, true),
        ("second_case", None, false),
        ("missing_case", None, false),
        ("skipped_case", None, false),
    ] {
        let mut command = Command::new(&entry.executable);
        configure_runtime(&mut command, entry).unwrap();
        command
            .args([case, "--exact", "--show-output", "--format", "pretty"])
            .env("RUST_TEST_THREADS", "1");
        if let Some(extra) = extra {
            command.arg(extra);
        }
        let output = budget.output(command).unwrap();
        if expected {
            assert!(String::from_utf8_lossy(&output.stdout).contains("owner report"));
        }
        assert_eq!(
            libtest_summary(&output.stdout, "first_case").is_ok(),
            expected
        );
    }
    let mut failed = Command::new(&entry.executable);
    configure_runtime(&mut failed, entry).unwrap();
    failed.args(["failed_case", "--exact", "--ignored", "--format", "pretty"]);
    assert!(budget.output(failed).is_err());
    for argv in [
        vec!["second_case".into()],
        vec!["--skip".into(), "first_case".into()],
        vec!["--ignored".into()],
        vec!["--list".into()],
    ] {
        assert!(admit_exact_arguments(&argv).is_err());
    }
    let executable = manifest.selected(&bin, NativeTargetKind::Binary).unwrap();
    let mut command = Command::new(&executable.executable);
    configure_runtime(&mut command, executable).unwrap();
    assert!(
        String::from_utf8(budget.output(command).unwrap().stdout)
            .unwrap()
            .contains("native binary")
    );
    let mut wrong = manifest.clone();
    wrong.entries[0].target_kind = if wrong.entries[0].target_kind == NativeTargetKind::Test {
        NativeTargetKind::Binary
    } else {
        NativeTargetKind::Test
    };
    assert!(wrong.admit(root).is_err());
    let library = &entry.runtime_libraries[0].path;
    let original = std::fs::read(library).unwrap();
    std::fs::write(library, b"replaced library").unwrap();
    let refusal = manifest.admit(root);
    std::fs::write(library, original).unwrap();
    assert!(refusal.is_err());
    manifest.admit(root).unwrap();
    let path = &entry.executable;
    let original = std::fs::read(path).unwrap();
    std::fs::write(path, b"replaced executable").unwrap();
    let refusal = manifest.admit(root);
    std::fs::write(path, original).unwrap();
    assert!(refusal.is_err());
    manifest.admit(root).unwrap();
    budget.completed();
}
