use super::*;

/// A validated immutable scene, including its request, authority and content identity.
///
/// ```compile_fail
/// use veoveo_view_mcp::SceneComposition;
/// fn change_parent(scene: &mut SceneComposition) { scene.revision = 2; }
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(try_from = "SceneCompositionWire")]
pub struct SceneComposition(SceneCompositionWire);

impl SceneComposition {
    pub fn new(
        mut request: CreateSceneCompositionRequest,
        authority: SceneCompositionAuthority,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SceneCompositionError> {
        request.validate()?;
        request
            .governed_inputs
            .sort_by(|left, right| left.input_id.cmp(&right.input_id));
        let request_digest = digest_json(&request)?;
        let authority_digest = digest_json(&authority)?;
        let stable_key = format!(
            "{}:{}:{}",
            SCENE_COMPOSITION_ALGORITHM_REVISION, authority_digest, request_digest
        );
        let composition_id = SceneCompositionId::from_stable_key(stable_key.as_bytes());
        let mut record = SceneCompositionWire {
            schema_version: request.schema_version,
            composition_uri: super::super::CompositionUri::new(composition_id.clone()),
            composition_id,
            revision: 1,
            base_layer: request.base_layer,
            map_releases: request.map_releases,
            local_frame: request.local_frame,
            style_id: request.style_id,
            governed_inputs: request.governed_inputs,
            overlays: request.overlays,
            algorithm_revision: SCENE_COMPOSITION_ALGORITHM_REVISION.to_owned(),
            request_digest_sha256: request_digest,
            composition_digest_sha256: Sha256Digest::from_bytes(&[]),
            authority,
            created_at,
        };
        let mut content =
            serde_json::to_value(&record).map_err(|_| SceneCompositionError::Serialization)?;
        let fields = content
            .as_object_mut()
            .ok_or(SceneCompositionError::Serialization)?;
        fields.remove("composition_digest_sha256");
        fields.remove("created_at");
        record.composition_digest_sha256 = digest_json(&content)?;
        Ok(Self(record))
    }
    pub fn schema_version(&self) -> u64 {
        self.0.schema_version
    }
    pub fn composition_id(&self) -> &SceneCompositionId {
        &self.0.composition_id
    }
    pub fn composition_uri(&self) -> &super::super::CompositionUri {
        &self.0.composition_uri
    }
    pub fn revision(&self) -> u64 {
        self.0.revision
    }
    pub fn base_layer(&self) -> &LayerId {
        &self.0.base_layer
    }
    pub fn map_releases(&self) -> &BTreeSet<MapReleaseUri> {
        &self.0.map_releases
    }
    pub fn local_frame(&self) -> Option<&LocalFrameBinding> {
        self.0.local_frame.as_ref()
    }
    pub fn style_id(&self) -> &SceneStyleId {
        &self.0.style_id
    }
    pub fn governed_inputs(&self) -> &[GovernedSceneInput] {
        &self.0.governed_inputs
    }
    pub fn overlays(&self) -> &[SceneOverlay] {
        &self.0.overlays
    }
    pub fn algorithm_revision(&self) -> &str {
        &self.0.algorithm_revision
    }
    pub fn request_digest_sha256(&self) -> &Sha256Digest {
        &self.0.request_digest_sha256
    }
    pub fn composition_digest_sha256(&self) -> &Sha256Digest {
        &self.0.composition_digest_sha256
    }
    pub fn authority(&self) -> &SceneCompositionAuthority {
        &self.0.authority
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
}

impl TryFrom<SceneCompositionWire> for SceneComposition {
    type Error = SceneCompositionError;

    fn try_from(value: SceneCompositionWire) -> Result<Self, Self::Error> {
        let request = CreateSceneCompositionRequest {
            schema_version: value.schema_version,
            base_layer: value.base_layer.clone(),
            map_releases: value.map_releases.clone(),
            local_frame: value.local_frame.clone(),
            style_id: value.style_id.clone(),
            governed_inputs: value.governed_inputs.clone(),
            overlays: value.overlays.clone(),
        };
        let expected = Self::new(request, value.authority.clone(), value.created_at)?;
        if expected.0 != value {
            return Err(SceneCompositionError::RecordMismatch);
        }
        Ok(expected)
    }
}

impl Serialize for SceneComposition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

fn digest_json(value: &impl Serialize) -> Result<Sha256Digest, SceneCompositionError> {
    serde_json::to_vec(value)
        .map(|bytes| Sha256Digest::from_bytes(&bytes))
        .map_err(|_| SceneCompositionError::Serialization)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
struct SceneCompositionWire {
    schema_version: u64,
    composition_id: SceneCompositionId,
    composition_uri: super::super::CompositionUri,
    revision: u64,
    base_layer: LayerId,
    map_releases: BTreeSet<MapReleaseUri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_frame: Option<LocalFrameBinding>,
    style_id: SceneStyleId,
    governed_inputs: Vec<GovernedSceneInput>,
    overlays: Vec<SceneOverlay>,
    algorithm_revision: String,
    request_digest_sha256: Sha256Digest,
    composition_digest_sha256: Sha256Digest,
    authority: SceneCompositionAuthority,
    created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests;
