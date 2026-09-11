use crate::WorkerError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_computers::maintenance::MaintenanceOperation;
use veoveo_computers_runtime::DevelopmentTemplate;

/// Installation declaration of a tested directed image transition. This is not
/// an end-user template/image selector and declaration alone is not qualification.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceTransition {
    pub source_fingerprint: String,
    pub target_fingerprint: String,
}
#[derive(Clone)]
pub struct MaintenanceProfiles {
    templates: BTreeMap<String, DevelopmentTemplate>,
    transitions: BTreeSet<MaintenanceTransition>,
}
impl MaintenanceProfiles {
    pub fn new(
        templates: Vec<DevelopmentTemplate>,
        transitions: Vec<MaintenanceTransition>,
    ) -> Result<Self, WorkerError> {
        if templates.is_empty() || templates.len() > 64 || transitions.len() > 256 {
            return Err(WorkerError::Configuration);
        }
        let mut admitted = BTreeMap::new();
        for template in templates {
            if template.persistent_home().is_none()
                || admitted.insert(template.fingerprint(), template).is_some()
            {
                return Err(WorkerError::Configuration);
            }
        }
        let mut selected = BTreeSet::new();
        for transition in transitions {
            let source = admitted
                .get(&transition.source_fingerprint)
                .ok_or(WorkerError::Configuration)?;
            let target = admitted
                .get(&transition.target_fingerprint)
                .ok_or(WorkerError::Configuration)?;
            source
                .check_replacement_profile(target)
                .map_err(|_| WorkerError::Configuration)?;
            if !selected.insert(transition) {
                return Err(WorkerError::Configuration);
            }
        }
        Ok(Self {
            templates: admitted,
            transitions: selected,
        })
    }
    pub fn admits(&self, source: &str, target: &str) -> bool {
        self.transitions.contains(&MaintenanceTransition {
            source_fingerprint: source.into(),
            target_fingerprint: target.into(),
        })
    }
    pub(crate) fn matches_catalog(&self, templates: &[DevelopmentTemplate]) -> bool {
        templates.len() == self.templates.len()
            && templates
                .iter()
                .all(|t| self.templates.contains_key(&t.fingerprint()))
    }
    pub(super) fn for_operation(
        &self,
        operation: &MaintenanceOperation,
    ) -> Result<(&DevelopmentTemplate, &DevelopmentTemplate), WorkerError> {
        if !self.admits(
            &operation.source_template_fingerprint,
            &operation.target.template_fingerprint,
        ) {
            return Err(WorkerError::Configuration);
        }
        Ok((
            self.templates
                .get(&operation.source_template_fingerprint)
                .ok_or(WorkerError::Configuration)?,
            self.templates
                .get(&operation.target.template_fingerprint)
                .ok_or(WorkerError::Configuration)?,
        ))
    }
}
