//! Admission of the owned View process's shared probe and production GPU logs.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum RendererBackend {
    #[vocabulary(rename = "Vulkan")]
    Vulkan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum RendererDeviceType {
    #[vocabulary(rename = "DiscreteGpu")]
    DiscreteGpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum JpegEncoder {
    #[vocabulary(rename = "nvjpeg_cuda_gpu")]
    NvjpegCudaGpu,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AdmittedAdapter {
    pub name: String,
    pub backend: RendererBackend,
    pub device_type: RendererDeviceType,
    pub jpeg_encoder: JpegEncoder,
    #[serde(serialize_with = "serialize_cuda_uuid")]
    pub cuda_device_uuid: Uuid,
}

fn serialize_cuda_uuid<S: serde::Serializer>(
    uuid: &Uuid,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&uuid.simple().to_string())
}

fn deserialize_cuda_uuid<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Uuid, D::Error> {
    let text = String::deserialize(decoder)?;
    ensure_cuda_uuid(&text).map_err(serde::de::Error::custom)
}

fn ensure_cuda_uuid(text: &str) -> Result<Uuid> {
    ensure!(
        text.len() == 32
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "View CUDA UUID must contain 32 lowercase hexadecimal digits"
    );
    let uuid = Uuid::parse_str(text)?;
    ensure!(!uuid.is_nil(), "View CUDA UUID is zero");
    Ok(uuid)
}

pub(super) fn admit_readiness(status: u16, body: &str) -> Result<()> {
    ensure!(status == 200, "View readiness did not return HTTP 200");
    ensure!(
        body.trim() == "ready",
        "View readiness did not return the shared text probe"
    );
    Ok(())
}

#[derive(Deserialize)]
struct EventMessage {
    message: String,
}

#[derive(Deserialize)]
struct RendererStartup {
    target: String,
    adapter: String,
    backend: RendererBackend,
    device_type: RendererDeviceType,
}

#[derive(Deserialize)]
struct EncoderCompletion {
    target: String,
    encoder: JpegEncoder,
    #[serde(deserialize_with = "deserialize_cuda_uuid")]
    cuda_device_uuid: Uuid,
    width: u32,
    height: u32,
    encoded_frames: u64,
}

#[derive(Default)]
struct ProcessEvents {
    renderers: Vec<RendererStartup>,
    encodes: Vec<EncoderCompletion>,
}

// The caller supplies logs fetched by CID from one owned process. Targets bind the
// events to the production owners rather than matching arbitrary diagnostic text.
fn process_events(logs: &str) -> Result<ProcessEvents> {
    let lower = logs.to_ascii_lowercase();
    for software in ["swiftshader", "llvmpipe", "software rasterizer"] {
        ensure!(
            !lower.contains(software),
            "View logs identify a software graphics backend"
        );
    }
    let mut events = ProcessEvents::default();
    for line in logs.lines() {
        let Ok(event) = serde_json::from_str::<EventMessage>(line) else {
            continue;
        };
        match event.message.as_str() {
            "hardware renderer initialized" => {
                let renderer: RendererStartup = serde_json::from_str(line)
                    .context("View renderer startup event has an invalid shape or backend")?;
                ensure!(
                    renderer.target == "veoveo_view_mcp::server",
                    "View renderer event has the wrong owner"
                );
                ensure!(
                    renderer.adapter.to_ascii_lowercase().contains("nvidia"),
                    "View renderer is not NVIDIA"
                );
                events.renderers.push(renderer);
            }
            "View GPU JPEG completed" => {
                let encode: EncoderCompletion = serde_json::from_str(line)
                    .context("View GPU JPEG completion has an invalid shape, encoder or UUID")?;
                ensure!(
                    encode.target == "veoveo_view_mcp::renderer::gpu_jpeg",
                    "View JPEG event has the wrong owner"
                );
                ensure!(
                    encode.width > 0 && encode.height > 0 && encode.encoded_frames > 0,
                    "View JPEG completion is empty"
                );
                events.encodes.push(encode);
            }
            _ => {}
        }
    }
    Ok(events)
}

pub(super) fn admit_startup_logs(logs: &str) -> Result<AdmittedAdapter> {
    let events = process_events(logs)?;
    ensure!(
        events.renderers.len() == 1,
        "View did not report one hardware renderer startup"
    );
    let mut warmup = events
        .encodes
        .iter()
        .filter(|encode| encode.width == 16 && encode.height == 16 && encode.encoded_frames == 1);
    let warmup_encode = warmup
        .next()
        .context("View did not complete its 16x16 GPU JPEG startup warmup")?;
    ensure!(
        warmup.next().is_none(),
        "View reported ambiguous GPU JPEG startup warmups"
    );
    ensure!(
        events
            .encodes
            .iter()
            .all(|encode| encode.cuda_device_uuid == warmup_encode.cuda_device_uuid),
        "View process changed CUDA UUID"
    );
    let renderer = events.renderers.into_iter().next().unwrap();
    Ok(AdmittedAdapter {
        name: renderer.adapter,
        backend: renderer.backend,
        device_type: renderer.device_type,
        jpeg_encoder: warmup_encode.encoder,
        cuda_device_uuid: warmup_encode.cuda_device_uuid,
    })
}

pub(super) fn assert_capture_completions(
    adapter: &AdmittedAdapter,
    minimum: usize,
    logs: &str,
) -> Result<()> {
    let events = process_events(logs)?;
    ensure!(
        events
            .encodes
            .iter()
            .all(|encode| encode.cuda_device_uuid == adapter.cuda_device_uuid
                && encode.encoder == adapter.jpeg_encoder),
        "View completion does not match its admitted GPU UUID or encoder"
    );
    ensure!(
        minimum > 0,
        "View capture qualification requires at least one completion"
    );
    // Production resets the counter after its 16x16 warmup. The first actual
    // capture legitimately has counter 1, just like the startup encode.
    let captures: Vec<_> = events
        .encodes
        .iter()
        .filter(|encode| encode.width == 256 && encode.height == 256 && encode.encoded_frames > 0)
        .collect();
    let mut sequences = std::collections::BTreeSet::new();
    ensure!(
        captures
            .iter()
            .all(|encode| sequences.insert(encode.encoded_frames)),
        "View reported duplicate GPU JPEG completion counters"
    );
    let completed = captures.len();
    ensure!(
        completed >= minimum,
        "View did not report {minimum} completed GPU JPEG captures on its admitted UUID"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const STARTUP: &str = concat!(
        "{\"timestamp\":\"2026-10-07T21:00:00Z\",\"level\":\"INFO\",\"message\":\"View GPU JPEG completed\",\"target\":\"veoveo_view_mcp::renderer::gpu_jpeg\",\"encoder\":\"nvjpeg_cuda_gpu\",\"cuda_device_uuid\":\"0123456789abcdef0123456789abcdef\",\"width\":16,\"height\":16,\"encoded_frames\":1}\n",
        "{\"level\":\"INFO\",\"message\":\"hardware renderer initialized\",\"target\":\"veoveo_view_mcp::server\",\"adapter\":\"NVIDIA GeForce RTX 4090\",\"backend\":\"Vulkan\",\"device_type\":\"DiscreteGpu\"}\n"
    );
    const CAPTURE: &str = "{\"message\":\"View GPU JPEG completed\",\"target\":\"veoveo_view_mcp::renderer::gpu_jpeg\",\"encoder\":\"nvjpeg_cuda_gpu\",\"cuda_device_uuid\":\"0123456789abcdef0123456789abcdef\",\"width\":256,\"height\":256,\"encoded_frames\":1}\n";

    #[test]
    fn shared_ready_text_requires_http_success_and_the_declared_body() {
        admit_readiness(200, "ready").unwrap();
        admit_readiness(200, "ready\n").unwrap();
        for (status, body) in [(503, "ready"), (200, "not ready"), (200, "{}"), (200, "")] {
            assert!(admit_readiness(status, body).is_err());
        }
    }

    #[test]
    fn actual_flattened_production_events_admit_the_compact_uuid_report() {
        let adapter = admit_startup_logs(STARTUP).unwrap();
        let report = serde_json::to_value(&adapter).unwrap();
        assert_eq!(report["backend"], "Vulkan");
        assert_eq!(report["deviceType"], "DiscreteGpu");
        assert_eq!(report["jpegEncoder"], "nvjpeg_cuda_gpu");
        assert_eq!(report["cudaDeviceUuid"], "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn startup_refuses_software_foreign_and_incomplete_events() {
        for invalid in [
            STARTUP.replace("NVIDIA GeForce RTX 4090", "Intel graphics"),
            STARTUP.replace("Vulkan", "Gl"),
            STARTUP.replace("DiscreteGpu", "Cpu"),
            STARTUP.replace("nvjpeg_cuda_gpu", "cpu_jpeg"),
            STARTUP.replace("veoveo_view_mcp::server", "foreign::server"),
            STARTUP.replace("veoveo_view_mcp::renderer::gpu_jpeg", "foreign::encoder"),
            STARTUP.replace("\"width\":16", "\"width\":256"),
            STARTUP.replace("\"encoded_frames\":1", "\"encoded_frames\":0"),
            format!("{STARTUP}\nSwiftShader software rasterizer warning"),
            format!("{STARTUP}{STARTUP}"),
            STARTUP.lines().skip(1).collect::<Vec<_>>().join("\n"),
            STARTUP.replace(
                "0123456789abcdef0123456789abcdef",
                "00000000000000000000000000000000",
            ),
            STARTUP.replace("0123456789abcdef0123456789abcdef", "not-a-cuda-uuid"),
        ] {
            assert!(admit_startup_logs(&invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn real_capture_completion_cannot_be_replaced_by_warmup_or_another_device() {
        let adapter = admit_startup_logs(STARTUP).unwrap();
        assert!(assert_capture_completions(&adapter, 0, STARTUP).is_err());
        assert!(assert_capture_completions(&adapter, 1, STARTUP).is_err());
        let second_capture = CAPTURE.replace("\"encoded_frames\":1", "\"encoded_frames\":2");
        assert_capture_completions(&adapter, 2, &format!("{STARTUP}{CAPTURE}{second_capture}"))
            .unwrap();
        assert_capture_completions(&adapter, 1, &format!("{STARTUP}{CAPTURE}")).unwrap();
        assert!(assert_capture_completions(&adapter, 2, &format!("{STARTUP}{CAPTURE}")).is_err());
        assert!(
            assert_capture_completions(&adapter, 2, &format!("{STARTUP}{CAPTURE}{CAPTURE}"))
                .is_err()
        );
        let other_device = CAPTURE.replace(
            "0123456789abcdef0123456789abcdef",
            "abcdef0123456789abcdef0123456789",
        );
        assert!(
            assert_capture_completions(&adapter, 1, &format!("{STARTUP}{other_device}")).is_err()
        );
    }
}
