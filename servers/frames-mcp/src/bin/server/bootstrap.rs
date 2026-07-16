use std::{collections::BTreeSet, path::Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use veoveo_frames_mcp::state::{FrameScope, FramesState};
use veoveo_mcp_contract::{
    FrameId, FrameKind, SERVER_BOOTSTRAP_ISSUER, ServerBootstrapDocument, ServerSlug,
    server_bootstrap_principal,
};
use veoveo_platform_store::{PlatformStore, PrincipalKind};
use veoveo_rrd::RrdFrameDefinition;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FramesBootstrapPayload {
    #[serde(default)]
    frames: Vec<RrdFrameDefinition>,
}

fn frames_slug() -> ServerSlug {
    ServerSlug::new(super::SERVER_SLUG).expect("frames is a valid server slug")
}

fn decode(bytes: &[u8]) -> Result<(ServerBootstrapDocument, FramesBootstrapPayload)> {
    let document = ServerBootstrapDocument::decode_for(&frames_slug(), bytes)?;
    let payload: FramesBootstrapPayload = document.payload()?;
    let mut frame_ids = BTreeSet::new();
    for frame in &payload.frames {
        validate_frame(frame)?;
        if !frame_ids.insert(frame.frame_id.as_str()) {
            bail!(
                "bootstrap frame `{}` appears more than once",
                frame.frame_id
            );
        }
    }
    Ok((document, payload))
}

fn validate_frame(frame: &RrdFrameDefinition) -> Result<()> {
    let frame_id = FrameId::new(frame.frame_id.as_str())?;
    if matches!(frame_id.as_str(), "WGS84" | "ECEF") {
        bail!("builtin frame `{frame_id}` is immutable");
    }
    if !matches!(frame.kind, FrameKind::Enu | FrameKind::Ned) {
        bail!("bootstrap frame `{frame_id}` must be ENU or NED");
    }
    if frame.parent.as_ref().map(|parent| parent.as_str()) != Some("WGS84") {
        bail!("bootstrap frame `{frame_id}` must have WGS84 as its parent");
    }
    frame
        .origin
        .as_ref()
        .context("bootstrap local frame requires a WGS84 origin")?
        .validate()
        .map_err(anyhow::Error::msg)?;
    frame
        .view_coordinates
        .as_ref()
        .context("bootstrap local frame requires view coordinates")?
        .to_rerun_view_coordinates()
        .map_err(anyhow::Error::msg)?;
    Ok(())
}

pub(super) async fn run_validate(path: &Path) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("reading Frames bootstrap document {}", path.display()))?;
    let (document, payload) = decode(&bytes)
        .with_context(|| format!("validating Frames bootstrap document {}", path.display()))?;
    println!(
        "ok: tenant `{}`, {} frame(s)",
        document.tenant_key,
        payload.frames.len()
    );
    Ok(())
}

pub(super) async fn apply(path: &Path, store: &PlatformStore, frames: &FramesState) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("reading Frames bootstrap document {}", path.display()))?;
    let (document, payload) = decode(&bytes)
        .with_context(|| format!("decoding Frames bootstrap document {}", path.display()))?;
    let principal = server_bootstrap_principal(&frames_slug());
    let identity = store
        .ensure_identity(
            &document.tenant_key,
            &principal,
            SERVER_BOOTSTRAP_ISSUER,
            &principal,
            PrincipalKind::Service,
        )
        .await?;
    let scope = FrameScope {
        identity,
        data_labels: BTreeSet::new(),
    };

    for frame in payload.frames {
        let frame_id = FrameId::new(frame.frame_id.as_str())?;
        if frames.get_frame(&scope, &frame_id).await?.is_some() {
            tracing::info!(frame = %frame_id, "Frames bootstrap frame already registered");
            continue;
        }
        frames.insert_frame(&scope, frame).await?;
        tracing::info!(frame = %frame_id, "Frames bootstrap frame registered");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(payload: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "server": "frames",
            "tenant_key": "bioma",
            "payload": payload,
        }))
        .expect("envelope serializes")
    }

    fn enu_frame() -> serde_json::Value {
        serde_json::json!({
            "frame_id": "bioma-uav-origin",
            "kind": "enu",
            "view_coordinates": {"x": "right", "y": "forward", "z": "up"},
            "parent": "WGS84",
            "origin": {
                "latitude_deg": 13.6929,
                "longitude_deg": -89.2182,
                "height_m": 700.0
            },
            "datum": "WGS84",
            "ellipsoid": "WGS84"
        })
    }

    #[test]
    fn payload_is_typed_and_validates_local_frames() {
        let (document, payload) = decode(&envelope(serde_json::json!({
            "frames": [enu_frame()]
        })))
        .expect("valid ENU bootstrap decodes");
        assert_eq!(document.tenant_key, "bioma");
        assert_eq!(payload.frames.len(), 1);
    }

    #[test]
    fn unknown_payload_fields_fail_closed() {
        let error = decode(&envelope(serde_json::json!({
            "frames": [],
            "legacy_frames": []
        })))
        .expect_err("unknown fields must fail");
        assert!(error.to_string().contains("legacy_frames"));
    }

    #[test]
    fn invalid_or_duplicate_frames_fail_closed() {
        let mut invalid = enu_frame();
        invalid["parent"] = serde_json::json!("ECEF");
        assert!(decode(&envelope(serde_json::json!({"frames": [invalid]}))).is_err());
        let frame = enu_frame();
        assert!(
            decode(&envelope(
                serde_json::json!({"frames": [frame.clone(), frame]})
            ))
            .is_err()
        );
    }

    #[test]
    fn mistargeted_documents_fail_closed() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "server": "map",
            "tenant_key": "bioma",
            "payload": {"frames": []}
        }))
        .expect("envelope serializes");
        assert!(decode(&bytes).is_err());
    }
}
