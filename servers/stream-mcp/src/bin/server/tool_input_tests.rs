//! Hosted protocol admission with unavailable execution paths; no GPU work occurs.
use super::*;
use serde_json::json;
use std::time::Duration;
use veoveo_mcp_contract::hosting::testing::{self, TestGateway};

#[tokio::test]
async fn unknown_tool_arguments_complete_before_recording_or_runner_access() {
    tokio::time::timeout(Duration::from_secs(120),async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::store_fixture::TestDb::new().await;
        crate::store_fixture::module_lanes::install(&db.a, vec![veoveo_stream_mcp::schema::module_setup(crate::store_fixture::module_lanes::execution("stream").unwrap()).unwrap()]).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let catalog = Arc::new(PipelineCatalog::new(vec![],vec![veoveo_stream_mcp::catalog::PipelineConfig {
            id:"preview".parse().unwrap(),title:"Preview".into(),description:String::new(),
            profile:veoveo_stream_mcp::catalog::PipelineProfileConfig::PassThrough,recording_replay:None,
            live:Some(serde_json::from_value(json!({
                "input_width":640,"input_height":480,"codec":"avc1.42e01f","frame_rate":30,"expected_bitrate_bps":4000000,
                "ingress":{"advertised_host":"stream-mcp","port":9001,"payload_type":97,"clock_rate":90000},
                "graph":{"launch":"udpsrc name=source ! h264parse ! identity name=encoded-output ! fakesink",
                    "source_element":"source","encoded_output_element":"encoded-output"}
            })).unwrap()),
        }]).unwrap());
        let live = Arc::new(LiveSessionManager::new(catalog.clone(),root.path().join("unavailable-runner"),
            Duration::from_secs(1),2,2,2,1024,128,Arc::new(SubscriptionHub::new())).unwrap());
        let artifacts = veoveo_artifact_client::HttpArtifactPlane::new("http://127.0.0.1:9");
        let recordings = RecordingReader::new(db.a.clone(),root.path().to_path_buf(),
            veoveo_recording_reader::cache::LayerCache::new(root.path().join("cache"),
                veoveo_recording_reader::cache::LayerCacheLimits {managed_bytes:1024,minimum_free_bytes:1},artifacts).unwrap()).unwrap();
        let state = Arc::new(AppState {
            live_app:veoveo_mcp_apps_extension::AppHtml::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/live.html")).unwrap(),
            tasks:veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(),"stream","strict-input")).unwrap(),
            artifacts:ArtifactRepository::new("http://127.0.0.1:9"),recordings:Arc::new(recordings),catalog,
            executor:StreamExecutor::new(root.path().join("unavailable-runner"),Duration::from_secs(1),2,2,1024).unwrap(),
            source_limits:VideoSourceLimits {max_samples:2,max_encoded_bytes:1024,max_segment_bytes:1024},
            max_artifact_bytes:1024,max_inline_resource_bytes:1024,work_slots:Arc::new(tokio::sync::Semaphore::new(1)),live,
        });
        let handler = state.clone();
        let gateway = TestGateway::new(testing::for_domain::<StreamMcp>().handler(move || Hosted::new(StreamMcp::new(handler.clone()))).build());
        let discover = gateway.rpc("server/discover",json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"video":{
            "recording_uri":"recording://recordings/01983da0-0000-7000-8000-000000000000",
            "entity_path":"/camera/front","timeline":"sensor_time","range":{"start":10,"end":20}
        },"pipeline_id":"preview"});
        let _:RunRecordingRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway.rpc("tools/call",json!({"name":"run_recording","arguments":arguments})).await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"],"complete","{body}");
        let result:rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error,Some(true));
        assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
        assert!(state.tasks.list().await.unwrap().is_empty());
        assert_eq!(state.work_slots.available_permits(),1);
    }).await.expect("Stream argument admission exceeded 120 seconds");
}
