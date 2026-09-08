use crate::{
    compile::objects::object_digest, helm_state::release_metadata, ownership::read_objects,
};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use veoveo_deploy_contract::components::*;

impl super::InstalledState {
    pub(crate) fn unselected(
        &self,
        catalog: &[LockedComponent],
        selected: &BTreeSet<ComponentId>,
    ) -> Result<UnselectedState> {
        let components = catalog
            .iter()
            .filter(|component| !selected.contains(&component.declaration.id))
            .collect::<Vec<_>>();
        let identities = components
            .iter()
            .flat_map(|component| &component.declaration.permitted_objects)
            .cloned()
            .collect();
        let live = read_objects(&self.context, &identities, None)?;
        let objects = identities
            .into_iter()
            .map(|identity| {
                let state = live
                    .get(&identity)
                    .map(|object| {
                        let text = |key| {
                            object
                                .pointer(key)
                                .and_then(serde_json::Value::as_str)
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned)
                                .context("unselected object has no API version identity")
                        };
                        Ok::<_, anyhow::Error>(ObservedObjectVersion {
                            uid: text("/metadata/uid")?,
                            resource_version: text("/metadata/resourceVersion")?,
                            digest: object_digest(object)?,
                        })
                    })
                    .transpose()?;
                Ok(ObjectSnapshot { identity, state })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut releases = Vec::new();
        for component in components {
            for target in &component.declaration.targets {
                if let AtomicTarget::HelmRelease { namespace, name } = target {
                    let state = release_metadata(&self.context, namespace, name)?
                        .map(|release| {
                            Ok::<_, anyhow::Error>(ObservedReleaseVersion {
                                revision: release.revision.value()?,
                                status: release.status,
                                chart: release.chart,
                                app_version: release.app_version,
                            })
                        })
                        .transpose()?;
                    releases.push(ReleaseSnapshot {
                        target: target.clone(),
                        state,
                    });
                }
            }
        }
        Ok(UnselectedState { objects, releases })
    }
}
