use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationProvenance, FrameId, FrameKind, UsageRecord,
    UsageReport,
};
use veoveo_rrd::{RrdFrameDefinition, RrdFrameId, RrdViewCoordinates};

use crate::uris;

pub struct CoordinatesState {
    frames: RwLock<HashMap<FrameId, RrdFrameDefinition>>,
    operations: RwLock<HashMap<CoordinateOperationId, CoordinateOperationProvenance>>,
    usage: RwLock<HashMap<String, Vec<UsageRecord>>>,
}

impl CoordinatesState {
    pub fn new() -> Self {
        let mut frames = HashMap::new();
        for frame in builtin_frames() {
            let key = frame_key(&frame).expect("valid builtin frame key");
            frames.insert(key, frame);
        }
        Self {
            frames: RwLock::new(frames),
            operations: RwLock::new(HashMap::new()),
            usage: RwLock::new(HashMap::new()),
        }
    }

    pub async fn list_frames(&self) -> Vec<RrdFrameDefinition> {
        let mut frames: Vec<_> = self.frames.read().await.values().cloned().collect();
        frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
        frames
    }

    pub async fn get_frame(&self, frame_id: &FrameId) -> Option<RrdFrameDefinition> {
        self.frames.read().await.get(frame_id).cloned()
    }

    pub async fn insert_frame(&self, frame: RrdFrameDefinition) -> Result<()> {
        let key = frame_key(&frame)?;
        self.frames.write().await.insert(key, frame);
        Ok(())
    }

    pub async fn require_frame(&self, frame_id: &FrameId) -> Result<RrdFrameDefinition> {
        self.get_frame(frame_id)
            .await
            .ok_or_else(|| anyhow!("unknown frame `{frame_id}`"))
    }

    pub async fn record_operation(&self, provenance: CoordinateOperationProvenance) {
        self.operations
            .write()
            .await
            .insert(provenance.operation.operation_id.clone(), provenance);
    }

    pub async fn get_operation(
        &self,
        operation_id: &CoordinateOperationId,
    ) -> Option<CoordinateOperationProvenance> {
        self.operations.read().await.get(operation_id).cloned()
    }

    pub async fn record_usage(&self, record: UsageRecord) {
        self.usage
            .write()
            .await
            .entry(record.task_id.clone())
            .or_default()
            .push(record);
    }

    pub async fn usage_report(&self, task_id: &str) -> UsageReport {
        let records = self
            .usage
            .read()
            .await
            .get(task_id)
            .cloned()
            .unwrap_or_default();
        UsageReport::new(task_id, uris::usage_task_uri(task_id)).with_records(records)
    }
}

impl Default for CoordinatesState {
    fn default() -> Self {
        Self::new()
    }
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
