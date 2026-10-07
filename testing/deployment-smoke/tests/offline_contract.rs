use std::{collections::BTreeSet, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageManifest {
    schema_version: u32,
    bundle_version: String,
    external_images: Vec<ExternalImage>,
    veoveo_images: Vec<VeoveoImage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalImage {
    r#ref: String,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VeoveoImage {
    r#ref: String,
    dockerfile: String,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("smoke crate lives under <root>/testing")
        .to_owned()
}

#[test]
fn offline_bundle_manifest_is_immutable_and_complete() {
    let root = repository_root();
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(root.join("deploy/offline/images.lock.json")).expect("read image manifest"),
    )
    .expect("parse image manifest");
    assert_eq!(manifest.schema_version, 2);
    assert!(!manifest.bundle_version.trim().is_empty());

    let helm_values =
        fs::read_to_string(root.join("deploy/helm/veoveo/values.yaml")).expect("read Helm values");
    let mut references = BTreeSet::new();
    for image in &manifest.external_images {
        assert!(references.insert(&image.r#ref), "duplicate image reference");
        assert!(!image.r#ref.ends_with(":latest"));
        let digest = image
            .source
            .rsplit_once("@sha256:")
            .map(|(_, digest)| digest)
            .expect("external image source must carry a sha256 digest");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        let (repository, tag) = image
            .r#ref
            .rsplit_once(':')
            .expect("external image ref must have an exact tag");
        assert!(
            helm_values.contains(&format!("repository: {repository}")),
            "Helm values do not use locked external repository {repository}"
        );
        assert!(
            helm_values.contains(&format!("tag: {tag}")),
            "Helm values do not use locked external tag {tag}"
        );
    }
    for image in &manifest.veoveo_images {
        assert!(references.insert(&image.r#ref), "duplicate image reference");
        assert!(!image.r#ref.ends_with(":latest"));
        assert!(
            root.join(&image.dockerfile).is_file(),
            "missing Dockerfile {}",
            image.dockerfile
        );
    }

    let loader = fs::read_to_string(root.join("deploy/offline/load-bundle.sh"))
        .expect("read offline loader");
    assert!(loader.contains("--install-dir"));
    assert!(loader.contains("cp -R \"$stage/payload/.\" \"$install_dir/\""));
    assert!(loader.contains("bundle-evidence"));

    let creator = fs::read_to_string(root.join("deploy/offline/create-bundle.sh"))
        .expect("read offline bundle creator");
    assert!(creator.contains("contract-schemas --output-dir \"$stage/payload/schemas\""));
    assert!(creator.contains("configs/deployments.json"));
    assert!(creator.contains("configs/gateway.local.json"));

    let offline_values = fs::read_to_string(root.join("deploy/offline/values.offline.yaml"))
        .expect("read offline Helm values");
    assert!(offline_values.contains("offline: true"));
    assert!(offline_values.contains("imagePullPolicy: Never"));
}

fn bounded_command(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new("timeout");
    command.args(["--signal=TERM", "--kill-after=2s", "20s", program]);
    command
}

fn admitted(profile: &str, value: &serde_json::Value) -> bool {
    use std::{io::Write, process::Stdio};
    let mut child = bounded_command("jq")
        .args(["-e", "--slurp", "--arg", "profile", profile, "-f"])
        .arg(repository_root().join("deploy/offline/admission.jq"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("jq is an offline qualification prerequisite");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(value).unwrap())
        .unwrap();
    child.wait().unwrap().success()
}

#[test]
fn offline_receivers_refuse_retired_mixed_and_incomplete_selected_identities() {
    let lock: serde_json::Value = serde_json::from_slice(
        &fs::read(repository_root().join("deploy/offline/images.lock.json")).unwrap(),
    )
    .unwrap();
    assert!(admitted("lock", &lock));
    let images: Vec<_> = lock["externalImages"].as_array().unwrap().iter()
        .chain(lock["veoveoImages"].as_array().unwrap())
        .map(|image| serde_json::json!({"ref": image["ref"], "imageId": format!("sha256:{}", "a".repeat(64))}))
        .collect();
    let bundle = serde_json::json!({"schemaVersion":2,"platform":"linux/amd64","bundle":lock,"images":images});
    assert!(admitted("bundle", &bundle));
    for keep_current in [false, true] {
        for (current, retired) in [
            ("schemaVersion", "schema_version"),
            ("externalImages", "external_images"),
            ("veoveoImages", "veoveo_images"),
            ("bundleVersion", "bundle_version"),
        ] {
            let mut bad = lock.clone();
            let field = bad[current].clone();
            if !keep_current {
                bad.as_object_mut().unwrap().remove(current);
            }
            bad[retired] = field;
            assert!(!admitted("lock", &bad), "retired field {retired}");
        }
    }
    for mode in 0..6 {
        let mut bad = bundle.clone();
        match mode {
            0 => bad["schemaVersion"] = 1.into(),
            1 => {
                bad["images"].as_array_mut().unwrap().pop();
            }
            2 => {
                let duplicate = bad["images"][0].clone();
                bad["images"].as_array_mut().unwrap().push(duplicate);
            }
            3 => bad["images"][0]["image_id"] = bad["images"][0]["imageId"].clone(),
            4 => bad["bundle"]["externalImages"][0]["unknown"] = true.into(),
            _ => bad["images"][0]["ref"] = "unselected/image:1".into(),
        }
        assert!(!admitted("bundle", &bad), "bundle mutation {mode}");
    }
}

#[test]
fn offline_loader_refuses_bad_metadata_before_runtime_or_destination_effects() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let stage = temp.path().join("stage");
    fs::create_dir_all(stage.join("metadata")).unwrap();
    fs::create_dir_all(stage.join("payload")).unwrap();
    fs::write(stage.join("images.tar"), b"not an image archive").unwrap();
    fs::write(stage.join("metadata/bundle.json"), b"{\"schemaVersion\":1}").unwrap();
    fs::write(stage.join("metadata/images.lock.json"), b"{}").unwrap();
    let files = [
        "images.tar",
        "metadata/bundle.json",
        "metadata/images.lock.json",
    ];
    let mut sums = Vec::new();
    for file in files {
        let output = bounded_command("sha256sum")
            .arg(file)
            .current_dir(&stage)
            .output()
            .unwrap();
        assert!(output.status.success());
        sums.extend(output.stdout);
    }
    fs::write(stage.join("SHA256SUMS"), sums).unwrap();
    let archive = temp.path().join("bad.tar.gz");
    assert!(
        bounded_command("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(&stage)
            .arg(".")
            .status()
            .unwrap()
            .success()
    );
    let drivers = temp.path().join("drivers");
    fs::create_dir(&drivers).unwrap();
    let marker = temp.path().join("runtime-called");
    for driver in ["docker", "ctr"] {
        let path = drivers.join(driver);
        fs::write(
            &path,
            "#!/bin/sh\nprintf called > \"$OFFLINE_EFFECT_MARKER\"\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for runtime in ["docker", "containerd"] {
        let destination = temp.path().join(format!("destination-{runtime}"));
        let path = std::env::join_paths(
            std::iter::once(drivers.clone())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let output = bounded_command("bash")
            .arg(repository_root().join("deploy/offline/load-bundle.sh"))
            .arg("--bundle")
            .arg(&archive)
            .args(["--runtime", runtime, "--install-dir"])
            .arg(&destination)
            .env("PATH", path)
            .env("OFFLINE_EFFECT_MARKER", &marker)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("invalid or unsupported bundle metadata")
        );
        assert!(!marker.exists());
        assert!(!destination.exists());
    }
    let lock: serde_json::Value = serde_json::from_slice(
        &fs::read(repository_root().join("deploy/offline/images.lock.json")).unwrap(),
    )
    .unwrap();
    let image_id = format!("sha256:{}", "a".repeat(64));
    let images: Vec<_> = lock["externalImages"]
        .as_array()
        .unwrap()
        .iter()
        .chain(lock["veoveoImages"].as_array().unwrap())
        .map(|image| serde_json::json!({"ref":image["ref"], "imageId":image_id}))
        .collect();
    let bundle = serde_json::json!({"schemaVersion":2,"platform":"linux/amd64", "bundle":lock,"images":images});
    fs::write(
        stage.join("metadata/bundle.json"),
        serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap();
    fs::write(
        stage.join("metadata/images.lock.json"),
        serde_json::to_vec(&lock).unwrap(),
    )
    .unwrap();
    fs::write(stage.join("payload/current-profile"), b"admitted").unwrap();
    let mut sums = Vec::new();
    for file in [
        "images.tar",
        "metadata/bundle.json",
        "metadata/images.lock.json",
        "payload/current-profile",
    ] {
        let output = bounded_command("sha256sum")
            .arg(file)
            .current_dir(&stage)
            .output()
            .unwrap();
        assert!(output.status.success());
        sums.extend(output.stdout);
    }
    fs::write(stage.join("SHA256SUMS"), sums).unwrap();
    assert!(
        bounded_command("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&stage)
            .arg(".")
            .status()
            .unwrap()
            .success()
    );
    fs::write(drivers.join("docker"), format!("#!/bin/sh\nprintf called > \"$OFFLINE_EFFECT_MARKER\"\nif [ \"$1\" = image ]; then printf '%s\\n' '{image_id}'; fi\n")).unwrap();
    for runtime in ["docker", "containerd"] {
        let destination = temp.path().join(format!("current-{runtime}"));
        let path = std::env::join_paths(
            std::iter::once(drivers.clone())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let output = bounded_command("bash")
            .arg(repository_root().join("deploy/offline/load-bundle.sh"))
            .arg("--bundle")
            .arg(&archive)
            .args(["--runtime", runtime, "--install-dir"])
            .arg(&destination)
            .env("PATH", path)
            .env("OFFLINE_EFFECT_MARKER", &marker)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(destination.join("current-profile")).unwrap(),
            b"admitted"
        );
        assert!(marker.exists());
    }
    fs::remove_file(&marker).unwrap();
    for target in ["bundle.json", "images.lock.json"] {
        let current = if target == "bundle.json" {
            &bundle
        } else {
            &lock
        };
        let mut retired = (*current).clone();
        retired["schemaVersion"] = serde_json::json!(1);
        let mut unselected = (*current).clone();
        if target == "bundle.json" {
            unselected["images"][0]["ref"] = serde_json::json!("unselected:1");
        } else {
            unselected["externalImages"][0]["ref"] = serde_json::json!("unselected:1");
        }
        for (case, documents) in [
            ("invalid-first", vec![&retired, current]),
            ("invalid-last", vec![current, &retired]),
            ("two-current", vec![current, current]),
            ("unselected-first", vec![&unselected, current]),
        ] {
            // The companion remains the matching current document. Checksums
            // cover the actual stream, so refusal belongs to JSON admission.
            fs::write(
                stage.join("metadata/bundle.json"),
                serde_json::to_vec(&bundle).unwrap(),
            )
            .unwrap();
            fs::write(
                stage.join("metadata/images.lock.json"),
                serde_json::to_vec(&lock).unwrap(),
            )
            .unwrap();
            let stream = documents
                .iter()
                .map(|value| serde_json::to_string(value).unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(stage.join("metadata").join(target), stream).unwrap();
            let mut sums = Vec::new();
            for file in [
                "images.tar",
                "metadata/bundle.json",
                "metadata/images.lock.json",
                "payload/current-profile",
            ] {
                let output = bounded_command("sha256sum")
                    .arg(file)
                    .current_dir(&stage)
                    .output()
                    .unwrap();
                assert!(output.status.success());
                sums.extend(output.stdout);
            }
            fs::write(stage.join("SHA256SUMS"), sums).unwrap();
            assert!(
                bounded_command("tar")
                    .arg("-czf")
                    .arg(&archive)
                    .arg("-C")
                    .arg(&stage)
                    .arg(".")
                    .status()
                    .unwrap()
                    .success()
            );
            for runtime in ["docker", "containerd"] {
                let destination = temp
                    .path()
                    .join(format!("stream-{target}-{case}-{runtime}"));
                let path = std::env::join_paths(
                    std::iter::once(drivers.clone())
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap();
                let output = bounded_command("bash")
                    .arg(repository_root().join("deploy/offline/load-bundle.sh"))
                    .arg("--bundle")
                    .arg(&archive)
                    .args(["--runtime", runtime, "--install-dir"])
                    .arg(&destination)
                    .env("PATH", path)
                    .env("OFFLINE_EFFECT_MARKER", &marker)
                    .output()
                    .unwrap();
                assert!(
                    !output.status.success(),
                    "loader admitted {target} {case} {runtime}"
                );
                assert!(String::from_utf8_lossy(&output.stderr).contains("invalid or unsupported"));
                assert!(!marker.exists());
                assert!(!destination.exists());
            }
        }
    }
}

#[test]
fn offline_builder_admits_current_lock_and_refuses_retired_lock_before_effects() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    fs::create_dir_all(root.join("deploy/offline")).unwrap();
    for file in [
        "create-bundle.sh",
        "load-bundle.sh",
        "admission.jq",
        "README.md",
        "images.lock.json",
        "values.offline.yaml",
    ] {
        fs::copy(
            repository_root().join("deploy/offline").join(file),
            root.join("deploy/offline").join(file),
        )
        .unwrap();
    }
    let lock_path = root.join("deploy/offline/images.lock.json");
    let current: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    for image in current["veoveoImages"].as_array().unwrap() {
        let path = root.join(image["dockerfile"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            b"# isolated admission fixture; no builder executes this file\n",
        )
        .unwrap();
    }
    for directory in ["configs/stream", "assets", "deploy/helm"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    for file in [
        "configs/deployments.json",
        "configs/gateway.local.json",
        "configs/otel-collector.yaml",
        "configs/otel-collector.siem.example.yaml",
        ".env.example",
    ] {
        fs::write(root.join(file), b"{}\n").unwrap();
    }
    let drivers = temp.path().join("drivers");
    fs::create_dir(&drivers).unwrap();
    let marker = temp.path().join("called");
    let digest = format!("sha256:{}", "a".repeat(64));
    for (name, body) in [
        (
            "cargo",
            "#!/bin/sh\nprintf called > \"$OFFLINE_EFFECT_MARKER\"\nexit 0\n".to_owned(),
        ),
        (
            "docker",
            format!(
                "#!/bin/sh\nprintf called > \"$OFFLINE_EFFECT_MARKER\"\ncase \"$1\" in save) : > \"$3\" ;; image) printf '%s\\n' '{digest}' ;; esac\n"
            ),
        ),
    ] {
        let path = drivers.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths(
        std::iter::once(drivers).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output_path = temp.path().join("current.tar.gz");
    let invoke = |destination: &std::path::Path| {
        bounded_command("bash")
            .arg(root.join("deploy/offline/create-bundle.sh"))
            .args(["--skip-build", "--skip-sbom", "--output"])
            .arg(destination)
            .env("PATH", &path)
            .env("OFFLINE_EFFECT_MARKER", &marker)
            .output()
            .unwrap()
    };
    let output = invoke(&output_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(marker.exists());
    assert!(output_path.exists());
    fs::remove_file(&marker).unwrap();
    // The complete stream must contain one document, even when its final
    // document is valid and agrees with the selected images.
    let mut retired = current.clone();
    retired["schemaVersion"] = serde_json::json!(1);
    for (case, documents) in [
        ("invalid-first", vec![&retired, &current]),
        ("invalid-last", vec![&current, &retired]),
        ("two-current", vec![&current, &current]),
    ] {
        let stream = documents
            .iter()
            .map(|value| serde_json::to_string(value).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&lock_path, stream).unwrap();
        let destination = temp.path().join(format!("stream-{case}.tar.gz"));
        let output = invoke(&destination);
        assert!(!output.status.success(), "builder admitted {case}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("invalid or unsupported image manifest")
        );
        assert!(!marker.exists());
        assert!(!destination.exists());
    }
    for keep_current in [false, true] {
        let mut bad = current.clone();
        let version = bad["schemaVersion"].clone();
        if !keep_current {
            bad.as_object_mut().unwrap().remove("schemaVersion");
        }
        bad["schema_version"] = version;
        fs::write(&lock_path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let destination = temp.path().join(format!("refused-{keep_current}.tar.gz"));
        let output = invoke(&destination);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("invalid or unsupported image manifest")
        );
        assert!(!marker.exists());
        assert!(!destination.exists());
    }
}
