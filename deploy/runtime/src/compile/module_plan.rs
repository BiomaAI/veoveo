//! Produces installation declarations using the selected composition image.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{LoadedProfile, PlatformComponent, validate_module_plan};
use veoveo_modules::{ModulePlanDocument, ModuleSelectionDocument};

#[path = "module_plan_process.rs"]
mod process;

pub(crate) struct GeneratedModulePlan {
    pub plan: ModulePlanDocument,
    pub selection_path: std::path::PathBuf,
}

pub(super) fn prepare(
    profile: &LoadedProfile,
    image_digests: &BTreeMap<String, String>,
) -> Result<Option<GeneratedModulePlan>> {
    let platform = profile.resolved_platform()?;
    if !platform.components.contains(&PlatformComponent::Gateway) {
        return Ok(None);
    }
    let input = profile
        .definition
        .module_installation
        .as_ref()
        .context("gateway installation requires moduleInstallation selection")?;
    input.validate()?;
    let selection_path = profile.resolve(&input.selection);
    let bytes = fs::read(&selection_path).context("reading immutable public module selection")?;
    ensure!(bytes.len() <= 64 * 1024, "module selection exceeds 64 KiB");
    let selection: ModuleSelectionDocument =
        serde_json::from_slice(&bytes).context("decoding public module selection")?;
    let digest = image_digests
        .get("veoveo/mcp-gateway")
        .context("module plan requires the digest-pinned gateway composition image")?;
    // Publication and installation registry endpoints are explicit aliases of
    // the same artifact. Fetch through the host endpoint, execute only its digest.
    let image = format!(
        "{}/veoveo/mcp-gateway@{digest}",
        profile.definition.registry.push_address
    );
    let directory = tempfile::Builder::new()
        .prefix("veoveo-module-selection-")
        .tempdir()?;
    let mounted = directory.path().join("selection.json");
    fs::write(&mounted, serde_json::to_vec(&selection)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&mounted, fs::Permissions::from_mode(0o444))?;
    }
    let output = process::generate(&image, &mounted, digest)?;
    let plan: ModulePlanDocument =
        serde_json::from_slice(&output).context("decoding composition-generated module plan")?;
    validate_module_plan(&plan, &selection, digest, &platform)?;
    Ok(Some(GeneratedModulePlan {
        plan,
        selection_path,
    }))
}
