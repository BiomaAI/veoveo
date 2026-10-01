use super::*;
use duckdb::params;

pub(super) fn location(value: MapLocation) -> Result<ObservedMap> {
    ObservedMap::new(
        MapKnowledgeMember::Location {
            location: value.location_id.clone(),
        },
        value.name.clone(),
        &value,
        None,
        None,
        None,
    )
}
pub(super) fn facility(value: Facility) -> Result<ObservedMap> {
    ObservedMap::new(
        MapKnowledgeMember::Facility {
            facility: value.facility_id.clone(),
        },
        value.name.clone(),
        &value,
        None,
        None,
        None,
    )
}

impl MapAnalytics {
    pub fn search_locations(
        &self,
        tenant: &str,
        request: &SearchLocationsRequest,
    ) -> Result<veoveo_mcp_knowledge_extension::SearchResults> {
        use veoveo_mcp_knowledge_extension::{SearchHit, SearchResults};
        request.coverage.validate()?;
        ensure!(
            !request.query.trim().is_empty() && request.query.len() <= 256,
            "location query must be non-empty and at most 256 bytes"
        );
        ensure!(
            (1..=100).contains(&request.limit),
            "location search limit must be within 1..=100"
        );
        let longitude = if request.coverage.west <= request.coverage.east {
            "longitude_deg BETWEEN ? AND ?"
        } else {
            "(longitude_deg >= ? OR longitude_deg <= ?)"
        };
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(&format!(
            "WITH candidates AS (
                SELECT 'location' AS kind, location_key AS member_key, name, longitude_deg, latitude_deg, source_release_key
                FROM map_visible_location WHERE tenant_key = ?
                UNION ALL
                SELECT 'facility', facility_key, name, longitude_deg, latitude_deg, source_release_key
                FROM map_visible_facility WHERE tenant_key = ? AND ?
             ), visible AS (
                SELECT *, row_number() OVER (PARTITION BY kind, member_key ORDER BY source_release_key ASC) AS position
                FROM candidates WHERE source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?)
             ) SELECT kind, member_key, name FROM visible WHERE position = 1 AND name ILIKE '%' || ? || '%'
                AND latitude_deg BETWEEN ? AND ? AND {longitude}
                ORDER BY name, kind, member_key LIMIT ?"))?;
        let mut rows = statement.query(params![
            tenant,
            tenant,
            request.include_facilities,
            tenant,
            request.query.trim(),
            request.coverage.south,
            request.coverage.north,
            request.coverage.west,
            request.coverage.east,
            request.limit
        ])?;
        let mut hits = Vec::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            let key: String = row.get(1)?;
            let title: String = row.get(2)?;
            let address = match kind.as_str() {
                "location" => MapKnowledgeMember::Location {
                    location: key.parse()?,
                },
                "facility" => MapKnowledgeMember::Facility {
                    facility: key.parse()?,
                },
                _ => anyhow::bail!("invalid Map search kind"),
            };
            hits.push(SearchHit::new(
                address.to_uri(),
                Some(summaries::link_title(&title)),
                None,
                None,
            )?);
        }
        Ok(SearchResults::new(hits)?)
    }

    pub(super) fn knowledge_page(
        &self,
        tenant: &str,
        address: &MapKnowledgePageUri,
    ) -> Result<Vec<ObservedMap>> {
        let (table, key, after) = match (address.collection(), address.after()) {
            (MapKnowledgeCollection::Locations, None) => {
                ("map_visible_location", "location_key", None)
            }
            (
                MapKnowledgeCollection::Locations,
                Some(MapKnowledgeMember::Location { location }),
            ) => (
                "map_visible_location",
                "location_key",
                Some(location.as_str()),
            ),
            (MapKnowledgeCollection::Facilities, None) => {
                ("map_visible_facility", "facility_key", None)
            }
            (
                MapKnowledgeCollection::Facilities,
                Some(MapKnowledgeMember::Facility { facility }),
            ) => (
                "map_visible_facility",
                "facility_key",
                Some(facility.as_str()),
            ),
            _ => anyhow::bail!("invalid geography page"),
        };
        // Match exact reads: when active datasets contain the same ID, the lowest
        // release ID supplies that member. Collapse in SQL before the keyset and limit.
        let connection = self.read_connection()?;
        let mut query = connection.prepare(&format!(
            "WITH members AS (SELECT {key} AS member_key, canonical_json,
             row_number() OVER (PARTITION BY {key} ORDER BY source_release_key ASC) AS position
             FROM {table} WHERE tenant_key = ? AND source_release_key IN
             (SELECT release_key FROM map_active_release WHERE tenant_key = ?))
             SELECT member_key, canonical_json FROM members WHERE position = 1 AND (? IS NULL OR member_key > ?)
             ORDER BY member_key ASC LIMIT ?"))?;
        let mut rows = query.query(params![tenant, tenant, after, after, PAGE_SIZE + 1])?;
        let mut members = Vec::new();
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            let text: String = row.get(1)?;
            let member = match address.collection() {
                MapKnowledgeCollection::Locations => {
                    let value: MapLocation = serde_json::from_str(&text)?;
                    ensure!(
                        value.location_id.as_str() == key,
                        "location document identity mismatch"
                    );
                    location(value)?
                }
                MapKnowledgeCollection::Facilities => {
                    let value: Facility = serde_json::from_str(&text)?;
                    ensure!(
                        value.facility_id.as_str() == key,
                        "facility document identity mismatch"
                    );
                    facility(value)?
                }
                _ => unreachable!("geography selection checked above"),
            };
            members.push(member);
        }
        Ok(members)
    }
}
