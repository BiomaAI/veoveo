//! Keep Deployment child identities until every Pod has actually disappeared.
use std::{
    collections::BTreeSet,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};

use super::{Deployment, PodList, ReplicaSetList, controlled_by, nonempty};
use crate::process::output_checked;

#[derive(Clone, Debug)]
pub(crate) struct Quiescence {
    deployment_uid: String,
    replica_sets: BTreeSet<String>,
    pods: BTreeSet<String>,
}

impl Quiescence {
    pub(crate) fn capture(context: &str, namespace: &str, deployment_uid: &str) -> Result<Self> {
        let mut observation = Self {
            deployment_uid: nonempty(deployment_uid, "quiesce Deployment UID")?.into(),
            replica_sets: BTreeSet::new(),
            pods: BTreeSet::new(),
        };
        observation.remaining(context, namespace)?;
        Ok(observation)
    }

    pub(crate) fn wait(&self, context: &str, namespace: &str, deployment: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut observation = self.clone();
        loop {
            observation.verify_scaled_down(context, namespace, deployment)?;
            let remaining = observation.remaining(context, namespace)?;
            if remaining.is_empty() {
                // Detect recreation or concurrent scaling during the child reads.
                return observation.verify_scaled_down(context, namespace, deployment);
            }
            ensure!(
                Instant::now() < deadline,
                "GPU Deployment {namespace}/{deployment} still owns Pods after quiesce timeout: {}",
                remaining.join(", ")
            );
            thread::sleep(Duration::from_secs(1));
        }
    }

    fn verify_scaled_down(&self, context: &str, namespace: &str, name: &str) -> Result<()> {
        let bytes = output_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                namespace,
                "get",
                "deployment",
                name,
                "--output=json",
                "--request-timeout=10s",
            ],
            None,
        )?;
        let deployment: Deployment =
            serde_json::from_slice(&bytes).context("decoding quiesced Deployment")?;
        ensure!(
            deployment.metadata.uid == self.deployment_uid
                && deployment.metadata.deletion_timestamp.is_none()
                && deployment.spec.replicas == 0,
            "GPU Deployment {namespace}/{name} changed while waiting for its Pods to exit"
        );
        Ok(())
    }

    fn remaining(&mut self, context: &str, namespace: &str) -> Result<Vec<String>> {
        // Read all revisions and terminating children. Labels or current-rollout
        // readiness alone can hide Pods that still hold the old GPU allocation.
        let replica_sets = output_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                namespace,
                "get",
                "replicasets",
                "--output=json",
                "--request-timeout=10s",
            ],
            None,
        )?;
        let pods = output_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                namespace,
                "get",
                "pods",
                "--output=json",
                "--request-timeout=10s",
            ],
            None,
        )?;
        self.observe(
            &serde_json::from_slice(&replica_sets).context("decoding quiesce ReplicaSets")?,
            &serde_json::from_slice(&pods).context("decoding quiesce Pods")?,
        )
    }

    fn observe(&mut self, replica_sets: &ReplicaSetList, pods: &PodList) -> Result<Vec<String>> {
        for replica_set in &replica_sets.items {
            if controlled_by(&replica_set.metadata, "Deployment", &self.deployment_uid) {
                self.replica_sets
                    .insert(nonempty(&replica_set.metadata.uid, "quiesce ReplicaSet UID")?.into());
            }
        }
        let mut remaining = Vec::new();
        for pod in &pods.items {
            if self.pods.contains(&pod.metadata.uid)
                || self
                    .replica_sets
                    .iter()
                    .any(|uid| controlled_by(&pod.metadata, "ReplicaSet", uid))
            {
                self.pods
                    .insert(nonempty(&pod.metadata.uid, "quiesce Pod UID")?.into());
                remaining.push(nonempty(&pod.metadata.name, "quiesce Pod name")?.into());
            }
        }
        remaining.sort();
        Ok(remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terminating_children_survive_replica_set_deletion_and_owner_changes() {
        let mut observation = Quiescence {
            deployment_uid: "deployment".into(),
            replica_sets: BTreeSet::new(),
            pods: BTreeSet::new(),
        };
        let sets: ReplicaSetList = serde_json::from_value(json!({"items":[
            {"metadata":{"uid":"old", "ownerReferences":[{"kind":"Deployment","uid":"deployment","controller":true}]}},
            {"metadata":{"uid":"new", "ownerReferences":[{"kind":"Deployment","uid":"deployment","controller":true}]}}
        ]})).unwrap();
        let mut pods: PodList = serde_json::from_value(json!({"items":[
            {"metadata":{"name":"terminating", "uid":"pod-old", "deletionTimestamp":"2026-09-08T00:00:00Z", "ownerReferences":[{"kind":"ReplicaSet","uid":"old","controller":true}]}},
            {"metadata":{"name":"current", "uid":"pod-new", "ownerReferences":[{"kind":"ReplicaSet","uid":"new","controller":true}]}},
            {"metadata":{"name":"unrelated", "uid":"pod-other", "ownerReferences":[{"kind":"ReplicaSet","uid":"other","controller":true}]}}
        ]})).unwrap();
        assert_eq!(
            observation.observe(&sets, &pods).unwrap(),
            ["current", "terminating"]
        );
        let absent_sets = ReplicaSetList { items: vec![] };
        pods.items[0].metadata.owner_references.clear();
        assert_eq!(
            observation.observe(&absent_sets, &pods).unwrap(),
            ["current", "terminating"]
        );
        pods.items.retain(|pod| pod.metadata.name == "unrelated");
        assert!(observation.observe(&absent_sets, &pods).unwrap().is_empty());
    }
}
