use anyhow::{Context, Result};
use re_sdk::{RecordingStream, RecordingStreamBuilder};
use re_sdk_types::{
    archetypes::{GeoPoints, LineStrips3D, Points3D, Scalars},
    components::LineStrip3D,
};

use crate::contract::TrafficState;

pub struct RecordingPublisher {
    stream: RecordingStream,
    sequence: i64,
}

impl RecordingPublisher {
    pub fn connect(proxy: impl Into<String>, recording_id: impl Into<String>) -> Result<Self> {
        let proxy = proxy.into();
        let stream = RecordingStreamBuilder::new("veoveo-sumo")
            .recording_id(recording_id.into())
            .connect_grpc_opts(proxy.clone())
            .with_context(|| format!("connecting SUMO recording publisher to {proxy}"))?;
        Ok(Self {
            stream,
            sequence: 0,
        })
    }

    pub fn publish_network(&self, geometry: &[Vec<[f64; 2]>]) -> Result<()> {
        let lines = geometry.iter().filter(|line| line.len() >= 2).map(|line| {
            LineStrip3D::from_iter(
                line.iter()
                    .map(|point| [point[0] as f32, point[1] as f32, 0.0]),
            )
        });
        self.stream
            .log_static("/world/sumo/network", &LineStrips3D::new(lines))
            .context("publishing SUMO network geometry")
    }

    pub fn publish(&mut self, state: &TrafficState) -> Result<()> {
        self.stream.set_time_sequence("tick", self.sequence);
        self.sequence = self.sequence.saturating_add(1);
        self.stream.log(
            "/world/sumo/vehicles",
            &GeoPoints::from_lat_lon(
                state
                    .vehicles
                    .iter()
                    .map(|vehicle| (vehicle.latitude, vehicle.longitude)),
            ),
        )?;
        self.stream.log(
            "/world/sumo/vehicles3d",
            &Points3D::new(
                state
                    .vehicles
                    .iter()
                    .map(|vehicle| [vehicle.x_m as f32, vehicle.y_m as f32, 0.0]),
            ),
        )?;
        self.stream.log(
            "/world/sumo/mean_speed_mps",
            &Scalars::single(state.mean_speed_mps),
        )?;
        self.stream.log(
            "/world/sumo/vehicle_count",
            &Scalars::single(state.vehicle_count as f64),
        )?;
        self.stream.log(
            "/world/sumo/simulation_time_s",
            &Scalars::single(state.simulation_time_s),
        )?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.stream.flush_blocking().map_err(Into::into)
    }
}
