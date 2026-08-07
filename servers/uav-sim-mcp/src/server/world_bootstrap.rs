use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{
    adapter::Adapter,
    contract::{SessionId, SimulationWorldBinding},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallationWorldBinding {
    session_id: SessionId,
    world: SimulationWorldBinding,
}

fn parse(document: &[u8]) -> Result<InstallationWorldBinding> {
    let binding: InstallationWorldBinding =
        serde_json::from_slice(document).context("decode installation world binding")?;
    let world = &binding.world;
    ensure!(
        world.spec_sha256.len() == 64
            && world
                .spec_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "installation world binding spec_sha256 must be 64 lowercase hexadecimal characters"
    );
    ensure!(
        world.simulation_frame_uri.revision_uri() == world.revision_uri,
        "installation simulation frame must belong to its immutable world revision"
    );
    let origin = &world.georeference_origin;
    ensure!(
        origin.latitude_degrees.is_finite() && (-90.0..=90.0).contains(&origin.latitude_degrees),
        "installation world latitude must be finite and within [-90, 90]"
    );
    ensure!(
        origin.longitude_degrees.is_finite()
            && (-180.0..=180.0).contains(&origin.longitude_degrees),
        "installation world longitude must be finite and within [-180, 180]"
    );
    ensure!(
        origin.ellipsoid_height_m.is_finite(),
        "installation world ellipsoid height must be finite"
    );
    Ok(binding)
}

pub(super) async fn apply(path: &Path, adapter: &Adapter) -> Result<()> {
    let document = tokio::fs::read(path)
        .await
        .with_context(|| format!("read installation world binding {}", path.display()))?;
    let binding = parse(&document)?;
    let result = adapter
        .configure_world_binding(&binding.session_id, &binding.world)
        .await
        .context("apply installation world binding to authoritative simulator")?;
    tracing::info!(
        session_id = %binding.session_id,
        revision_uri = %result.world.revision_uri,
        simulation_frame_uri = %result.world.simulation_frame_uri,
        "installation world binding applied"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
      "session_id":"session-alpha",
      "world":{
        "revision_uri":"frames://world/world-alpha/revision/revision-1",
        "spec_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "simulation_frame_uri":"frames://world/world-alpha/revision/revision-1/frame/isaac-world",
        "georeference_origin":{
          "latitude_degrees":40.758,
          "longitude_degrees":-73.9855,
          "ellipsoid_height_m":-17.0
        }
      }
    }"#;

    #[test]
    fn accepts_one_strict_immutable_binding() {
        let binding = parse(VALID.as_bytes()).unwrap();
        assert_eq!(binding.session_id.as_str(), "session-alpha");
        assert_eq!(
            binding.world.simulation_frame_uri.frame_id().as_str(),
            "isaac-world"
        );
    }

    #[test]
    fn rejects_cross_revision_and_malformed_digest() {
        let cross_revision = VALID.replace("revision-1/frame", "revision-2/frame");
        assert!(
            parse(cross_revision.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("must belong")
        );
        let uppercase = VALID.replacen(&"a".repeat(64), &"A".repeat(64), 1);
        assert!(
            parse(uppercase.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("lowercase hexadecimal")
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let unknown = VALID.replacen(
            "\"session_id\":\"session-alpha\"",
            "\"session_id\":\"session-alpha\",\"retry_seconds\":1",
            1,
        );
        assert!(
            parse(unknown.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("decode installation world binding")
        );
    }
}
