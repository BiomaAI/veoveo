//! Publish through Frames before preparing the simulator's installation input.

use super::*;
use veoveo_frames_mcp::contract::{CreateWorldRequest, PublishWorldOutput, PublishWorldRequest};
use veoveo_uav_sim_mcp::contract::{InstallationWorldBinding, SimulationWorldBinding};

pub(super) async fn publish_world_revision(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<FrameWorldRevision> {
    let tree_digest = hex::encode(Sha256::digest(serde_json::to_vec(&scenario.world.tree)?));
    let world_id = FrameWorldId::parse(format!(
        "{}-{}",
        scenario.world.world_id,
        &tree_digest[..16]
    ))?;
    operator
        .call_tool(
            "frames__create_world",
            serde_json::to_value(CreateWorldRequest {
                world_id: world_id.clone(),
                display_name: scenario.world.display_name.clone(),
                description: Some(scenario.world.description.clone()),
            })?,
        )
        .await?;
    let publication: PublishWorldOutput = serde_json::from_value(
        operator
            .call_tool(
                "frames__publish_world",
                serde_json::to_value(PublishWorldRequest {
                    world_id: world_id.clone(),
                    expected_head_revision_id: None,
                    tree: scenario.world.tree.clone(),
                })?,
            )
            .await?,
    )
    .context("decoding the Frames publication")?;
    ensure!(
        publication.revision.world_id() == world_id,
        "Frames returned a revision from a different world"
    );
    let frame_uri = WorldFrameUri::new(
        publication.revision.revision_uri(),
        &scenario.world.simulation_frame_id,
    );
    verify_published_world(
        operator,
        scenario,
        publication.revision.revision_uri(),
        &frame_uri,
        Some(&world_id),
    )
    .await
}

pub(crate) async fn uav_world_publish(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
    output: &Path,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    installation.operator.validate_credentials()?;
    let operator = OperatorClient {
        conformance,
        installation,
    };
    let revision = publish_world_revision(&operator, &scenario).await?;
    let frame_uri =
        WorldFrameUri::new(revision.revision_uri(), &scenario.world.simulation_frame_id);
    let binding = InstallationWorldBinding::new(
        scenario.session_id,
        SimulationWorldBinding::from_revision(&revision, &frame_uri)?,
    )?;
    let mut document = serde_json::to_vec_pretty(&binding)?;
    document.push(b'\n');
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &document)
        .with_context(|| format!("writing world binding {}", output.display()))?;
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Receipt<'a> {
        schema: &'static str,
        binding_file: &'a Path,
        content_sha256: String,
        binding: &'a InstallationWorldBinding,
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&Receipt {
            schema: "veoveo.ai/uav-world-publication/v1",
            binding_file: output,
            content_sha256: hex::encode(Sha256::digest(&document)),
            binding: &binding,
        })?
    );
    Ok(())
}
