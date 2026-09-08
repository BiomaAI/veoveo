use super::*;

fn lease() -> Lease {
    Lease {
        api_version: API_VERSION.into(),
        kind: "Lease".into(),
        metadata: Metadata {
            namespace: "fixture".into(),
            name: NAME.into(),
            uid: Some("one".into()),
            resource_version: Some("17".into()),
            labels: BTreeMap::new(),
        },
        spec: Spec {
            holder_identity: "installer-one".into(),
            lease_duration_seconds: None,
        },
    }
}

#[test]
fn changed_or_expiring_lock_cannot_authorize_execution() {
    let expected = lease();
    validate_current(&expected, &lease()).unwrap();
    let mut changed = lease();
    changed.metadata.uid = Some("replacement".into());
    assert!(validate_current(&expected, &changed).is_err());
    changed = lease();
    changed.metadata.resource_version = Some("18".into());
    assert!(validate_current(&expected, &changed).is_err());
    changed = lease();
    changed.spec.holder_identity = "installer-two".into();
    assert!(validate_current(&expected, &changed).is_err());
    changed = lease();
    changed.spec.lease_duration_seconds = Some(30);
    assert!(validate_current(&expected, &changed).is_err());
}

#[test]
fn delete_requires_both_server_identities() {
    let encoded = serde_json::to_value(DeleteOptions {
        api_version: "v1",
        kind: "DeleteOptions",
        preconditions: Preconditions {
            uid: "one",
            resource_version: "17",
        },
    })
    .unwrap();
    assert_eq!(
        encoded["preconditions"],
        serde_json::json!({"uid":"one", "resourceVersion":"17"})
    );
}

#[test]
#[ignore = "requires VEOVEO_OWNERSHIP_TEST_CONTEXT; uses only isolated namespace and Lease objects"]
fn native_independent_installers_serialize_and_stale_unlock_is_rejected() {
    let context = std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live context");
    let nonce = tempfile::Builder::new()
        .prefix("veoveo-coordination-")
        .tempdir()
        .unwrap();
    let namespace = nonce
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_lowercase();
    crate::process::output_checked(
        "kubectl",
        ["--context", &context, "create", "namespace", &namespace],
        None,
    )
    .unwrap();
    struct Cleanup {
        context: String,
        namespace: String,
        removed: bool,
    }
    impl Cleanup {
        fn finish(&mut self) -> Result<()> {
            crate::process::output_checked(
                "kubectl",
                [
                    "--context",
                    &self.context,
                    "delete",
                    "namespace",
                    &self.namespace,
                    "--wait=true",
                    "--timeout=60s",
                ],
                None,
            )?;
            let absent = crate::process::output_checked(
                "kubectl",
                [
                    "--context",
                    &self.context,
                    "get",
                    "namespace",
                    &self.namespace,
                    "--ignore-not-found",
                    "--output=json",
                ],
                None,
            )?;
            ensure!(
                absent.is_empty(),
                "coordination fixture namespace survived cleanup"
            );
            self.removed = true;
            Ok(())
        }
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if !self.removed {
                let _ = self.finish();
            }
        }
    }
    let mut cleanup = Cleanup {
        context: context.clone(),
        namespace: namespace.clone(),
        removed: false,
    };
    let mut first = ExecutionLock::at(&context, &namespace, NAME).unwrap();
    assert!(
        ExecutionLock::at(&context, &namespace, NAME)
            .err()
            .expect("occupied lock must reject a second installer")
            .to_string()
            .contains("another operation")
    );
    first.check().unwrap();
    let evidence = first.release().unwrap();
    assert!(evidence.released);
    let mut replacement = ExecutionLock::at(&context, &namespace, NAME).unwrap();
    assert_ne!(first.lease.metadata.uid, replacement.lease.metadata.uid);
    // An interrupted holder cannot release the replacement even with the same name.
    first.phase = Phase::Executing;
    assert!(first.release().is_err());
    replacement.check().unwrap();
    let (old_uid, old_version) = identity(&first.lease).unwrap();
    let path = format!("/apis/coordination.k8s.io/v1/namespaces/{namespace}/leases/{NAME}");
    assert!(
        request(
            &context,
            &["delete", "--raw", &path, "--filename=-"],
            &DeleteOptions {
                api_version: "v1",
                kind: "DeleteOptions",
                preconditions: Preconditions {
                    uid: old_uid,
                    resource_version: old_version
                },
            }
        )
        .is_err()
    );
    replacement.check().unwrap();
    first.phase = Phase::Released; // the original object was already removed successfully.
    replacement.release().unwrap();
    let absent = crate::process::output_checked(
        "kubectl",
        [
            "--context",
            &context,
            "--namespace",
            &namespace,
            "get",
            "lease",
            NAME,
            "--ignore-not-found",
            "--output=json",
        ],
        None,
    )
    .unwrap();
    assert!(absent.is_empty());
    let mut interrupted = ExecutionLock::at(&context, &namespace, NAME).unwrap();
    interrupted.begin_execution().unwrap();
    let (uid, version) = identity(&interrupted.lease).unwrap();
    let uid = uid.to_owned();
    let version = version.to_owned();
    drop(interrupted);
    assert!(
        ExecutionLock::at(&context, &namespace, NAME).is_err(),
        "incomplete execution must not release its lock"
    );
    // This fixture launches no mutation children; recover only its exact UID.
    request(
        &context,
        &["delete", "--raw", &path, "--filename=-"],
        &DeleteOptions {
            api_version: "v1",
            kind: "DeleteOptions",
            preconditions: Preconditions {
                uid: &uid,
                resource_version: &version,
            },
        },
    )
    .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            barrier.wait();
            ExecutionLock::at(&context, &namespace, NAME)
        });
        let two = scope.spawn(|| {
            barrier.wait();
            ExecutionLock::at(&context, &namespace, NAME)
        });
        [one.join().unwrap(), two.join().unwrap()]
    });
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    for mut winner in outcomes.into_iter().flatten() {
        winner.release().unwrap();
    }
    cleanup.finish().unwrap();
}
