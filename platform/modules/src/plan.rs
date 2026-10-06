//! Generic generated-plan wire types; installation compositions own registrations.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SelectionWire", rename_all = "camelCase")]
pub struct ModuleSelectionDocument {
    format: String,
    enabled: Vec<ModuleName>,
    generation: InstallationGeneration,
    credential_revision: CredentialRevision,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SelectionWire {
    format: String,
    enabled: Vec<ModuleName>,
    generation: InstallationGeneration,
    credential_revision: CredentialRevision,
}
impl TryFrom<SelectionWire> for ModuleSelectionDocument {
    type Error = DeclarationError;
    fn try_from(wire: SelectionWire) -> Result<Self, Self::Error> {
        if wire.format != "veoveo.ai/module-selection/v1" {
            return Err(DeclarationError::new("unsupported module selection format"));
        }
        Self::new(wire.enabled, wire.generation, wire.credential_revision)
    }
}
impl ModuleSelectionDocument {
    pub fn new(
        mut enabled: Vec<ModuleName>,
        generation: InstallationGeneration,
        credential_revision: CredentialRevision,
    ) -> Result<Self, DeclarationError> {
        enabled.sort();
        if enabled.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(DeclarationError::new("duplicate enabled module"));
        }
        Ok(Self {
            format: "veoveo.ai/module-selection/v1".into(),
            enabled,
            generation,
            credential_revision,
        })
    }
    pub fn enabled(&self) -> &[ModuleName] {
        &self.enabled
    }
    pub fn generation(&self) -> InstallationGeneration {
        self.generation
    }
    pub fn credential_revision(&self) -> &CredentialRevision {
        &self.credential_revision
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RequirementDescriptor {
    pub module: ModuleName,
    pub minimum: Option<MigrationVersion>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LaneDescriptor {
    pub module: ModuleName,
    pub layer: ModuleLayer,
    pub requires: Vec<RequirementDescriptor>,
    pub image: ExecutionImage,
    pub command: ExecutionCommand,
    pub latest: Option<MigrationVersion>,
    pub lane_sha256: LaneIdentity,
}
/// Host predicate: every present key must be enabled. Separate rows express alternatives. Lane selection
/// never enables a host and does not require any host to exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeBinding {
    pub module: ModuleName,
    pub component: Option<RuntimeBindingKey>,
    pub mcp_server: Option<RuntimeBindingKey>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PlanWire", rename_all = "camelCase")]
pub struct ModulePlanDocument {
    format: String,
    composition: CompositionIdentity,
    enabled: Vec<ModuleName>,
    generation: InstallationGeneration,
    credential_revision: CredentialRevision,
    catalog: Vec<ModuleName>,
    lanes: Vec<LaneDescriptor>,
    runtime_bindings: Vec<ModuleRuntimeBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlanWire {
    format: String,
    composition: CompositionIdentity,
    enabled: Vec<ModuleName>,
    generation: InstallationGeneration,
    credential_revision: CredentialRevision,
    catalog: Vec<ModuleName>,
    lanes: Vec<LaneDescriptor>,
    runtime_bindings: Vec<ModuleRuntimeBinding>,
}
impl TryFrom<PlanWire> for ModulePlanDocument {
    type Error = DeclarationError;
    fn try_from(wire: PlanWire) -> Result<Self, Self::Error> {
        if wire.format != "veoveo.ai/module-plan/v1" {
            return Err(DeclarationError::new("unsupported module plan format"));
        }
        let selection =
            ModuleSelectionDocument::new(wire.enabled, wire.generation, wire.credential_revision)?;
        if wire.lanes.len() > 256 || wire.runtime_bindings.len() > 256 {
            return Err(DeclarationError::new("module plan exceeds catalog limit"));
        }
        let catalog: BTreeSet<_> = wire.catalog.iter().cloned().collect();
        if catalog.len() != wire.catalog.len() || catalog.len() > 256 {
            return Err(DeclarationError::new("invalid compiled module catalog"));
        }
        let mut names = BTreeSet::new();
        for lane in &wire.lanes {
            if !catalog.contains(&lane.module) || !names.insert(lane.module.clone()) {
                return Err(DeclarationError::new("duplicate module plan lane"));
            }
        }
        let mut completed = BTreeSet::new();
        for lane in &wire.lanes {
            let mut deps = BTreeSet::new();
            for required in &lane.requires {
                if required.module == lane.module
                    || !completed.contains(&required.module)
                    || !deps.insert(required.module.clone())
                {
                    return Err(DeclarationError::new(
                        "invalid or unordered plan prerequisite",
                    ));
                }
                let dependency = wire
                    .lanes
                    .iter()
                    .find(|other| other.module == required.module)
                    .expect("completed lane exists");
                if lane.layer == ModuleLayer::Kernel && dependency.layer != ModuleLayer::Kernel {
                    return Err(DeclarationError::new(
                        "kernel plan lane depends on optional lane",
                    ));
                }
                if required
                    .minimum
                    .is_some_and(|minimum| dependency.latest.is_none_or(|latest| latest < minimum))
                {
                    return Err(DeclarationError::new(
                        "plan prerequisite exceeds compiled lane",
                    ));
                }
            }
            completed.insert(lane.module.clone());
        }
        for enabled in selection.enabled() {
            if wire
                .lanes
                .iter()
                .find(|lane| &lane.module == enabled)
                .is_none_or(|lane| lane.layer != ModuleLayer::Optional)
            {
                return Err(DeclarationError::new(
                    "enabled plan module is not a selected optional lane",
                ));
            }
        }
        let mut bindings = BTreeSet::new();
        for binding in &wire.runtime_bindings {
            if (binding.component.is_none() && binding.mcp_server.is_none())
                || !catalog.contains(&binding.module)
                || !bindings.insert((
                    binding.module.clone(),
                    binding.component.clone(),
                    binding.mcp_server.clone(),
                ))
            {
                return Err(DeclarationError::new("invalid plan runtime binding"));
            }
        }
        Ok(Self {
            format: wire.format,
            composition: wire.composition,
            enabled: selection.enabled,
            generation: selection.generation,
            credential_revision: selection.credential_revision,
            catalog: wire.catalog,
            lanes: wire.lanes,
            runtime_bindings: wire.runtime_bindings,
        })
    }
}
impl ModulePlanDocument {
    pub fn generate(
        registry: &ModuleRegistry,
        selection: &ModuleSelectionDocument,
        composition: CompositionIdentity,
        bindings: Vec<ModuleRuntimeBinding>,
    ) -> Result<Self, DeclarationError> {
        for name in selection.enabled() {
            if registry
                .module(name)
                .is_none_or(|module| module.layer() != ModuleLayer::Optional)
            {
                return Err(DeclarationError::new(
                    "enabled modules must name compiled optional modules",
                ));
            }
        }
        let selected = registry.select(selection.enabled().to_vec())?;
        let lanes = selected
            .ordered()
            .iter()
            .map(|module| {
                Ok(LaneDescriptor {
                    module: module.name().clone(),
                    layer: module.layer(),
                    requires: module
                        .requires()
                        .iter()
                        .map(|r| RequirementDescriptor {
                            module: r.module().clone(),
                            minimum: r.minimum(),
                        })
                        .collect(),
                    image: module.execution().image().clone(),
                    command: module.execution().command().clone(),
                    latest: module.lane().latest(),
                    lane_sha256: lane_identity(module)?,
                })
            })
            .collect::<Result<Vec<_>, DeclarationError>>()?;
        let mut runtime_bindings = bindings;
        runtime_bindings.sort_by(|a, b| {
            (&a.module, &a.component, &a.mcp_server).cmp(&(&b.module, &b.component, &b.mcp_server))
        });
        Self::try_from(PlanWire {
            format: "veoveo.ai/module-plan/v1".into(),
            composition,
            enabled: selection.enabled.clone(),
            generation: selection.generation,
            credential_revision: selection.credential_revision.clone(),
            catalog: registry
                .modules()
                .iter()
                .map(|m| m.name().clone())
                .collect(),
            lanes,
            runtime_bindings,
        })
    }
    pub fn composition(&self) -> &CompositionIdentity {
        &self.composition
    }
    pub fn enabled(&self) -> &[ModuleName] {
        &self.enabled
    }
    pub fn generation(&self) -> InstallationGeneration {
        self.generation
    }
    pub fn credential_revision(&self) -> &CredentialRevision {
        &self.credential_revision
    }
    pub fn catalog(&self) -> &[ModuleName] {
        &self.catalog
    }
    pub fn lanes(&self) -> &[LaneDescriptor] {
        &self.lanes
    }
    pub fn runtime_bindings(&self) -> &[ModuleRuntimeBinding] {
        &self.runtime_bindings
    }
    pub fn selection(&self) -> Result<ModuleSelectionDocument, DeclarationError> {
        ModuleSelectionDocument::new(
            self.enabled.clone(),
            self.generation,
            self.credential_revision.clone(),
        )
    }
}
fn lane_identity(module: &ModuleSetup) -> Result<LaneIdentity, DeclarationError> {
    let mut hash = Sha256::new();
    fn part(hash: &mut Sha256, value: &[u8]) {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    part(&mut hash, b"veoveo.ai/module-lane/v1");
    part(&mut hash, module.name().as_str().as_bytes());
    for claim in module.ownership() {
        part(
            &mut hash,
            match claim.kind() {
                ObjectKind::Table => b"table",
                ObjectKind::Function => b"function",
                ObjectKind::Analyzer => b"analyzer",
            },
        );
        part(&mut hash, if claim.prefix() { b"prefix" } else { b"exact" });
        part(&mut hash, claim.text().as_bytes());
    }
    for migration in module.lane().migrations() {
        part(&mut hash, &migration.version().get().to_be_bytes());
        part(&mut hash, migration.name().as_str().as_bytes());
        part(&mut hash, migration.filename().as_bytes());
        part(&mut hash, migration.sql().as_bytes());
        for required in migration.requires() {
            part(&mut hash, required.module().as_str().as_bytes());
            part(
                &mut hash,
                &required
                    .minimum()
                    .map(|v| v.get().to_be_bytes().to_vec())
                    .unwrap_or_default(),
            );
        }
    }
    let hex: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    LaneIdentity::new(format!("sha256:{hex}"))
}

/// Shared installation identity. Preserve the v1 full-plan/username length framing.
pub fn preparation_key(
    plan: &ModulePlanDocument,
    runtime_username: &str,
) -> Result<PreparationKey, DeclarationError> {
    let mut hash = Sha256::new();
    for bytes in [
        b"veoveo.ai/installation-preparation/v1".as_slice(),
        serde_json::to_vec(plan)
            .map_err(|_| DeclarationError::new("cannot encode preparation plan"))?
            .as_slice(),
        runtime_username.as_bytes(),
    ] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    PreparationKey::new(plan.generation(), digest)
}
