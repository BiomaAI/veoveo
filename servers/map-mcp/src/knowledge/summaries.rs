use super::*;

pub(super) trait Summarize: Serialize {
    fn details(&self) -> MapKnowledgeDetails;
}

/// Bound escaped title bytes so 100 entries fit the page and tool response budgets.
pub(super) fn link_title(value: &str) -> String {
    let mut title = excerpt(value, 128);
    while serde_json::to_string(&title)
        .expect("text serialization")
        .len()
        > 130
    {
        title.pop();
    }
    title
}

/// Keep a UTF-8 prefix within a byte budget. Limits apply before JSON escaping;
/// even worst-case escaping of the declared fields fits the 64 KiB member bound.
pub(super) fn excerpt(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
impl Summarize for FeatureLayer {
    fn details(&self) -> MapKnowledgeDetails {
        MapKnowledgeDetails::Layer {
            description: self.description.as_deref().map(|s| excerpt(s, 4096)),
            content_class: self.content_class,
            revision: self.revision,
            archived: self.archived_at.is_some(),
            schema_version: self.schema.version,
        }
    }
}
impl Summarize for MapFeature {
    fn details(&self) -> MapKnowledgeDetails {
        let properties = self
            .properties
            .iter()
            .take(32)
            .map(|(name, value)| {
                let text = if let Some(text) = value.as_str() {
                    text.to_owned()
                } else {
                    serde_json::to_string(value).expect("JSON property")
                };
                MapPropertyExcerpt {
                    name: excerpt(name, 128),
                    value: excerpt(&text, 128),
                    truncated: name.len() > 128 || text.len() > 128,
                }
            })
            .collect::<Vec<_>>();
        MapKnowledgeDetails::Feature {
            semantic_type: excerpt(&self.semantic_type, 256),
            bounds: self.geometry.bounding_box(),
            feature_revision: self.feature_revision,
            layer_revision: self.layer_revision,
            deleted: self.deleted,
            omitted_properties: self.properties.len().saturating_sub(properties.len()),
            properties,
        }
    }
}
impl Summarize for LayerPublication {
    fn details(&self) -> MapKnowledgeDetails {
        MapKnowledgeDetails::Publication {
            layer_revision: self.layer_revision,
            schema_version: self.schema_version,
            artifact_count: self.artifact_uris.len(),
        }
    }
}
impl Summarize for MapLocation {
    fn details(&self) -> MapKnowledgeDetails {
        MapKnowledgeDetails::Location {
            position: self.position.clone(),
            alternate_names: self
                .alternate_names
                .iter()
                .take(16)
                .map(|v| excerpt(v, 256))
                .collect(),
            omitted_names: self.alternate_names.len().saturating_sub(16),
        }
    }
}
impl Summarize for Facility {
    fn details(&self) -> MapKnowledgeDetails {
        MapKnowledgeDetails::Facility {
            position: self.position.clone(),
            facility_kind: self.kind,
        }
    }
}
impl Summarize for DatasetRelease {
    fn details(&self) -> MapKnowledgeDetails {
        MapKnowledgeDetails::Release {
            coverage: self.coverage.clone(),
            state: self.state,
            record_version: self.record_version,
            attribution: excerpt(&self.license.attribution, 4096),
        }
    }
}
