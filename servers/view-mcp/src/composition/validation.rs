use super::*;

impl ResolvedSceneComposition {
    pub fn new(
        record: SceneComposition,
        resolved_overlays: Vec<ResolvedSceneOverlay>,
        artifact_bytes: BTreeMap<SceneInputId, ResolvedArtifactBytes>,
    ) -> anyhow::Result<Self> {
        let resolved = Self {
            record,
            resolved_overlays,
            artifact_bytes,
        };
        resolved.validate()?;
        Ok(resolved)
    }

    pub fn record(&self) -> &SceneComposition {
        &self.record
    }

    /// Check retained bytes and resolved geometry against the immutable public record.
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.resolved_overlays.len() == self.record.overlays().len(),
            "resolved overlay count disagrees with composition"
        );
        let inputs = self
            .record
            .governed_inputs()
            .iter()
            .map(|input| (&input.input_id, input))
            .collect::<BTreeMap<_, _>>();
        let total_bytes = self
            .artifact_bytes
            .values()
            .try_fold(0_u64, |total, bytes| {
                anyhow::ensure!(
                    bytes.0.len() as u64 <= MAX_OVERLAY_ARTIFACT_BYTES,
                    "retained artifact exceeds per-artifact limit"
                );
                let total = total.saturating_add(bytes.0.len() as u64);
                anyhow::ensure!(
                    total <= MAX_COMPOSITION_ARTIFACT_BYTES,
                    "retained artifacts exceed composition limit"
                );
                Ok::<_, anyhow::Error>(total)
            })?;
        let mut used_artifacts = BTreeSet::new();
        for (overlay, resolved) in self.record.overlays().iter().zip(&self.resolved_overlays) {
            anyhow::ensure!(
                &resolved.overlay == overlay,
                "resolved overlay identity or metadata disagrees with composition"
            );
            let geometry = match &overlay.geometry {
                SceneOverlayGeometrySource::Inline { geometry } => geometry.clone(),
                SceneOverlayGeometrySource::Artifact { input_id } => {
                    let bytes =
                        self.checked_artifact(input_id, OVERLAY_ARTIFACT_MIME_TYPE, &inputs)?;
                    used_artifacts.insert(input_id.clone());
                    serde_json::from_slice(bytes).context("decoding retained overlay geometry")?
                }
            };
            anyhow::ensure!(
                geometry == resolved.geometry,
                "resolved geometry disagrees with its declared input"
            );
            validate_artifact_geometry(&geometry, &inputs)?;
            anyhow::ensure!(
                self.record.local_frame().is_some()
                    || !geometry
                        .positions()
                        .any(|position| matches!(position, ScenePosition::LocalMeters { .. })),
                "resolved local geometry requires a Frames binding"
            );
            if let SceneOverlayGeometry::OrientedMeshInstance { mesh_input_id, .. } = &geometry {
                let bytes = self.checked_artifact(mesh_input_id, GLB_MIME_TYPE, &inputs)?;
                decode_glb(bytes).context("validating retained GLB overlay")?;
                used_artifacts.insert(mesh_input_id.clone());
            }
        }
        anyhow::ensure!(
            used_artifacts.len() == self.artifact_bytes.len(),
            "retained composition contains unreferenced artifact bytes"
        );
        debug_assert!(total_bytes <= MAX_COMPOSITION_ARTIFACT_BYTES);
        Ok(())
    }

    fn checked_artifact<'a>(
        &'a self,
        input_id: &SceneInputId,
        media_type: &str,
        inputs: &BTreeMap<&SceneInputId, &GovernedSceneInput>,
    ) -> anyhow::Result<&'a [u8]> {
        let input = inputs
            .get(input_id)
            .context("retained artifact input is undeclared")?;
        anyhow::ensure!(
            input.resource_uri.is_artifact() && input.media_type.as_deref() == Some(media_type),
            "retained artifact input has the wrong resource or media type"
        );
        let bytes = self
            .artifact_bytes
            .get(input_id)
            .context("retained artifact bytes are missing")?
            .as_slice();
        anyhow::ensure!(
            Sha256Digest::from_bytes(bytes) == input.digest_sha256,
            "retained artifact digest disagrees with governed input"
        );
        Ok(bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ResolvedSceneCompositionWire {
    record: SceneComposition,
    resolved_overlays: Vec<ResolvedSceneOverlay>,
    artifact_bytes: BTreeMap<SceneInputId, ResolvedArtifactBytes>,
}

impl TryFrom<ResolvedSceneCompositionWire> for ResolvedSceneComposition {
    type Error = anyhow::Error;
    fn try_from(value: ResolvedSceneCompositionWire) -> Result<Self, Self::Error> {
        Self::new(value.record, value.resolved_overlays, value.artifact_bytes)
    }
}

#[cfg(test)]
mod tests;
