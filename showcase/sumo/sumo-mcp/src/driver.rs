use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use traci_rs::TraciClient;

use crate::contract::{Scenario, Signal, TrafficState, Vehicle};

pub trait SimDriver: Send {
    fn describe(&mut self) -> Result<Scenario>;
    fn state(&mut self) -> Result<TrafficState>;
    fn network_geometry(&mut self) -> Result<Vec<Vec<[f64; 2]>>>;
    fn step(&mut self, count: u32) -> Result<()>;
    fn set_signal_phase(&mut self, signal_id: &str, phase: i32) -> Result<()>;
    fn reroute_vehicle(&mut self, vehicle_id: &str, target_edge_id: &str) -> Result<()>;
    fn set_edge_speed(&mut self, edge_id: &str, speed_mps: f64) -> Result<()>;
    fn close_lane(&mut self, lane_id: &str) -> Result<()>;
    fn open_lane(&mut self, lane_id: &str) -> Result<()>;
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FakeSimDriver {
    name: String,
    vehicle_count: usize,
    seed: u64,
    step: u64,
    congestion_window: (u64, u64),
    signal_phases: BTreeMap<String, i32>,
    rerouted: BTreeMap<String, String>,
    edge_speeds: BTreeMap<String, f64>,
    closed_lanes: BTreeSet<String>,
}

impl Default for FakeSimDriver {
    fn default() -> Self {
        Self::new(12, 1, (40, 60))
    }
}

impl FakeSimDriver {
    pub fn new(vehicle_count: usize, seed: u64, congestion_window: (u64, u64)) -> Self {
        Self {
            name: "grid-fake".to_owned(),
            vehicle_count,
            seed,
            step: 0,
            congestion_window,
            signal_phases: BTreeMap::new(),
            rerouted: BTreeMap::new(),
            edge_speeds: BTreeMap::new(),
            closed_lanes: BTreeSet::new(),
        }
    }

    fn edges() -> Vec<String> {
        (0..4).map(|index| format!("edge_{index}")).collect()
    }

    fn signals() -> Vec<String> {
        vec!["tl_center".to_owned()]
    }

    fn simulation_time(&self) -> f64 {
        self.step as f64
    }

    fn in_congestion(&self) -> bool {
        self.step >= self.congestion_window.0 && self.step < self.congestion_window.1
    }

    fn speed(&self, index: usize) -> f64 {
        let base = 12.0 + 4.0 * (self.simulation_time() * 0.3 + index as f64).sin();
        let congested = if self.in_congestion() {
            base * 0.15
        } else {
            base
        };
        let jitter = seeded_unit(self.seed, self.step * 97 + index as u64) - 0.5;
        (congested + jitter).max(0.0)
    }

    fn vehicles(&self) -> Vec<Vehicle> {
        const ORIGIN_LAT: f64 = 47.3769;
        const ORIGIN_LON: f64 = 8.5417;
        const METERS_PER_DEGREE_LATITUDE: f64 = 111_320.0;
        let meters_per_degree_longitude =
            METERS_PER_DEGREE_LATITUDE * ORIGIN_LAT.to_radians().cos();
        (0..self.vehicle_count)
            .map(|index| {
                let angle =
                    TAU * index as f64 / self.vehicle_count as f64 + self.simulation_time() * 0.05;
                let x = 200.0 * angle.cos();
                let y = 200.0 * angle.sin();
                let heading = (-angle.sin())
                    .atan2(angle.cos())
                    .to_degrees()
                    .rem_euclid(360.0);
                let bus = index % 5 == 0;
                let id = format!("veh_{index}");
                Vehicle {
                    id: id.clone(),
                    latitude: ORIGIN_LAT + y / METERS_PER_DEGREE_LATITUDE,
                    longitude: ORIGIN_LON + x / meters_per_degree_longitude,
                    speed_mps: self.speed(index),
                    edge_id: self
                        .rerouted
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| format!("edge_{}", index % 4)),
                    heading_degrees: heading,
                    x_m: x,
                    y_m: y,
                    length_m: if bus { 12.0 } else { 4.5 },
                    width_m: if bus { 2.5 } else { 1.8 },
                    height_m: if bus { 3.2 } else { 1.5 },
                    vehicle_class: if bus { "bus" } else { "passenger" }.to_owned(),
                }
            })
            .collect()
    }

    fn validate_edge(edge_id: &str) -> Result<()> {
        ensure!(
            Self::edges().iter().any(|edge| edge == edge_id),
            "unknown edge `{edge_id}`"
        );
        Ok(())
    }

    fn validate_lane(lane_id: &str) -> Result<()> {
        let edge = lane_id.rsplit_once('_').map_or(lane_id, |(edge, _)| edge);
        Self::validate_edge(edge).with_context(|| format!("unknown lane `{lane_id}`"))
    }
}

impl SimDriver for FakeSimDriver {
    fn describe(&mut self) -> Result<Scenario> {
        Ok(Scenario {
            name: self.name.clone(),
            edge_count: Self::edges().len(),
            signal_count: Self::signals().len(),
            edges: Self::edges(),
            signals: Self::signals(),
            origin_latitude: 47.3769,
            origin_longitude: 8.5417,
        })
    }

    fn state(&mut self) -> Result<TrafficState> {
        let vehicles = self.vehicles();
        let mean_speed = if vehicles.is_empty() {
            0.0
        } else {
            vehicles
                .iter()
                .map(|vehicle| vehicle.speed_mps)
                .sum::<f64>()
                / vehicles.len() as f64
        };
        Ok(TrafficState {
            simulation_time_s: self.simulation_time(),
            vehicle_count: vehicles.len(),
            mean_speed_mps: mean_speed,
            vehicles,
            signals: Self::signals()
                .into_iter()
                .map(|id| Signal {
                    phase: self.signal_phases.get(&id).copied().unwrap_or_default(),
                    id,
                })
                .collect(),
        })
    }

    fn network_geometry(&mut self) -> Result<Vec<Vec<[f64; 2]>>> {
        Ok(vec![
            vec![[-200.0, 0.0], [200.0, 0.0]],
            vec![[0.0, -200.0], [0.0, 200.0]],
        ])
    }

    fn step(&mut self, count: u32) -> Result<()> {
        self.step = self.step.saturating_add(u64::from(count));
        Ok(())
    }

    fn set_signal_phase(&mut self, signal_id: &str, phase: i32) -> Result<()> {
        ensure!(phase >= 0, "phase must be non-negative");
        ensure!(
            Self::signals().iter().any(|signal| signal == signal_id),
            "unknown signal `{signal_id}`"
        );
        self.signal_phases.insert(signal_id.to_owned(), phase);
        Ok(())
    }

    fn reroute_vehicle(&mut self, vehicle_id: &str, target_edge_id: &str) -> Result<()> {
        Self::validate_edge(target_edge_id)?;
        ensure!(
            vehicle_id
                .strip_prefix("veh_")
                .and_then(|index| index.parse::<usize>().ok())
                .is_some_and(|index| index < self.vehicle_count),
            "unknown vehicle `{vehicle_id}`"
        );
        self.rerouted
            .insert(vehicle_id.to_owned(), target_edge_id.to_owned());
        Ok(())
    }

    fn set_edge_speed(&mut self, edge_id: &str, speed_mps: f64) -> Result<()> {
        Self::validate_edge(edge_id)?;
        ensure!(
            (0.0..=60.0).contains(&speed_mps),
            "speed must be in 0..=60 m/s"
        );
        self.edge_speeds.insert(edge_id.to_owned(), speed_mps);
        Ok(())
    }

    fn close_lane(&mut self, lane_id: &str) -> Result<()> {
        Self::validate_lane(lane_id)?;
        self.closed_lanes.insert(lane_id.to_owned());
        Ok(())
    }

    fn open_lane(&mut self, lane_id: &str) -> Result<()> {
        Self::validate_lane(lane_id)?;
        self.closed_lanes.remove(lane_id);
        Ok(())
    }
}

pub struct TraciSimDriver {
    client: TraciClient,
    name: String,
    max_vehicles: usize,
}

impl TraciSimDriver {
    pub fn connect(
        host: &str,
        port: u16,
        name: impl Into<String>,
        max_vehicles: usize,
        retries: u32,
    ) -> Result<Self> {
        ensure!(max_vehicles > 0, "max vehicles must be greater than zero");
        let mut last_error = None;
        for attempt in 0..=retries {
            match TraciClient::connect(host, port) {
                Ok(mut client) => {
                    client.set_order(1).context("setting TraCI client order")?;
                    return Ok(Self {
                        client,
                        name: name.into(),
                        max_vehicles,
                    });
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt < retries {
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }
        bail!(
            "connecting to TraCI {host}:{port} failed after {} attempts: {}",
            retries + 1,
            last_error.expect("at least one TraCI attempt")
        )
    }

    fn edges(&mut self) -> Result<Vec<String>> {
        let scope = std::mem::take(&mut self.client.edge);
        let result = scope.get_id_list(&mut self.client);
        self.client.edge = scope;
        Ok(result?
            .into_iter()
            .filter(|edge| !edge.starts_with(':'))
            .collect())
    }

    fn signal_ids(&mut self) -> Result<Vec<String>> {
        let scope = std::mem::take(&mut self.client.traffic_lights);
        let result = scope.get_id_list(&mut self.client);
        self.client.traffic_lights = scope;
        Ok(result?)
    }

    fn simulation_time(&mut self) -> Result<f64> {
        let scope = std::mem::take(&mut self.client.simulation);
        let result = scope.get_time(&mut self.client);
        self.client.simulation = scope;
        Ok(result?)
    }

    fn convert_to_geo(&mut self, x: f64, y: f64) -> Result<(f64, f64)> {
        let scope = std::mem::take(&mut self.client.simulation);
        let result = scope.convert_geo(&mut self.client, x, y, false);
        self.client.simulation = scope;
        let position = result.context("converting SUMO position through the network projection")?;
        ensure!(
            position.x.abs() <= 180.0 && position.y.abs() <= 90.0,
            "SUMO projection returned invalid longitude/latitude ({}, {})",
            position.x,
            position.y
        );
        Ok((position.y, position.x))
    }

    fn vehicle_ids(&mut self) -> Result<Vec<String>> {
        let scope = std::mem::take(&mut self.client.vehicle);
        let result = scope.get_id_list(&mut self.client);
        self.client.vehicle = scope;
        Ok(result?)
    }

    fn vehicle(&mut self, id: &str) -> Result<Vehicle> {
        let scope = std::mem::take(&mut self.client.vehicle);
        let position = scope.get_position(&mut self.client, id);
        let speed = scope.get_speed(&mut self.client, id);
        let edge = scope.get_road_id(&mut self.client, id);
        let heading = scope.get_angle(&mut self.client, id);
        let vehicle_class = scope.get_type_id(&mut self.client, id);
        self.client.vehicle = scope;
        let position = position?;
        let (latitude, longitude) = self.convert_to_geo(position.x, position.y)?;
        Ok(Vehicle {
            id: id.to_owned(),
            latitude,
            longitude,
            speed_mps: speed?,
            edge_id: edge?,
            heading_degrees: heading?,
            x_m: position.x,
            y_m: position.y,
            length_m: 4.5,
            width_m: 1.8,
            height_m: 1.5,
            vehicle_class: vehicle_class?,
        })
    }

    fn signals(&mut self) -> Result<Vec<Signal>> {
        let ids = self.signal_ids()?;
        let scope = std::mem::take(&mut self.client.traffic_lights);
        let result = ids
            .into_iter()
            .map(|id| {
                Ok(Signal {
                    phase: scope.get_phase(&mut self.client, &id)?,
                    id,
                })
            })
            .collect();
        self.client.traffic_lights = scope;
        result
    }
}

impl SimDriver for TraciSimDriver {
    fn describe(&mut self) -> Result<Scenario> {
        let edges = self.edges()?;
        let signals = self.signal_ids()?;
        let (origin_latitude, origin_longitude) = self.convert_to_geo(0.0, 0.0)?;
        Ok(Scenario {
            name: self.name.clone(),
            edge_count: edges.len(),
            signal_count: signals.len(),
            edges: edges.into_iter().take(500).collect(),
            signals: signals.into_iter().take(500).collect(),
            origin_latitude,
            origin_longitude,
        })
    }

    fn state(&mut self) -> Result<TrafficState> {
        let ids = self.vehicle_ids()?;
        let vehicle_count = ids.len();
        let vehicles = ids
            .into_iter()
            .take(self.max_vehicles)
            .map(|id| self.vehicle(&id))
            .collect::<Result<Vec<_>>>()?;
        let mean_speed_mps = if vehicles.is_empty() {
            0.0
        } else {
            vehicles
                .iter()
                .map(|vehicle| vehicle.speed_mps)
                .sum::<f64>()
                / vehicles.len() as f64
        };
        Ok(TrafficState {
            simulation_time_s: self.simulation_time()?,
            vehicle_count,
            mean_speed_mps,
            vehicles,
            signals: self.signals()?,
        })
    }

    fn network_geometry(&mut self) -> Result<Vec<Vec<[f64; 2]>>> {
        let edges = self.edges()?;
        let scope = std::mem::take(&mut self.client.lane);
        let mut strips = Vec::new();
        for edge in edges {
            if let Ok(points) = scope.get_shape(&mut self.client, &format!("{edge}_0"))
                && points.len() >= 2
            {
                strips.push(points.into_iter().map(|point| [point.x, point.y]).collect());
            }
        }
        self.client.lane = scope;
        Ok(strips)
    }

    fn step(&mut self, count: u32) -> Result<()> {
        for _ in 0..count {
            self.client.simulation_step(0.0)?;
        }
        Ok(())
    }

    fn set_signal_phase(&mut self, signal_id: &str, phase: i32) -> Result<()> {
        ensure!(phase >= 0, "phase must be non-negative");
        let scope = std::mem::take(&mut self.client.traffic_lights);
        let result = scope.set_phase(&mut self.client, signal_id, phase);
        self.client.traffic_lights = scope;
        Ok(result?)
    }

    fn reroute_vehicle(&mut self, vehicle_id: &str, target_edge_id: &str) -> Result<()> {
        let scope = std::mem::take(&mut self.client.vehicle);
        let result = scope.change_target(&mut self.client, vehicle_id, target_edge_id);
        self.client.vehicle = scope;
        Ok(result?)
    }

    fn set_edge_speed(&mut self, edge_id: &str, speed_mps: f64) -> Result<()> {
        ensure!(
            (0.0..=60.0).contains(&speed_mps),
            "speed must be in 0..=60 m/s"
        );
        let scope = std::mem::take(&mut self.client.edge);
        let result = scope.set_max_speed(&mut self.client, edge_id, speed_mps);
        self.client.edge = scope;
        Ok(result?)
    }

    fn close_lane(&mut self, lane_id: &str) -> Result<()> {
        let scope = std::mem::take(&mut self.client.lane);
        let result = scope.set_disallowed(&mut self.client, lane_id, &["all".to_owned()]);
        self.client.lane = scope;
        Ok(result?)
    }

    fn open_lane(&mut self, lane_id: &str) -> Result<()> {
        let scope = std::mem::take(&mut self.client.lane);
        let result = scope.set_allowed(&mut self.client, lane_id, &["all".to_owned()]);
        self.client.lane = scope;
        Ok(result?)
    }

    fn close(&mut self) -> Result<()> {
        self.client.close().map_err(Into::into)
    }
}

fn seeded_unit(seed: u64, tick: u64) -> f64 {
    let mut value = seed
        .wrapping_add(0x9e37_79b9_7f4a_7c15)
        .wrapping_mul(tick.wrapping_add(1));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 11) as f64 / (1_u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_world_is_deterministic_and_typed() {
        let mut first = FakeSimDriver::new(8, 7, (5, 10));
        let mut second = FakeSimDriver::new(8, 7, (5, 10));
        assert_eq!(first.state().unwrap(), second.state().unwrap());
        first.step(6).unwrap();
        let jammed = first.state().unwrap();
        assert!(jammed.mean_speed_mps < 5.0);
        assert_eq!(jammed.vehicle_count, 8);
    }

    #[test]
    fn fake_controls_validate_targets() {
        let mut driver = FakeSimDriver::default();
        assert!(driver.set_signal_phase("missing", 0).is_err());
        assert!(driver.set_edge_speed("missing", 5.0).is_err());
        assert!(driver.reroute_vehicle("veh_0", "missing").is_err());
        driver.close_lane("edge_1_0").unwrap();
        driver.open_lane("edge_1_0").unwrap();
    }
}
