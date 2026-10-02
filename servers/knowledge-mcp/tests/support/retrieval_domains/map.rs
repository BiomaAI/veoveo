use super::*;
use veoveo_map_mcp::contract::*;

pub(super) struct Mapped {
    pub layer: EvaluationMemberId,
    pub feature: EvaluationMemberId,
    pub publication: EvaluationMemberId,
    pub location: EvaluationMemberId,
    pub facility: EvaluationMemberId,
    pub release: EvaluationMemberId,
}
pub(super) fn add(builder: &mut Builder, s: &Scenario) -> Result<Mapped> {
    let layer_id = FeatureLayerId::from_stable_key(s.key.as_bytes());
    let bounds = Wgs84BoundingBox {
        west: -89.3,
        south: 13.5,
        east: -89.1,
        north: 13.7,
    };
    let position = Wgs84Position::new(-89.2, 13.6, None)?;
    let mut add = |address: MapKnowledgeMember,
                   title: &str,
                   details: MapKnowledgeDetails|
     -> Result<EvaluationMemberId> {
        let descriptor = address.collection().descriptor();
        // The digest identifies the fictional scenario seed; no live Map record
        // or sensor observation is asserted by this constructed benchmark member.
        let summary = MapKnowledgeSummary {
            source: address.source_uri(),
            source_sha256: content_digest(&serde_json::to_string(s)?),
            title: title.into(),
            details,
        };
        let text = serde_json::to_string(&summary)?;
        let _: MapKnowledgeSummary = serde_json::from_str(&text)?;
        builder.add(
            descriptor,
            address.to_uri(),
            title,
            text,
            ReadPolicy::SelectedWorkContextMembers {},
        )
    };
    Ok(Mapped {
        layer: add(
            MapKnowledgeMember::Layer {
                layer: layer_id.clone(),
            },
            &s.title,
            MapKnowledgeDetails::Layer {
                description: Some(s.finding.clone()),
                content_class: FeatureContentClass::Boundaries,
                revision: 1,
                archived: false,
                schema_version: 1,
            },
        )?,
        feature: add(
            MapKnowledgeMember::Feature {
                layer: layer_id.clone(),
                feature: MapFeatureId::from_stable_key(s.key.as_bytes()),
            },
            &s.title,
            MapKnowledgeDetails::Feature {
                semantic_type: "inspection-hazard".into(),
                bounds: bounds.clone(),
                feature_revision: 1,
                layer_revision: 1,
                deleted: false,
                properties: vec![
                    MapPropertyExcerpt {
                        name: "observed_condition".into(),
                        value: s.finding.clone(),
                        truncated: false,
                    },
                    MapPropertyExcerpt {
                        name: "required_action".into(),
                        value: s.action.clone(),
                        truncated: false,
                    },
                ],
                omitted_properties: 0,
            },
        )?,
        publication: add(
            MapKnowledgeMember::Publication {
                layer: layer_id,
                publication: LayerPublicationId::from_stable_key(s.key.as_bytes()),
            },
            &format!("{} reviewed feature publication", s.title),
            MapKnowledgeDetails::Publication {
                layer_revision: 1,
                schema_version: 1,
                artifact_count: 2,
            },
        )?,
        location: add(
            MapKnowledgeMember::Location {
                location: LocationId::from_stable_key(s.key.as_bytes()),
            },
            &s.place,
            MapKnowledgeDetails::Location {
                position: position.clone(),
                alternate_names: s.aliases.clone(),
                omitted_names: 0,
            },
        )?,
        facility: add(
            MapKnowledgeMember::Facility {
                facility: FacilityId::from_stable_key(s.key.as_bytes()),
            },
            &s.facility,
            MapKnowledgeDetails::Facility {
                position,
                facility_kind: s.facility_kind,
            },
        )?,
        release: add(
            MapKnowledgeMember::Release {
                dataset: MapDatasetId::from_stable_key(s.key.as_bytes()),
                release: DatasetReleaseId::from_stable_key(s.key.as_bytes()),
            },
            &format!("{} field geography release", s.place),
            MapKnowledgeDetails::Release {
                coverage: bounds,
                state: DatasetReleaseState::Active,
                record_version: 1,
                attribution: format!(
                    "Fictional {} field survey for retrieval qualification",
                    s.place
                ),
            },
        )?,
    })
}
