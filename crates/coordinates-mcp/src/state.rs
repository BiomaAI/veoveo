use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow, bail};
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationProvenance, FrameId, FrameKind,
};
use veoveo_platform_store::{
    CoordinateFrameDraft, CoordinateOperationDraft, OpenObject, PlatformIdentity, PlatformStore,
    TaskId,
};
use veoveo_rrd::{RrdFrameDefinition, RrdFrameId, RrdViewCoordinates};

#[derive(Clone, Debug)]
pub struct CoordinateScope {
    pub identity: PlatformIdentity,
    pub data_labels: BTreeSet<String>,
}

#[derive(Clone)]
pub struct CoordinatesState {
    store: PlatformStore,
}

impl CoordinatesState {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub async fn list_frames(&self, scope: &CoordinateScope) -> Result<Vec<RrdFrameDefinition>> {
        let mut frames = builtin_frames();
        for record in self
            .store
            .list_coordinate_frames(scope.identity.tenant_id)
            .await?
        {
            if labels_allow(&record.labels, &scope.data_labels) {
                frames.push(frame_from_definition(record.definition)?);
            }
        }
        frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
        Ok(frames)
    }

    pub async fn get_frame(
        &self,
        scope: &CoordinateScope,
        frame_id: &FrameId,
    ) -> Result<Option<RrdFrameDefinition>> {
        if let Some(frame) = builtin_frames()
            .into_iter()
            .find(|frame| frame.frame_id.as_str() == frame_id.as_str())
        {
            return Ok(Some(frame));
        }
        let Some(record) = self
            .store
            .coordinate_frame_by_key(scope.identity.tenant_id, frame_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&record.labels, &scope.data_labels) {
            return Ok(None);
        }
        Ok(Some(frame_from_definition(record.definition)?))
    }

    pub async fn insert_frame(
        &self,
        scope: &CoordinateScope,
        frame: RrdFrameDefinition,
    ) -> Result<()> {
        let key = frame_key(&frame)?;
        if matches!(key.as_str(), "WGS84" | "ECEF") {
            bail!("builtin frame `{key}` is immutable");
        }
        let display_name = frame.description.clone().unwrap_or_else(|| key.to_string());
        self.store
            .create_coordinate_frame(CoordinateFrameDraft {
                identity: scope.identity.clone(),
                frame_key: key.to_string(),
                display_name,
                definition: object_from_value(serde_json::to_value(&frame)?)?,
                proj_pipeline: None,
                classification: "gateway_labels".to_owned(),
                labels: scope.data_labels.iter().cloned().collect(),
            })
            .await?;
        Ok(())
    }

    pub async fn require_frame(
        &self,
        scope: &CoordinateScope,
        frame_id: &FrameId,
    ) -> Result<RrdFrameDefinition> {
        self.get_frame(scope, frame_id)
            .await?
            .ok_or_else(|| anyhow!("unknown frame `{frame_id}`"))
    }

    pub async fn record_operation(
        &self,
        scope: &CoordinateScope,
        task_id: Option<TaskId>,
        provenance: &CoordinateOperationProvenance,
    ) -> Result<()> {
        let kind = serde_json::to_value(&provenance.kind)?
            .as_str()
            .ok_or_else(|| anyhow!("coordinate operation kind did not serialize as a string"))?
            .to_owned();
        self.store
            .upsert_coordinate_operation(CoordinateOperationDraft {
                identity: scope.identity.clone(),
                task_id,
                operation_key: provenance.operation.operation_id.to_string(),
                kind,
                provenance: object_from_value(serde_json::to_value(provenance)?)?,
                classification: "gateway_labels".to_owned(),
                labels: scope.data_labels.iter().cloned().collect(),
                created_at: provenance.operation.created_at,
            })
            .await?;
        Ok(())
    }

    pub async fn get_operation(
        &self,
        scope: &CoordinateScope,
        operation_id: &CoordinateOperationId,
    ) -> Result<Option<CoordinateOperationProvenance>> {
        let Some(record) = self
            .store
            .coordinate_operation(scope.identity.tenant_id, operation_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&record.labels, &scope.data_labels) {
            return Ok(None);
        }
        Ok(Some(serde_json::from_value(value_from_object(
            record.provenance,
        ))?))
    }
}

fn object_from_value(value: serde_json::Value) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(values.into_iter().collect())),
        _ => bail!("coordinate record must serialize as an object"),
    }
}

fn value_from_object(object: OpenObject) -> serde_json::Value {
    serde_json::Value::Object(object.into_map().into_iter().collect())
}

fn frame_from_definition(definition: OpenObject) -> Result<RrdFrameDefinition> {
    serde_json::from_value(value_from_object(definition)).context("decoding coordinate frame")
}

fn labels_allow(required: &[String], caller: &BTreeSet<String>) -> bool {
    required.iter().all(|label| caller.contains(label))
}

fn frame_key(frame: &RrdFrameDefinition) -> Result<FrameId> {
    FrameId::new(frame.frame_id.as_str()).with_context(|| {
        format!(
            "frame `{}` is not a coordinates resource id",
            frame.frame_id
        )
    })
}

fn builtin_frames() -> Vec<RrdFrameDefinition> {
    vec![
        RrdFrameDefinition {
            frame_id: RrdFrameId::new("WGS84").expect("valid builtin frame id"),
            kind: FrameKind::Wgs84,
            view_coordinates: None,
            parent: None,
            origin: None,
            crs: Some(veoveo_mcp_contract::CrsId::new("EPSG:4326").expect("valid CRS")),
            datum: Some(veoveo_mcp_contract::DatumId::new("WGS84").expect("valid datum")),
            ellipsoid: Some(
                veoveo_mcp_contract::EllipsoidId::new("WGS84").expect("valid ellipsoid"),
            ),
            epoch: None,
            description: Some("WGS84 geodetic latitude, longitude, ellipsoidal height.".into()),
            metadata: Default::default(),
        },
        RrdFrameDefinition {
            frame_id: RrdFrameId::new("ECEF").expect("valid builtin frame id"),
            kind: FrameKind::Ecef,
            view_coordinates: Some(RrdViewCoordinates::xyz_meters()),
            parent: Some(RrdFrameId::new("WGS84").expect("valid builtin frame id")),
            origin: None,
            crs: Some(veoveo_mcp_contract::CrsId::new("EPSG:4978").expect("valid CRS")),
            datum: Some(veoveo_mcp_contract::DatumId::new("WGS84").expect("valid datum")),
            ellipsoid: Some(
                veoveo_mcp_contract::EllipsoidId::new("WGS84").expect("valid ellipsoid"),
            ),
            epoch: None,
            description: Some("Earth-centered, Earth-fixed WGS84 cartesian frame.".into()),
            metadata: Default::default(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_well_formed_and_unique() {
        let frames = builtin_frames();
        assert_eq!(frames.len(), 2);
        assert_ne!(frames[0].frame_id, frames[1].frame_id);
        for frame in frames {
            frame_key(&frame).unwrap();
        }
    }

    #[test]
    fn label_visibility_requires_every_record_label() {
        let caller = BTreeSet::from(["cui".to_owned(), "pii".to_owned()]);
        assert!(labels_allow(&["cui".to_owned()], &caller));
        assert!(!labels_allow(&["itar".to_owned()], &caller));
    }
}
