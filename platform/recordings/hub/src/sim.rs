//! Typed, deterministic sensor generators.
//!
//! Every generator is a pure function of `(seed, tick)`, so a stack run is
//! exactly reproducible: the smoke asserts emitted counts and final values
//! against [`StackReport`], not against approximations. The same binary is the
//! smoke's fake stack and the bench harness's load.

use serde::{Deserialize, Serialize};

/// A validated sensor id (path-safe, non-empty).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct SensorId(String);

impl SensorId {
    pub fn new(raw: impl Into<String>) -> anyhow::Result<Self> {
        let raw = raw.into();
        anyhow::ensure!(!raw.is_empty(), "sensor id must not be empty");
        anyhow::ensure!(
            raw.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "sensor id `{raw}` must be [A-Za-z0-9_-]"
        );
        Ok(Self(raw))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SensorId {
    type Error = anyhow::Error;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "pattern", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackPattern {
    /// Circular orbit around the origin.
    Orbit { radius_m: f64, period_s: f64 },
    /// Straight line at a constant heading and speed.
    Line { heading_deg: f64, speed_mps: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "wave", rename_all = "snake_case", deny_unknown_fields)]
pub enum Wave {
    Sine { amplitude: f64, period_s: f64 },
    Step { low: f64, high: f64, period_s: f64 },
    RandomWalk { step: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SensorKind {
    Imu {
        rate_hz: f64,
        accel_bias: [f64; 3],
        gyro_noise: f64,
    },
    Gnss {
        rate_hz: f64,
        origin: LatLon,
        pattern: TrackPattern,
    },
    Camera {
        fps: f64,
        frame_bytes: usize,
    },
    Scalar {
        rate_hz: f64,
        name: String,
        wave: Wave,
    },
}

impl SensorKind {
    pub fn rate_hz(&self) -> f64 {
        match self {
            Self::Imu { rate_hz, .. }
            | Self::Gnss { rate_hz, .. }
            | Self::Scalar { rate_hz, .. } => *rate_hz,
            Self::Camera { fps, .. } => *fps,
        }
    }

    pub fn entity_path(&self, id: &SensorId) -> String {
        let sub = match self {
            Self::Imu { .. } => "imu",
            Self::Gnss { .. } => "gnss",
            Self::Camera { .. } => "camera",
            Self::Scalar { .. } => "scalar",
        };
        format!("/world/sim/{sub}/{}", id.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "SensorSpecWire")]
pub struct SensorSpec {
    pub id: SensorId,
    /// The recording (session) this sensor writes into at the hub.
    pub recording: String,
    /// The Rerun application id, used by the spooler for dataset routing.
    pub application_id: String,
    #[serde(flatten)]
    pub kind: SensorKind,
    pub seed: u64,
    /// Emit for this many seconds; `None` runs until stopped.
    #[serde(default)]
    pub duration_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorStack {
    pub sensors: Vec<SensorSpec>,
}

// The JSON manifest puts sensor-specific fields beside the common fields.
// Decode that whole object as one closed variant; Serde cannot safely combine
// flatten with deny_unknown_fields during deserialization.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SensorSpecWire {
    Imu {
        id: SensorId,
        recording: String,
        application_id: String,
        seed: u64,
        #[serde(default)]
        duration_s: Option<f64>,
        rate_hz: f64,
        accel_bias: [f64; 3],
        gyro_noise: f64,
    },
    Gnss {
        id: SensorId,
        recording: String,
        application_id: String,
        seed: u64,
        #[serde(default)]
        duration_s: Option<f64>,
        rate_hz: f64,
        origin: LatLon,
        pattern: TrackPattern,
    },
    Camera {
        id: SensorId,
        recording: String,
        application_id: String,
        seed: u64,
        #[serde(default)]
        duration_s: Option<f64>,
        fps: f64,
        frame_bytes: usize,
    },
    Scalar {
        id: SensorId,
        recording: String,
        application_id: String,
        seed: u64,
        #[serde(default)]
        duration_s: Option<f64>,
        rate_hz: f64,
        name: String,
        wave: Wave,
    },
}

impl From<SensorSpecWire> for SensorSpec {
    fn from(wire: SensorSpecWire) -> Self {
        match wire {
            SensorSpecWire::Imu {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                rate_hz,
                accel_bias,
                gyro_noise,
            } => Self {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                kind: SensorKind::Imu {
                    rate_hz,
                    accel_bias,
                    gyro_noise,
                },
            },
            SensorSpecWire::Gnss {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                rate_hz,
                origin,
                pattern,
            } => Self {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                kind: SensorKind::Gnss {
                    rate_hz,
                    origin,
                    pattern,
                },
            },
            SensorSpecWire::Camera {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                fps,
                frame_bytes,
            } => Self {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                kind: SensorKind::Camera { fps, frame_bytes },
            },
            SensorSpecWire::Scalar {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                rate_hz,
                name,
                wave,
            } => Self {
                id,
                recording,
                application_id,
                seed,
                duration_s,
                kind: SensorKind::Scalar {
                    rate_hz,
                    name,
                    wave,
                },
            },
        }
    }
}

/// A deterministic sample for one tick: the timeline value and the numeric
/// payload the generator emits (used both to log and to compute the report).
#[derive(Debug, Clone)]
pub enum Sample {
    /// Numeric channels (IMU axes, scalar value).
    Scalars(Vec<f64>),
    /// A geodetic fix.
    Geo(LatLon),
    /// A synthetic camera frame: (frame_index, byte_len).
    Frame { index: u64, bytes: usize },
}

/// Deterministic per-sensor generator.
pub struct Generator {
    spec: SensorSpec,
    ticks: u64,
    last: Option<Sample>,
    // RandomWalk accumulator (deterministic).
    walk: f64,
}

impl Generator {
    pub fn new(spec: SensorSpec) -> Self {
        Self {
            spec,
            ticks: 0,
            last: None,
            walk: 0.0,
        }
    }

    pub fn spec(&self) -> &SensorSpec {
        &self.spec
    }

    /// Total ticks over the sensor's duration (0 if unbounded).
    pub fn planned_ticks(&self) -> u64 {
        match self.spec.duration_s {
            Some(dur) => (dur * self.spec.kind.rate_hz()).floor() as u64,
            None => 0,
        }
    }

    pub fn emitted(&self) -> u64 {
        self.ticks
    }

    pub fn last_sample(&self) -> Option<&Sample> {
        self.last.as_ref()
    }

    /// The wall time (seconds) of tick `n`.
    pub fn tick_time_s(&self, n: u64) -> f64 {
        n as f64 / self.spec.kind.rate_hz()
    }

    /// Produce the next sample, advancing the tick counter.
    pub fn next_sample(&mut self) -> Sample {
        let n = self.ticks;
        let t = self.tick_time_s(n);
        let sample = match &self.spec.kind {
            SensorKind::Imu {
                accel_bias,
                gyro_noise,
                ..
            } => {
                // Deterministic pseudo-motion: biased sinusoids + seeded noise.
                let ax = accel_bias[0] + (t * 1.7).sin();
                let ay = accel_bias[1] + (t * 2.3).cos();
                let az = accel_bias[2] + 9.81;
                let noise = seeded_unit(self.spec.seed, n) * gyro_noise;
                let gx = (t * 0.9).sin() + noise;
                let gy = (t * 1.1).cos() + noise;
                let gz = noise;
                Sample::Scalars(vec![ax, ay, az, gx, gy, gz])
            }
            SensorKind::Gnss {
                origin, pattern, ..
            } => Sample::Geo(geo_at(*origin, pattern, t)),
            SensorKind::Camera { frame_bytes, .. } => Sample::Frame {
                index: n,
                bytes: *frame_bytes,
            },
            SensorKind::Scalar { wave, .. } => {
                let value = match wave {
                    Wave::Sine {
                        amplitude,
                        period_s,
                    } => amplitude * (std::f64::consts::TAU * t / period_s).sin(),
                    Wave::Step {
                        low,
                        high,
                        period_s,
                    } => {
                        if (t / period_s).floor() as i64 % 2 == 0 {
                            *low
                        } else {
                            *high
                        }
                    }
                    Wave::RandomWalk { step } => {
                        self.walk += (seeded_unit(self.spec.seed, n) - 0.5) * 2.0 * step;
                        self.walk
                    }
                };
                Sample::Scalars(vec![value])
            }
        };
        self.ticks += 1;
        self.last = Some(sample.clone());
        sample
    }
}

/// A geodetic position along a track pattern at time `t`.
pub fn geo_at(origin: LatLon, pattern: &TrackPattern, t: f64) -> LatLon {
    // Local flat-earth approximation, adequate for a deterministic showcase.
    const M_PER_DEG_LAT: f64 = 111_320.0;
    let m_per_deg_lon = M_PER_DEG_LAT * origin.lat.to_radians().cos();
    let (dx, dy) = match pattern {
        TrackPattern::Orbit { radius_m, period_s } => {
            let theta = std::f64::consts::TAU * t / period_s;
            (radius_m * theta.cos(), radius_m * theta.sin())
        }
        TrackPattern::Line {
            heading_deg,
            speed_mps,
        } => {
            let h = heading_deg.to_radians();
            let dist = speed_mps * t;
            (dist * h.sin(), dist * h.cos())
        }
    };
    LatLon {
        lat: origin.lat + dy / M_PER_DEG_LAT,
        lon: origin.lon + dx / m_per_deg_lon,
    }
}

/// A stable pseudo-random unit value in [0, 1) from (seed, tick) — no global RNG,
/// so results are identical across runs and machines.
fn seeded_unit(seed: u64, tick: u64) -> f64 {
    // SplitMix64-style mix.
    let mut z = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_mul(tick.wrapping_add(1));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Per-sensor ground truth after a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReport {
    pub id: SensorId,
    pub recording: String,
    pub application_id: String,
    pub entity_path: String,
    pub emitted: u64,
    /// Final numeric payload (empty for camera).
    pub final_scalars: Vec<f64>,
    /// Final geodetic fix, if any.
    pub final_geo: Option<LatLon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackReport {
    pub sensors: Vec<SensorReport>,
    pub total_emitted: u64,
}

impl StackReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("stack report serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor_manifests() -> Vec<serde_json::Value> {
        let kinds = [
            serde_json::json!({"kind": "imu", "rate_hz": 200.0,
                "accel_bias": [0.05, -0.02, 0.01], "gyro_noise": 0.01}),
            serde_json::json!({"kind": "camera", "fps": 30.0, "frame_bytes": 1024}),
            serde_json::json!({"kind": "gnss", "rate_hz": 10.0,
                "origin": {"lat": 47.0, "lon": 8.0},
                "pattern": {"pattern": "orbit", "radius_m": 120.0, "period_s": 40.0}}),
            serde_json::json!({"kind": "gnss", "rate_hz": 10.0,
                "origin": {"lat": 47.0, "lon": 8.0},
                "pattern": {"pattern": "line", "heading_deg": 90.0, "speed_mps": 5.0}}),
            serde_json::json!({"kind": "scalar", "rate_hz": 20.0, "name": "speed_mps",
                "wave": {"wave": "sine", "amplitude": 12.0, "period_s": 15.0}}),
            serde_json::json!({"kind": "scalar", "rate_hz": 20.0, "name": "speed_mps",
                "wave": {"wave": "step", "low": 0.0, "high": 12.0, "period_s": 15.0}}),
            serde_json::json!({"kind": "scalar", "rate_hz": 20.0, "name": "speed_mps",
                "wave": {"wave": "random_walk", "step": 0.5}}),
        ];
        kinds
            .into_iter()
            .map(|mut sensor| {
                let fields = sensor.as_object_mut().unwrap();
                fields.insert("id".into(), "sensor-a".into());
                fields.insert("recording".into(), "sim-sensor-a".into());
                fields.insert("application_id".into(), "veoveo-sim-sensor-a".into());
                fields.insert("seed".into(), 42.into());
                fields.insert("duration_s".into(), 2.0.into());
                sensor
            })
            .collect()
    }

    #[test]
    fn manifest_variants_preserve_wire_and_duration_default() {
        for sensor in sensor_manifests() {
            let manifest = serde_json::json!({"sensors": [sensor]});
            let stack: SensorStack = serde_json::from_value(manifest.clone()).unwrap();
            assert_eq!(serde_json::to_value(&stack).unwrap(), manifest);

            let mut unbounded = manifest;
            unbounded["sensors"][0]
                .as_object_mut()
                .unwrap()
                .remove("duration_s");
            let stack: SensorStack = serde_json::from_value(unbounded.clone()).unwrap();
            assert_eq!(stack.sensors[0].duration_s, None);
            unbounded["sensors"][0]["duration_s"] = serde_json::Value::Null;
            assert_eq!(serde_json::to_value(&stack).unwrap(), unbounded);
            let stack: SensorStack = serde_json::from_value(unbounded).unwrap();
            assert_eq!(stack.sensors[0].duration_s, None);
        }
    }

    #[test]
    fn manifest_rejects_unknown_fields_before_generator_admission() {
        for sensor in sensor_manifests() {
            let manifest = serde_json::json!({"sensors": [sensor]});
            for path in [
                "",
                "/sensors/0",
                "/sensors/0/origin",
                "/sensors/0/pattern",
                "/sensors/0/wave",
            ] {
                let mut invalid = manifest.clone();
                let Some(object) = invalid.pointer_mut(path).and_then(|v| v.as_object_mut()) else {
                    continue;
                };
                object.insert("unexpected".into(), true.into());
                let error = serde_json::from_value::<SensorStack>(invalid).unwrap_err();
                assert!(error.to_string().contains("unexpected"), "{path}: {error}");
            }

            // A field declared by another sensor kind is also unknown here.
            let mut invalid = manifest;
            let foreign_field = if invalid["sensors"][0]["kind"] == "camera" {
                "rate_hz"
            } else {
                "frame_bytes"
            };
            invalid["sensors"][0][foreign_field] = 1.into();
            let error = serde_json::from_value::<SensorStack>(invalid).unwrap_err();
            assert!(error.to_string().contains(foreign_field), "{error}");
        }
    }

    #[test]
    fn sensor_ids_apply_constructor_admission_when_decoded() {
        for raw in ["sensor-a", "sensor_A1", "0"] {
            let id: SensorId = serde_json::from_value(raw.into()).unwrap();
            assert_eq!(id, SensorId::new(raw).unwrap());
            assert_eq!(serde_json::to_value(&id).unwrap(), raw);
            let mut manifest = sensor_manifests().remove(0);
            manifest["id"] = raw.into();
            let stack: SensorStack =
                serde_json::from_value(serde_json::json!({"sensors": [manifest]})).unwrap();
            assert_eq!(stack.sensors[0].id, id);
        }
        for raw in ["", "sensor/a", "sensor a", "sensor.a", "sensor-é"] {
            let diagnostic = SensorId::new(raw).unwrap_err().to_string();
            let error = serde_json::from_value::<SensorId>(raw.into()).unwrap_err();
            assert!(error.to_string().contains(&diagnostic), "{error}");
            let mut manifest = sensor_manifests().remove(0);
            manifest["id"] = raw.into();
            let error =
                serde_json::from_value::<SensorStack>(serde_json::json!({"sensors": [manifest]}))
                    .unwrap_err();
            assert!(error.to_string().contains(&diagnostic), "{error}");
        }
    }

    #[test]
    fn manifest_requires_common_and_selected_sensor_fields() {
        for sensor in sensor_manifests() {
            // Every supplied field except duration_s is required. Check nested
            // payload fields as well as the common and sensor-specific fields.
            for path in ["", "/origin", "/pattern", "/wave"] {
                let Some(fields) = sensor.pointer(path).and_then(|v| v.as_object()) else {
                    continue;
                };
                for field in fields.keys().filter(|field| *field != "duration_s") {
                    let mut invalid = sensor.clone();
                    invalid
                        .pointer_mut(path)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove(field);
                    let error = serde_json::from_value::<SensorStack>(
                        serde_json::json!({"sensors": [invalid]}),
                    )
                    .unwrap_err();
                    assert!(error.to_string().contains(field), "{path}/{field}: {error}");
                }
            }
        }
        let error = serde_json::from_str::<SensorStack>("{}").unwrap_err();
        assert!(error.to_string().contains("sensors"), "{error}");
    }

    fn gnss(pattern: TrackPattern) -> Generator {
        Generator::new(SensorSpec {
            id: SensorId::new("g1").unwrap(),
            recording: "rec-g1".into(),
            application_id: "veoveo-sim-g1".into(),
            kind: SensorKind::Gnss {
                rate_hz: 10.0,
                origin: LatLon {
                    lat: 47.0,
                    lon: 8.0,
                },
                pattern,
            },
            seed: 42,
            duration_s: Some(1.0),
        })
    }

    #[test]
    fn generators_are_deterministic() {
        let mut a = gnss(TrackPattern::Orbit {
            radius_m: 100.0,
            period_s: 60.0,
        });
        let mut b = gnss(TrackPattern::Orbit {
            radius_m: 100.0,
            period_s: 60.0,
        });
        for _ in 0..10 {
            let (sa, sb) = (a.next_sample(), b.next_sample());
            match (sa, sb) {
                (Sample::Geo(x), Sample::Geo(y)) => {
                    assert_eq!(x.lat.to_bits(), y.lat.to_bits());
                    assert_eq!(x.lon.to_bits(), y.lon.to_bits());
                }
                _ => panic!("expected geo samples"),
            }
        }
    }

    #[test]
    fn planned_ticks_matches_rate_times_duration() {
        let g = gnss(TrackPattern::Line {
            heading_deg: 90.0,
            speed_mps: 5.0,
        });
        assert_eq!(g.planned_ticks(), 10); // 10 Hz * 1.0 s
    }

    #[test]
    fn line_track_moves_east_on_heading_90() {
        let mut g = gnss(TrackPattern::Line {
            heading_deg: 90.0,
            speed_mps: 10.0,
        });
        let origin = match &g.spec().kind {
            SensorKind::Gnss { origin, .. } => *origin,
            _ => unreachable!(),
        };
        for _ in 0..10 {
            g.next_sample();
        }
        if let Some(Sample::Geo(last)) = g.last_sample() {
            assert!(last.lon > origin.lon, "heading 90 moves east");
            assert!((last.lat - origin.lat).abs() < 1e-6, "heading 90 holds lat");
        } else {
            panic!("expected geo");
        }
    }

    #[test]
    fn imu_emits_six_channels() {
        let mut g = Generator::new(SensorSpec {
            id: SensorId::new("imu1").unwrap(),
            recording: "rec".into(),
            application_id: "veoveo-sim-imu1".into(),
            kind: SensorKind::Imu {
                rate_hz: 100.0,
                accel_bias: [0.1, 0.2, 0.3],
                gyro_noise: 0.01,
            },
            seed: 7,
            duration_s: Some(0.1),
        });
        match g.next_sample() {
            Sample::Scalars(v) => assert_eq!(v.len(), 6),
            _ => panic!("imu emits scalars"),
        }
    }

    #[test]
    fn stack_json_roundtrips() {
        let stack = SensorStack {
            sensors: vec![
                gnss(TrackPattern::Orbit {
                    radius_m: 50.0,
                    period_s: 30.0,
                })
                .spec()
                .clone(),
            ],
        };
        let json = serde_json::to_string(&stack).unwrap();
        let back: SensorStack = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sensors.len(), 1);
    }
}
