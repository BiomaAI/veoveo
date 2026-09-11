//! Pure template validation shared by executable admission and qualification.
use super::{ConfigurationError, NamedTemplate, Result, Templates};
use serde::Deserialize;
use veoveo_computers_runtime::{
    DevelopmentTemplate, PERSISTENT_COMMAND, PersistentHome, parse_policy,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Template {
    id: String,
    fingerprint: String,
    image: String,
    cpus: u32,
    memory_mib: u32,
    home_capacity_mib: u32,
    temporary_mib: u32,
    // Private provider policy uses its generated protobuf JSON mapping.
    policy: serde_json::Value,
}
pub(super) fn catalog(templates: &[Template], default: &str) -> Result<Templates> {
    if templates.is_empty() || templates.len() > 64 {
        return Err(ConfigurationError::Templates);
    }
    let mut admitted = Vec::with_capacity(templates.len());
    for (index, t) in templates.iter().enumerate() {
        let invalid = || ConfigurationError::Template { index };
        let runtime = DevelopmentTemplate::new(
            t.image.clone(),
            t.cpus,
            t.memory_mib,
            parse_policy(&t.policy).map_err(|_| invalid())?,
            PERSISTENT_COMMAND.map(str::to_owned).into(),
            Some(PersistentHome::new(t.home_capacity_mib, t.temporary_mib).map_err(|_| invalid())?),
        )
        .map_err(|_| invalid())?;
        if runtime.fingerprint() != t.fingerprint {
            return Err(invalid());
        }
        admitted.push(NamedTemplate::new(t.id.clone(), runtime).map_err(|_| invalid())?);
    }
    Templates::new(admitted, Some(default.to_owned())).map_err(|_| ConfigurationError::Templates)
}
