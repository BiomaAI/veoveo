//! Current retained producers and the browser's maintained contract receiver.
use super::*;
use serde_json::{Value, json};
use veoveo_stream_mcp::contract::*;

fn renamed_fields(value: &Value, pointer: &str, out: &mut Vec<(String, String, String)>) {
    match value {
        Value::Object(fields) => {
            for (name, value) in fields {
                if name.chars().any(char::is_uppercase) {
                    let retired = name
                        .chars()
                        .flat_map(|c| {
                            if c.is_uppercase() {
                                vec!['_', c.to_ascii_lowercase()]
                            } else {
                                vec![c]
                            }
                        })
                        .collect();
                    out.push((pointer.into(), name.clone(), retired));
                }
                renamed_fields(value, &format!("{pointer}/{name}"), out);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                renamed_fields(value, &format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}
fn refuse_retired<T: serde::de::DeserializeOwned>(wire: Value) {
    assert!(serde_json::from_value::<T>(wire.clone()).is_ok());
    let mut fields = Vec::new();
    renamed_fields(&wire, "", &mut fields);
    for (pointer, current, retired) in fields {
        for mode in 0..3 {
            let mut invalid = wire.clone();
            let object = invalid
                .pointer_mut(&pointer)
                .unwrap()
                .as_object_mut()
                .unwrap();
            let value = if mode == 2 {
                json!("conflicting retired value")
            } else {
                object[&current].clone()
            };
            if mode == 0 {
                object.remove(&current);
            }
            object.insert(retired.clone(), value);
            assert!(
                serde_json::from_value::<T>(invalid).is_err(),
                "{pointer}/{retired} mode {mode}"
            );
        }
    }
}

#[tokio::test]
async fn actual_retained_live_products_admit_current_spelling_for_browser_consumption() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let owner = crate::test_support::owner();
        let (manager, id, _) = subscription_fixture::new(owner.clone()).await;
        let selected = manager.sessions.lock().await.get(&id).unwrap().clone();
        {
            let mut state = selected.state.lock().await;
            state.started_at = "2026-09-28T00:00:00Z".parse().unwrap();
            state.stopped_at = Some("2026-09-28T00:01:00Z".parse().unwrap());
            state.frames.push_back(LiveResultFrame {
                index: 0, observed_at: "2026-09-28T00:00:00Z".into(), detections: vec![DetectionBuilder {
                    class_id: 1, label: "person".into(), confidence: Some(0.8), tracker_confidence: Some(0.9),
                    bounds: BoundingBox2D { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }, track_id: Some(7),
                }.build().unwrap()],
            });
            state.processed_frames = 1;
            state.received_video_frames = 1;
            state.video_chunks.push_back(EncodedVideoChunk { sequence: 0, timestamp_us: 0, keyframe: true, data_base64: "AA==".into() });
        }
        let session = manager.view(id, &owner).await.unwrap();
        let results = manager.results(id, &owner).await.unwrap();
        let preview = manager.preview(id, &owner).await.unwrap();
        let page = manager.page(&owner, None, 100).await;
        let started = StartLiveSessionOutput::new(id, selected.pipeline_id.clone(), LiveStartDetails {
            ingress: selected.ingress.clone(), video: selected.video.clone(), recording_output: None,
            started_at: session.started_at.clone(),
        });
        let stopped = { let state = selected.state.lock().await; stop_output(&selected, &state) };
        let products = json!({
            "pipelines": manager.catalog.pipeline_views(),
            "sessions": LiveSessionsPage { sessions: page.sessions, limit: 100, next_cursor: None },
            "session": session, "results": results, "preview": preview, "started": started, "stopped": stopped,
        });
        refuse_retired::<LiveSessionView>(products["session"].clone());
        refuse_retired::<LiveSessionsPage>(products["sessions"].clone());
        refuse_retired::<LiveResultsView>(products["results"].clone());
        refuse_retired::<LivePreviewView>(products["preview"].clone());
        refuse_retired::<StartLiveSessionOutput>(products["started"].clone());
        refuse_retired::<StopLiveSessionOutput>(products["stopped"].clone());
        for (root, old) in [("results", "veoveo.stream-live-results/v1"), ("preview", "veoveo.stream-live-preview/v1")] {
            let mut wire = products[root].clone(); wire["schema"] = old.into();
            assert!(if root == "results" { serde_json::from_value::<LiveResultsView>(wire).is_err() } else { serde_json::from_value::<LivePreviewView>(wire).is_err() });
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app/testdata/contracts.json");
        if std::env::var_os("UPDATE_STREAM_APP_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&products).unwrap() + "\n").unwrap();
        }
        let captured: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(products, captured, "current actual Stream producers differ from App fixtures");
    }).await.expect("retained Stream product admission exceeded five seconds");
}

#[tokio::test]
#[ignore = "requires VEOVEO_STREAM_GST_RUNNER built with the owning SDK; pure live-request admission"]
async fn actual_sdk_live_parser_refuses_retired_names_before_socket_or_graph_effects() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let runner = std::path::PathBuf::from(
            std::env::var_os("VEOVEO_STREAM_GST_RUNNER").expect("SDK executable required"),
        );
        let workspace = tempfile::tempdir().unwrap();
        let socket = workspace.path().join("unopened-events.sock");
        let request_path = workspace.path().join("live-request.json");
        let manager = tests::session_index();
        let pipeline = manager
            .catalog
            .pipeline(&"preview".parse().unwrap())
            .unwrap();
        let live = pipeline.live.as_ref().unwrap();
        let current = serde_json::to_value(LiveRunnerRequest {
            schema: LIVE_RUNNER_REQUEST_SCHEMA,
            session_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
            input_width: live.input_width,
            input_height: live.input_height,
            pipeline: RunnerPipeline {
                pipeline_id: pipeline.id.clone(),
                graph: live.graph.clone(),
                profile: RunnerPipelineProfile::from(&pipeline.profile),
            },
            model: None,
            max_detections_per_frame: 2,
            max_event_bytes: 1024,
            max_video_chunk_bytes: 128,
        })
        .unwrap();
        let mut cases = vec![(current.clone(), true)];
        let mut fields = Vec::new();
        renamed_fields(&current, "", &mut fields);
        for (pointer, field, retired) in fields {
            for mode in 0..3 {
                let mut invalid = current.clone();
                let object = invalid
                    .pointer_mut(&pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap();
                let value = if mode == 2 {
                    json!("retired-conflict")
                } else {
                    object[&field].clone()
                };
                if mode == 0 {
                    object.remove(&field);
                }
                object.insert(retired.clone(), value);
                cases.push((invalid, false));
            }
        }
        for (pointer, value) in [
            ("/schema", json!("veoveo.stream-live-runner-request/v1")),
            ("/sessionId", json!("01983da0-0000-4000-8000-000000000001")),
            ("/pipeline/profile/kind", json!("unknown")),
        ] {
            let mut wire = current.clone();
            *wire.pointer_mut(pointer).unwrap() = value;
            cases.push((wire, false));
        }
        for (wire, admitted) in cases {
            std::fs::write(&request_path, serde_json::to_vec(&wire).unwrap()).unwrap();
            let output = Command::new(&runner)
                .kill_on_drop(true)
                .arg("--validate-request")
                .arg("--request-json")
                .arg(&request_path)
                .arg("--event-socket")
                .arg(&socket)
                .output()
                .await
                .unwrap();
            assert_eq!(
                output.status.success(),
                admitted,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stdout.is_empty());
            assert!(output.stderr.len() <= 1024);
            assert!(!socket.exists());
        }
    })
    .await
    .expect("SDK live parser controls exceeded thirty seconds");
}

#[test]
fn actual_live_event_decoder_refuses_retired_markers_and_nested_spelling() {
    let current = json!({"schema":"veoveo.ai/stream-live-video-chunk/v2", "chunk": EncodedVideoChunk {
        sequence: 3, timestamp_us: 10, keyframe: true, data_base64: "AA==".into(),
    }});
    assert!(serde_json::from_value::<LiveRunnerEvent>(current.clone()).is_ok());
    for (field, retired) in [
        ("timestampUs", "timestamp_us"),
        ("dataBase64", "data_base64"),
    ] {
        for mode in 0..3 {
            let mut invalid = current.clone();
            let object = invalid["chunk"].as_object_mut().unwrap();
            let value = if mode == 2 {
                json!("retired-conflict")
            } else {
                object[field].clone()
            };
            if mode == 0 {
                object.remove(field);
            }
            object.insert(retired.into(), value);
            assert!(serde_json::from_value::<LiveRunnerEvent>(invalid).is_err());
        }
    }
    let mut old = current;
    old["schema"] = "veoveo.stream-live-video-chunk/v1".into();
    assert!(serde_json::from_value::<LiveRunnerEvent>(old).is_err());
    let frame =
        json!({"schema":"veoveo.ai/stream-live-frame/v2", "frame":{"index":0,"detections":[]}});
    assert!(serde_json::from_value::<LiveRunnerEvent>(frame.clone()).is_ok());
    let mut old = frame;
    old["schema"] = "veoveo.stream-live-frame/v1".into();
    assert!(serde_json::from_value::<LiveRunnerEvent>(old).is_err());
}
