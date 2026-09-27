//! Geography completion searches committed, active projection keys in DuckDB.
use super::MapAnalytics;
use anyhow::{Result, ensure};
use duckdb::params;

#[derive(Clone, Copy)]
pub(crate) enum GeographyCompletion {
    Location,
    Facility,
}
impl MapAnalytics {
    pub(crate) fn complete_geography(
        &self,
        tenant: &str,
        domain: GeographyCompletion,
        needle: &str,
    ) -> Result<Vec<String>> {
        ensure!(
            needle.len() <= 512 && !needle.chars().any(char::is_control),
            "invalid completion search text"
        );
        let (table, field) = match domain {
            GeographyCompletion::Location => ("map_visible_location", "location_key"),
            GeographyCompletion::Facility => ("map_visible_facility", "facility_key"),
        };
        let connection = self.read_connection()?;
        let mut query=connection.prepare(&format!("SELECT DISTINCT {field} FROM {table} WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND contains(lower({field}), ?) ORDER BY {field} ASC LIMIT 101"))?;
        Ok(query
            .query_map(params![tenant, tenant, needle.to_lowercase()], |row| {
                row.get(0)
            })?
            .collect::<duckdb::Result<Vec<String>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::MapAnalyticsConfig;

    #[test]
    fn geography_completion_matches_beyond_the_old_cap_and_excludes_inactive_releases() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = tempfile::TempDir::new().unwrap();
        let analytics = MapAnalytics::open(MapAnalyticsConfig {
            database_path: root.path().join("map.duckdb"),
            authoring_task_root: root.path().join("tasks"),
            spill_dir: root.path().join("spill"),
            spatial_extension: extension.into(),
            memory_limit: "256MB".into(),
            threads: 1,
        })
        .unwrap();
        let connection = analytics.connection().unwrap();
        connection.execute_batch("INSERT INTO map_active_release VALUES ('a','dataset','active'), ('b','dataset','active'); INSERT INTO map_release_projection SELECT tenant, release, 'attempt', 10050, 10050, 0, 0, 0, 0, now() FROM (VALUES ('a','active'),('b','active'),('a','inactive')) AS entries(tenant,release);").unwrap();
        for (table, key, prefix, kind) in [
            ("map_location", "location_key", "location-", ""),
            ("map_facility", "facility_key", "facility-", ", 'site'"),
        ] {
            connection.execute_batch(&format!("INSERT INTO {table} SELECT 'a', '{prefix}' || printf('%08x-0000-7000-8000-000000000000',i), 'name'{kind}, 0, 0, '{{}}', 'active', 'attempt', i FROM range(10050) t(i); INSERT INTO {table} SELECT 'b', '{prefix}foreign', 'name'{kind}, 0, 0, '{{}}', 'active', 'attempt', 0; INSERT INTO {table} SELECT 'a', '{prefix}inactive', 'name'{kind}, 0, 0, '{{}}', 'inactive', 'attempt', 0; INSERT INTO {table} SELECT 'a', '{prefix}pending', 'name'{kind}, 0, 0, '{{}}', 'active', 'uncommitted', 0; INSERT INTO {table} SELECT * FROM {table} WHERE {key} = '{prefix}00000000-0000-7000-8000-000000000000';")).unwrap();
        }
        for (domain, prefix) in [
            (GeographyCompletion::Location, "location-"),
            (GeographyCompletion::Facility, "facility-"),
        ] {
            assert_eq!(
                analytics.complete_geography("a", domain, "").unwrap().len(),
                101
            );
            assert_eq!(
                analytics
                    .complete_geography("a", domain, "00002741")
                    .unwrap(),
                vec![format!("{prefix}00002741-0000-7000-8000-000000000000")]
            );
            assert_eq!(
                analytics
                    .complete_geography("a", domain, &prefix.to_uppercase())
                    .unwrap()
                    .len(),
                101
            );
            for needle in ["foreign", "inactive", "pending", "' OR true --"] {
                assert!(
                    analytics
                        .complete_geography("a", domain, needle)
                        .unwrap()
                        .is_empty()
                );
            }
        }
    }
}
