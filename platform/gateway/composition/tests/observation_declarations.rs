//! Fresh owner lane qualification for every current observation declaration.
use futures::StreamExt;
use veoveo_modules::{ObservationReplay, ObservationTable};
use veoveo_platform_store::{ChangefeedCursor, PlatformTable};
#[path = "../../../../testing/fixtures/store.rs"]
mod store;

#[tokio::test]
async fn all_kernel_and_owner_observations_match_native_schema_retention() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let execution = |name| store::module_lanes::execution(name).unwrap();
        let fixture = store::TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(execution("agents")).unwrap(),
            veoveo_computers::schema::module_setup(execution("computers")).unwrap(),
            veoveo_recording_mcp::schema::module_setup(execution("recordings")).unwrap(),
            veoveo_map_mcp::schema::module_setup(execution("map")).unwrap(),
            veoveo_time_mcp::schema::module_setup(execution("time")).unwrap(),
            veoveo_frames_mcp::schema::module_setup(execution("frames")).unwrap(),
            veoveo_media_mcp::schema::module_setup(execution("media")).unwrap(),
            veoveo_uav_sim_mcp::schema::module_setup(execution("uav")).unwrap(),
        ]).await;
        let mut tables: Vec<ObservationTable> =
            PlatformTable::ALL.into_iter().map(Into::into).collect();
        tables.extend(
            veoveo_agent_runtime::AgentObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_computers::schema::ComputerObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_recording_mcp::schema::RecordingObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_map_mcp::MapObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_time_mcp::TimeObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_frames_mcp::FramesObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_media_mcp::MediaObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        tables.extend(
            veoveo_uav_sim_mcp::UavObservationTable::ALL
                .iter()
                .copied()
                .map(Into::into),
        );
        let mut response = fixture
            .a
            .client()
            .query(include_str!("queries/observation_declarations/all_kernel_and_owner_observations_match_native_schema_retention/statement_1.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let info: Option<serde_json::Value> = response.take(0).unwrap();
        let definitions = info.unwrap()["tables"].as_object().unwrap().clone();
        let mut unique = std::collections::BTreeSet::new();
        for table in tables {
            assert!(
                unique.insert(table.as_str().to_owned()),
                "duplicate observation declaration {}",
                table.as_str()
            );
            let definition = definitions
                .get(table.as_str())
                .unwrap_or_else(|| panic!("missing current table {}", table.as_str()))
                .as_str()
                .unwrap();
            assert!(definition.contains("SCHEMAFULL"), "{}", definition);
            match table.replay() {
                ObservationReplay::LiveOnly => {
                    assert!(!definition.contains("CHANGEFEED"), "{}", definition)
                }
                ObservationReplay::Changefeed(retention) => {
                    let mut words = definition.split_whitespace();
                    let duration = words
                        .find(|word| *word == "CHANGEFEED")
                        .and_then(|_| words.next())
                        .expect("native changefeed retention token");
                    let actual: surrealdb::types::Duration =
                        duration.parse().expect("SDK duration admission");
                    assert!(
                        actual.secs() >= u64::from(retention.seconds()),
                        "declared retention exceeds installed schema: {}",
                        definition
                    );
                }
            }
        }
        // Rejection happens before a source can produce an establishment baseline.
        let mut source = fixture.a.observe_changes(
            vec![PlatformTable::KnowledgeCoordinator],
            ChangefeedCursor::initial(),
        );
        assert!(source.next().await.unwrap().is_err());
        assert!(source.next().await.is_none());
        // The qualified 3.3 grammar rejects parameters in SHOW numeric slots.
        let result = fixture
            .a
            .client()
            .query(include_str!("queries/observation_declarations/malformed_show_numeric_parameters.surql"))
            .bind(("cursor", 0_i64))
            .bind(("limit", 1_u32))
            .await;
        assert!(match result {
            Err(_) => true,
            Ok(response) => response.check().is_err(),
        });
    })
    .await
    .expect("observation schema qualification exceeded120seconds");
}
