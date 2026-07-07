use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    AxisConvention, CoordinateOperationId, CoordinateOperationProvenance, FrameDefinition, FrameId,
    FrameKind, UsageRecord, UsageReport,
};

use crate::uris;

pub struct CoordinatesState {
    frames: RwLock<HashMap<FrameId, FrameDefinition>>,
    operations: RwLock<HashMap<CoordinateOperationId, CoordinateOperationProvenance>>,
    usage: RwLock<HashMap<String, Vec<UsageRecord>>>,
}

impl CoordinatesState {
    pub fn new() -> Self {
        let mut frames = HashMap::new();
        for frame in builtin_frames() {
            frames.insert(frame.frame_id.clone(), frame);
        }
        Self {
            frames: RwLock::new(frames),
            operations: RwLock::new(HashMap::new()),
            usage: RwLock::new(HashMap::new()),
        }
    }

    pub async fn list_frames(&self) -> Vec<FrameDefinition> {
        let mut frames: Vec<_> = self.frames.read().await.values().cloned().collect();
        frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
        frames
    }

    pub async fn get_frame(&self, frame_id: &FrameId) -> Option<FrameDefinition> {
        self.frames.read().await.get(frame_id).cloned()
    }

    pub async fn insert_frame(&self, frame: FrameDefinition) {
        self.frames
            .write()
            .await
            .insert(frame.frame_id.clone(), frame);
    }

    pub async fn require_frame(&self, frame_id: &FrameId) -> Result<FrameDefinition> {
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

fn builtin_frames() -> Vec<FrameDefinition> {
    vec![
        FrameDefinition {
            frame_id: FrameId::new("WGS84").expect("valid builtin frame id"),
            kind: FrameKind::Wgs84,
            axis_convention: AxisConvention::LatitudeLongitudeHeight,
            parent: None,
            origin: None,
            crs: Some(veoveo_mcp_contract::CrsId::new("EPSG:4326").expect("valid CRS")),
            datum: Some(veoveo_mcp_contract::DatumId::new("WGS84").expect("valid datum")),
            ellipsoid: Some(
                veoveo_mcp_contract::EllipsoidId::new("WGS84").expect("valid ellipsoid"),
            ),
            epoch: None,
            description: Some("WGS84 geodetic latitude, longitude, ellipsoidal height.".into()),
        },
        FrameDefinition {
            frame_id: FrameId::new("ECEF").expect("valid builtin frame id"),
            kind: FrameKind::Ecef,
            axis_convention: AxisConvention::XyzMeters,
            parent: Some(FrameId::new("WGS84").expect("valid builtin frame id")),
            origin: None,
            crs: Some(veoveo_mcp_contract::CrsId::new("EPSG:4978").expect("valid CRS")),
            datum: Some(veoveo_mcp_contract::DatumId::new("WGS84").expect("valid datum")),
            ellipsoid: Some(
                veoveo_mcp_contract::EllipsoidId::new("WGS84").expect("valid ellipsoid"),
            ),
            epoch: None,
            description: Some("Earth-centered, Earth-fixed WGS84 cartesian frame.".into()),
        },
    ]
}
