//! sensor-sim: a deterministic sensor stack that pushes typed Rerun streams
//! into the hub. It is both the smoke suite's fake stack and the bench
//! harness's load. Every sensor is a seeded pure function of its tick, so
//! `--report` is exact ground truth the smoke asserts against.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{GeoPoints, Scalars};
use veoveo_recording_hub::sim::{
    Generator, LatLon, Sample, SensorId, SensorKind, SensorReport, SensorSpec, SensorStack,
    StackReport, TrackPattern, Wave,
};

const SIM_TIMELINE: &str = "tick";

#[derive(Parser, Debug)]
#[command(name = "sensor-sim", about = "Deterministic sensor stack → hub")]
struct Args {
    /// Proxy URI to push into (the hub spooler).
    #[arg(long, default_value = "rerun+http://127.0.0.1:9876/proxy")]
    proxy: String,
    /// Stack manifest JSON. When omitted, a built-in 3-sensor stack is used.
    #[arg(long)]
    stack: Option<PathBuf>,
    /// Emit this many seconds when the built-in stack is used.
    #[arg(long, default_value_t = 2.0)]
    duration_s: f64,
    /// Multiply every sensor's rate (bench load).
    #[arg(long, default_value_t = 1.0)]
    burst: f64,
    /// Sleep between ticks to emit at real rate; off = as fast as possible.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    realtime: bool,
    /// Write the exact stack report JSON here (also printed to stdout).
    #[arg(long)]
    report: Option<PathBuf>,
}

/// A built-in, seeded stack used when no manifest is supplied.
fn builtin_stack(duration_s: f64) -> SensorStack {
    SensorStack {
        sensors: vec![
            SensorSpec {
                id: SensorId::new("imu-a").unwrap(),
                recording: "sim-imu-a".into(),
                application_id: "veoveo-sim-imu-a".into(),
                kind: SensorKind::Imu {
                    rate_hz: 200.0,
                    accel_bias: [0.05, -0.02, 0.01],
                    gyro_noise: 0.01,
                },
                seed: 1,
                duration_s: Some(duration_s),
            },
            SensorSpec {
                id: SensorId::new("gnss-a").unwrap(),
                recording: "sim-gnss-a".into(),
                application_id: "veoveo-sim-gnss-a".into(),
                kind: SensorKind::Gnss {
                    rate_hz: 10.0,
                    origin: LatLon {
                        lat: 47.3769,
                        lon: 8.5417,
                    },
                    pattern: TrackPattern::Orbit {
                        radius_m: 120.0,
                        period_s: 40.0,
                    },
                },
                seed: 2,
                duration_s: Some(duration_s),
            },
            SensorSpec {
                id: SensorId::new("speed").unwrap(),
                recording: "sim-speed".into(),
                application_id: "veoveo-sim-speed".into(),
                kind: SensorKind::Scalar {
                    rate_hz: 20.0,
                    name: "speed_mps".into(),
                    wave: Wave::Sine {
                        amplitude: 12.0,
                        period_s: 15.0,
                    },
                },
                seed: 3,
                duration_s: Some(duration_s),
            },
        ],
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let stack = match &args.stack {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("reading stack {}", path.display()))?;
            serde_json::from_str(&raw)
                .with_context(|| format!("parsing stack {}", path.display()))?
        }
        None => builtin_stack(args.duration_s),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(run_stack(stack, &args))?;

    let json = report.to_json();
    if let Some(path) = &args.report {
        std::fs::write(path, &json)
            .with_context(|| format!("writing report {}", path.display()))?;
    }
    println!("{json}");
    Ok(())
}

async fn run_stack(stack: SensorStack, args: &Args) -> Result<StackReport> {
    let mut handles = Vec::new();
    for spec in stack.sensors {
        let proxy = args.proxy.clone();
        let burst = args.burst.max(1e-9);
        let realtime = args.realtime;
        handles.push(tokio::spawn(async move {
            run_sensor(spec, proxy, burst, realtime).await
        }));
    }

    let mut sensors = Vec::new();
    let mut total = 0u64;
    for handle in handles {
        let report = handle.await.context("sensor task panicked")??;
        total += report.emitted;
        sensors.push(report);
    }
    Ok(StackReport {
        sensors,
        total_emitted: total,
    })
}

async fn run_sensor(
    spec: SensorSpec,
    proxy: String,
    burst: f64,
    realtime: bool,
) -> Result<SensorReport> {
    let entity_path = spec.kind.entity_path(&spec.id);
    let stream = RecordingStreamBuilder::new(spec.application_id.clone())
        .recording_id(spec.recording.clone())
        .connect_grpc_opts(proxy.clone())
        .with_context(|| format!("connecting sensor {} to {proxy}", spec.id.as_str()))?;

    let rate = spec.kind.rate_hz() * burst;
    let tick_dur = std::time::Duration::from_secs_f64(1.0 / rate.max(1e-9));
    let mut generator = Generator::new(spec.clone());
    // In realtime mode `burst` speeds emission of the planned ticks; in
    // as-fast-as-possible (bench) mode it also multiplies the tick count, so a
    // burst run actually generates burst× the data over the same duration.
    let total = if realtime {
        generator.planned_ticks()
    } else {
        ((generator.planned_ticks() as f64) * burst).round() as u64
    };
    let mut interval = if realtime {
        Some(tokio::time::interval(tick_dur))
    } else {
        None
    };

    let mut final_scalars = Vec::new();
    let mut final_geo = None;

    for n in 0..total {
        if let Some(interval) = interval.as_mut() {
            interval.tick().await;
        }
        stream.set_time_sequence(SIM_TIMELINE, n as i64);
        match generator.next_sample() {
            Sample::Scalars(values) => {
                stream
                    .log(entity_path.as_str(), &Scalars::new(values.clone()))
                    .with_context(|| format!("logging scalars for {}", entity_path))?;
                final_scalars = values;
            }
            Sample::Geo(fix) => {
                stream
                    .log(
                        entity_path.as_str(),
                        &GeoPoints::from_lat_lon([(fix.lat, fix.lon)]),
                    )
                    .with_context(|| format!("logging geo for {}", entity_path))?;
                final_geo = Some(fix);
            }
            Sample::Frame { index, .. } => {
                // Frame blobs are represented by their index for deterministic
                // counting; real camera frames land here as encoded images.
                stream
                    .log(entity_path.as_str(), &Scalars::new([index as f64]))
                    .with_context(|| format!("logging frame for {}", entity_path))?;
                final_scalars = vec![index as f64];
            }
        }
    }

    let _ = stream.flush_blocking();
    tracing::info!(
        sensor = spec.id.as_str(),
        recording = spec.recording,
        emitted = generator.emitted(),
        "sensor done"
    );

    Ok(SensorReport {
        id: spec.id,
        recording: spec.recording,
        application_id: spec.application_id,
        entity_path,
        emitted: generator.emitted(),
        final_scalars,
        final_geo,
    })
}
