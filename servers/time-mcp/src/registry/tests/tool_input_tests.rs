//! Hosted argument admission using the existing isolated authority fixture.
#[path = "../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use super::*;
use serde_json::json;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

fn retired_members(wire: &serde_json::Value) -> Vec<serde_json::Value> {
    fn collect(value: &serde_json::Value, path: &str, keys: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    if key.bytes().any(|b| b.is_ascii_uppercase()) {
                        keys.push((path.into(), key.clone()));
                    }
                    collect(child, &format!("{path}/{key}"), keys);
                }
            }
            serde_json::Value::Array(values) => {
                for (i, child) in values.iter().enumerate() {
                    collect(child, &format!("{path}/{i}"), keys);
                }
            }
            _ => {}
        }
    }
    let mut keys = vec![];
    collect(wire, "", &mut keys);
    let mut controls = vec![];
    for (path, key) in keys {
        let retired: String = key
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['_', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        for mixed in [false, true] {
            let mut bad = wire.clone();
            let object = bad.pointer_mut(&path).unwrap().as_object_mut().unwrap();
            let value = object[&key].clone();
            if !mixed {
                object.remove(&key);
            }
            object.insert(retired.clone(), value);
            controls.push(bad);
        }
    }
    controls
}

#[tokio::test]
async fn unknown_tool_arguments_complete_before_time_resolution() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.clone());
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: files.root.path().join("scratch"),
                release_root: files.root.path().join("releases"),
                zic_executable: files.root.path().join("unavailable-zic"),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
            catalog.clone(),
        )
        .unwrap();
        let state = Arc::new(crate::state::TimeApplication {
            tasks: veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "time", "strict-input"),
            catalog,
            authorities: files.registry(),
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
            event_watchers: Arc::default(),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || Hosted::new(crate::mcp::TimeMcp::new(handler.clone())))
                .admin_routes(crate::admin::router(state.clone()))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        // Real authenticated HTTP captures remain snake_case while owner JSON is camelCase.
        let token = |tenant: &str| {
            let mut principal = testing::principal();
            principal.tenant = Some(tenant.parse().unwrap());
            for scope in TimeScope::ALL {
                principal.scopes.insert((*scope).into());
            }
            let mut authority = testing::authority();
            authority.tenant = tenant.parse().unwrap();
            veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
                veoveo_mcp_contract::TokenIssuer::parse(
                    veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER,
                )
                .unwrap(),
                testing::signing_key("test-key"),
            )
            .issue(
                "operations".parse().unwrap(),
                "time".parse().unwrap(),
                principal,
                authority,
                None,
                Utc::now() + chrono::TimeDelta::minutes(5),
            )
            .unwrap()
            .bearer_token
        };
        let owner_token = token("tenant-a");
        let inputs: Vec<serde_json::Value> =
            serde_json::from_slice(include_bytes!("../../../testdata/controlled-inputs.json"))
                .unwrap();
        let calendar: OperationalCalendar = serde_json::from_value(
            inputs
                .iter()
                .find(|case| case["tool"] == "expand_schedule")
                .unwrap()["arguments"]["calendar"]
                .clone(),
        )
        .unwrap();
        // Decoder corpus IDs exercise the public lexical profile. This native
        // HTTP fixture must use the catalog's admitted UUIDv7 record identity.
        let mut calendar: crate::contract::OperationalCalendarValue = calendar.into();
        calendar.calendar_id =
            crate::contract::CalendarId::parse("calendar-0195dabe-7777-7abc-8def-000000000007")
                .unwrap();
        let calendar = calendar.build().unwrap();
        let create = CreateCalendarRequestValue {
            calendar: calendar.clone(),
            idempotency_key: "http-calendar-fixture".into(),
        }
        .build()
        .unwrap();
        let request = gateway
            .request("/admin/calendars")
            .method("POST")
            .header("authorization", format!("Bearer {owner_token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&create).unwrap()))
            .unwrap();
        let (status, body) = gateway.send(request).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{body}");
        let created: OperationalCalendar = serde_json::from_str(&body).unwrap();
        assert_eq!(created, calendar);
        let path = format!(
            "/admin/calendars/{}/versions/{}",
            calendar.calendar_id,
            calendar.version.get()
        );
        for (route, bearer, expected) in [
            (
                path.clone(),
                owner_token.clone(),
                axum::http::StatusCode::OK,
            ),
            (
                "/admin/calendars/epoch-wrong/versions/1".into(),
                owner_token.clone(),
                axum::http::StatusCode::BAD_REQUEST,
            ),
            (
                format!("/admin/calendars/{}/versions/0", calendar.calendar_id),
                owner_token.clone(),
                axum::http::StatusCode::BAD_REQUEST,
            ),
            (
                format!(
                    "/admin/calendars/{}/versions/9223372036854775808",
                    calendar.calendar_id
                ),
                owner_token.clone(),
                axum::http::StatusCode::BAD_REQUEST,
            ),
            (path, token("tenant-b"), axum::http::StatusCode::NOT_FOUND),
        ] {
            let (status, body) = gateway
                .send(
                    gateway
                        .request(&route)
                        .header("authorization", format!("Bearer {bearer}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await;
            assert_eq!(status, expected, "{route}: {body}");
            if expected == axum::http::StatusCode::OK {
                assert_eq!(
                    serde_json::from_str::<OperationalCalendar>(&body).unwrap(),
                    calendar
                );
            }
        }
        let (_, clock) = gateway
            .rpc_with(
                "resources/read",
                json!({"uri":"time://clock/current"}),
                Some(&owner_token),
            )
            .await;
        assert!(clock.get("error").is_none(), "{clock}");
        let clock_text = clock["result"]["contents"][0]["text"].as_str().unwrap();
        let current: ClockCurrent = serde_json::from_str(clock_text).unwrap();
        let wire = serde_json::to_value(&current).unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(ClockCurrent)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&wire));
        assert!(wire.get("effectivePolicy").is_some() && wire.get("clockQuality").is_some());
        for (current, retired) in [
            ("effectivePolicy", "effective_policy"),
            ("clockQuality", "clock_quality"),
        ] {
            assert!(wire.get(retired).is_none());
            for mixed in [false, true] {
                let mut bad = wire.clone();
                bad[retired] = bad[current].clone();
                if !mixed {
                    bad.as_object_mut().unwrap().remove(current);
                }
                assert!(!validator.is_valid(&bad));
                assert!(serde_json::from_value::<ClockCurrent>(bad).is_err());
            }
        }
        let listed = gateway.rpc("prompts/list", json!({})).await;
        assert!(listed.get("error").is_none(), "{listed}");
        for (name, arguments, argument, retired) in [
            (
                "resolve_operational_time",
                json!({"expression":"2026-01-01T00:00:00Z","zone_id":"UTC"}),
                "zone_id",
                "zoneId",
            ),
            (
                "expand_operational_calendar",
                json!({"calendar_id":"calendar-fixture","version":"1","horizon":"[0,100)"}),
                "calendar_id",
                "calendarId",
            ),
        ] {
            let declaration = listed["result"]["prompts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|prompt| prompt["name"] == name)
                .unwrap();
            assert!(
                declaration["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value["name"] == argument)
            );
            assert!(
                !declaration["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value["name"] == retired)
            );
            let result = gateway
                .rpc("prompts/get", json!({"name":name,"arguments":arguments}))
                .await;
            assert!(result.get("error").is_none(), "{result}");
            assert!(!result["result"]["messages"].as_array().unwrap().is_empty());
            for mixed in [false, true] {
                let mut bad = arguments.clone();
                bad[retired] = bad[argument].clone();
                if !mixed {
                    bad.as_object_mut().unwrap().remove(argument);
                }
                let refused = gateway
                    .rpc("prompts/get", json!({"name":name,"arguments":bad}))
                    .await;
                assert_eq!(refused["error"]["code"], -32602, "{refused}");
            }
        }
        let mut arguments =
            json!({"expression":{"format":"rfc3339","value":"2026-01-01T00:00:00Z"}});
        let _: ResolveTimeRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"resolve_time","arguments":arguments}),
            )
            .await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 30);
        for case in cases {
            match case.tool.as_str() {
                "resolve_time" => {
                    let _: crate::contract::ResolveTimeRequest = case.decode();
                }
                "convert_time" => {
                    let _: crate::contract::ConvertTimeRequest = case.decode();
                }
                "evaluate_windows" => {
                    let _: crate::contract::EvaluateWindowsRequest = case.decode();
                }
                "expand_schedule" => {
                    let _: crate::contract::ExpandScheduleRequest = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for arguments in retired_members(&case.arguments) {
                let body = gateway
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(body.get("error").is_none(), "{}: {body}", case.branch);
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(result.is_error, Some(true), "{}: {body}", case.branch);
                assert!(state.tasks.list().await.unwrap().is_empty());
                assert!(state.event_watchers.lock().await.is_empty());
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let body = gateway
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(
                    body.get("error").is_none(),
                    "{} {location}: {body}",
                    case.branch
                );
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{} {location}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {body}",
                    case.branch
                );
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
        assert!(state.tasks.list().await.unwrap().is_empty());
        assert!(state.event_watchers.lock().await.is_empty());
    })
    .await
    .expect("Time argument admission exceeded 120 seconds");
}
