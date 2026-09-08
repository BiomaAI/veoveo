//! Observations hash actual API contents, including defaults and status.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use serde_json::Value;
use veoveo_deploy_contract::components::{
    InstalledObjectObservation, InstalledObjectState, ObjectIdentity,
};
use veoveo_extension_contract::ArtifactDigest;

use crate::{compile::objects::object_digest, ownership::validate_manager_value};

pub(super) fn fingerprint(value: &Value) -> Result<ArtifactDigest> {
    let mut value = value.clone();
    let metadata = value
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .context("live object has no metadata")?;
    for field in ["resourceVersion", "managedFields"] {
        metadata.remove(field);
    }
    object_digest(&value)
}

fn uid(value: &Value) -> Result<&str> {
    value
        .pointer("/metadata/uid")
        .and_then(Value::as_str)
        .filter(|uid| !uid.is_empty())
        .context("live object has no UID")
}

// A matching literal projection needs no normalization request. If Kubernetes
// rewrote quantities or omitted empty fields, the API must establish equivalence.
pub(super) fn contains_desired(live: &Value, desired: &Value) -> bool {
    match (live, desired) {
        (Value::Object(live), Value::Object(desired)) => desired.iter().all(|(key, value)| {
            live.get(key)
                .is_some_and(|actual| contains_desired(actual, value))
        }),
        (Value::Array(live), Value::Array(desired)) => {
            live.len() == desired.len()
                && live
                    .iter()
                    .zip(desired)
                    .all(|(actual, desired)| contains_desired(actual, desired))
        }
        _ => live == desired,
    }
}

fn completed_ttl_job(value: &Value) -> bool {
    value["apiVersion"] == "batch/v1"
        && value["kind"] == "Job"
        && value
            .pointer("/spec/ttlSecondsAfterFinished")
            .and_then(Value::as_u64)
            .is_some()
        && value
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .is_some_and(|conditions| {
                conditions.iter().any(|condition| {
                    condition["type"] == "Complete" && condition["status"] == "True"
                }) && !conditions
                    .iter()
                    .any(|condition| condition["type"] == "Failed" && condition["status"] == "True")
            })
}

pub(super) fn capture(
    unit: &crate::compile::CompiledUnit,
    live: &BTreeMap<ObjectIdentity, Value>,
    completed_hooks: &BTreeSet<ObjectIdentity>,
    normalize: impl Fn(&Value) -> Result<Value>,
) -> Result<Option<Vec<InstalledObjectObservation>>> {
    let mut observations = Vec::new();
    for (rendered, desired) in unit.prepared.objects.iter().zip(&unit.objects) {
        let state = if let Some(actual) = live.get(&rendered.identity) {
            validate_manager_value(&unit.prepared.target, actual)?;
            if !actual["metadata"]["deletionTimestamp"].is_null() {
                return Ok(None);
            }
            let digest = fingerprint(actual)?;
            if !contains_desired(actual, desired) && fingerprint(&normalize(desired)?)? != digest {
                return Ok(None);
            }
            InstalledObjectState::Present {
                uid: uid(actual)?.into(),
                digest,
                completed_ttl_job: completed_ttl_job(actual),
            }
        } else if completed_hooks.contains(&rendered.identity) {
            InstalledObjectState::CompletedHook
        } else {
            return Ok(None);
        };
        observations.push(InstalledObjectObservation {
            identity: rendered.identity.clone(),
            state,
        });
    }
    Ok(Some(observations))
}

pub(super) fn unchanged(
    observations: &[InstalledObjectObservation],
    live: &BTreeMap<ObjectIdentity, Value>,
    completed_hooks: &BTreeSet<ObjectIdentity>,
) -> Result<bool> {
    for observation in observations {
        match (&observation.state, live.get(&observation.identity)) {
            (
                InstalledObjectState::Present {
                    uid: expected_uid,
                    digest,
                    ..
                },
                Some(actual),
            ) if uid(actual)? == expected_uid && &fingerprint(actual)? == digest => {}
            (
                InstalledObjectState::Present {
                    completed_ttl_job: true,
                    ..
                },
                None,
            ) => {}
            (InstalledObjectState::CompletedHook, None)
                if completed_hooks.contains(&observation.identity) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fingerprints_preserve_defaults_status_uid_and_extra_fields() {
        let baseline = json!({"metadata":{"uid":"original","resourceVersion":"1","managedFields":[]},"spec":{"replicas":1},"status":{"readyReplicas":1}});
        let mut actual = baseline.clone();
        actual["metadata"]["resourceVersion"] = json!("2");
        actual["metadata"]["managedFields"] = json!([{"manager":"controller"}]);
        assert_eq!(
            fingerprint(&actual).unwrap(),
            fingerprint(&baseline).unwrap()
        );
        for pointer in ["/metadata/uid", "/spec/replicas", "/status/readyReplicas"] {
            let mut drift = baseline.clone();
            *drift.pointer_mut(pointer).unwrap() = json!("changed");
            assert_ne!(
                fingerprint(&drift).unwrap(),
                fingerprint(&baseline).unwrap()
            );
        }
        actual["spec"]["extra"] = json!(true);
        assert_ne!(
            fingerprint(&actual).unwrap(),
            fingerprint(&baseline).unwrap()
        );
    }

    #[test]
    fn desired_comparison_allows_defaults_but_rejects_scalar_and_array_drift() {
        assert!(contains_desired(
            &json!({"a":1,"default":true}),
            &json!({"a":1})
        ));
        assert!(!contains_desired(&json!({"a":"1"}), &json!({"a":1})));
        assert!(!contains_desired(&json!([1, 2]), &json!([2, 1])));
        assert!(!contains_desired(&json!([1, 2]), &json!([1])));
    }

    #[test]
    fn job_absence_requires_observed_completion_and_declared_ttl() {
        let mut job = json!({"apiVersion":"batch/v1","kind":"Job","spec":{"ttlSecondsAfterFinished":0},"status":{"conditions":[{"type":"Complete","status":"True"}]}});
        assert!(completed_ttl_job(&job));
        job["status"]["conditions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"Failed","status":"True"}));
        assert!(!completed_ttl_job(&job));
        job["status"]["conditions"] = json!([]);
        assert!(!completed_ttl_job(&job));
    }

    #[test]
    fn completed_job_may_disappear_but_replacement_and_ordinary_absence_are_drift() {
        let identity = ObjectIdentity {
            group: "batch".into(),
            kind: "Job".into(),
            name: "bootstrap".into(),
            namespace: Some("test".into()),
        };
        let actual = json!({"metadata":{"uid":"completed-job"}});
        let mut observation = InstalledObjectObservation {
            identity: identity.clone(),
            state: InstalledObjectState::Present {
                uid: "completed-job".into(),
                digest: fingerprint(&actual).unwrap(),
                completed_ttl_job: true,
            },
        };
        assert!(unchanged(&[observation.clone()], &BTreeMap::new(), &BTreeSet::new()).unwrap());
        let replacement =
            BTreeMap::from([(identity.clone(), json!({"metadata":{"uid":"replacement"}}))]);
        assert!(!unchanged(&[observation.clone()], &replacement, &BTreeSet::new()).unwrap());
        let InstalledObjectState::Present {
            completed_ttl_job, ..
        } = &mut observation.state
        else {
            unreachable!()
        };
        *completed_ttl_job = false;
        assert!(!unchanged(&[observation.clone()], &BTreeMap::new(), &BTreeSet::new()).unwrap());
        observation.state = InstalledObjectState::CompletedHook;
        assert!(!unchanged(&[observation.clone()], &BTreeMap::new(), &BTreeSet::new()).unwrap());
        assert!(
            unchanged(
                &[observation],
                &BTreeMap::new(),
                &BTreeSet::from([identity])
            )
            .unwrap()
        );
    }
}
