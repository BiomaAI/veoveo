use super::*;
use crate::contract::test_support as fixture;

pub(crate) fn resolved() -> ResolvedSceneComposition {
    let record = fixture::composition();
    let resolved_overlays = record
        .overlays()
        .iter()
        .map(|overlay| {
            let SceneOverlayGeometrySource::Inline { geometry } = &overlay.geometry else {
                panic!("inline fixture")
            };
            ResolvedSceneOverlay {
                overlay: overlay.clone(),
                geometry: geometry.clone(),
            }
        })
        .collect();
    ResolvedSceneComposition {
        record,
        resolved_overlays,
        artifact_bytes: BTreeMap::new(),
    }
}

pub(crate) fn with_artifact() -> ResolvedSceneComposition {
    let mut resolved = resolved();
    let mut request = fixture::request();
    let bytes = serde_json::to_vec(&resolved.resolved_overlays[0].geometry).unwrap();
    let input_id = request.governed_inputs[0].input_id.clone();
    request.governed_inputs[0].digest_sha256 = Sha256Digest::from_bytes(&bytes);
    request.governed_inputs[0].media_type = Some(OVERLAY_ARTIFACT_MIME_TYPE.into());
    request.overlays[0].geometry = SceneOverlayGeometrySource::Artifact {
        input_id: input_id.clone(),
    };
    resolved.resolved_overlays[0].overlay = request.overlays[0].clone();
    resolved.record = SceneComposition::new(request, fixture::authority(), fixture::now()).unwrap();
    resolved
        .artifact_bytes
        .insert(input_id, ResolvedArtifactBytes(bytes));
    resolved
}
