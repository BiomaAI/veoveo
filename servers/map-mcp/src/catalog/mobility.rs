//! Tenant-visible immutable profiles, ordered by typed ID and numeric version.
use super::{MapAccessContext, MapCatalog, decode, wire};
use crate::contract::{
    MOBILITY_PROFILE_PAGE_SIZE, MapMobilityProfileCursor, MapMobilityProfilePage,
    MapMobilityProfilesUri, MobilityProfile, MobilityProfileId, MobilityProfileVersion,
};
use anyhow::{Result, ensure};
use veoveo_platform_store::{MapMobilityProfileRecord, RecordId};

#[derive(Clone, Copy)]
enum Selection<'a> {
    Exact(&'a MobilityProfileId, MobilityProfileVersion),
    Page(Option<&'a MapMobilityProfileCursor>),
}

impl MapCatalog {
    /// ```compile_fail
    /// use veoveo_map_mcp::{catalog::{MapAccessContext, MapCatalog}, contract::MobilityProfileId};
    /// async fn wrong(catalog: MapCatalog, scope: MapAccessContext) {
    ///     catalog.mobility_profile(&scope, &MobilityProfileId::new(), 1).await;
    /// }
    /// ```
    pub async fn mobility_profile(
        &self,
        scope: &MapAccessContext,
        id: &MobilityProfileId,
        version: MobilityProfileVersion,
    ) -> Result<Option<MobilityProfile>> {
        let mut rows = self
            .select_mobility(scope, Selection::Exact(id, version))
            .await?;
        ensure!(rows.len() <= 1, "duplicate Map mobility profile version");
        Ok(rows.pop())
    }

    pub async fn mobility_profiles_page(
        &self,
        scope: &MapAccessContext,
        address: &MapMobilityProfilesUri,
    ) -> Result<MapMobilityProfilePage> {
        MapMobilityProfilePage::from_lookahead(
            self.select_mobility(scope, Selection::Page(address.cursor()))
                .await?,
        )
        .map_err(Into::into)
    }

    async fn select_mobility(
        &self,
        scope: &MapAccessContext,
        selection: Selection<'_>,
    ) -> Result<Vec<MobilityProfile>> {
        let (predicate, limit) = match selection {
            Selection::Exact(_, _) => ("AND profile_key = $key AND profile_version = $version", 2),
            Selection::Page(None) => ("", MOBILITY_PROFILE_PAGE_SIZE + 1),
            Selection::Page(Some(_)) => (
                "AND (profile_key > $key OR (profile_key = $key AND profile_version > $version))",
                MOBILITY_PROFILE_PAGE_SIZE + 1,
            ),
        };
        let query = self
            .store()
            .client()
            .query(format!(
                "SELECT * FROM map_mobility_profile WHERE tenant = $tenant {predicate}
             ORDER BY profile_key ASC, profile_version ASC LIMIT $limit TIMEOUT 5s;"
            ))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("limit", limit));
        let query = match selection {
            Selection::Exact(id, version) => query
                .bind(("key", id.to_string()))
                .bind(("version", version.get())),
            Selection::Page(Some(cursor)) => query
                .bind(("key", cursor.after_id().to_string()))
                .bind(("version", cursor.after_version().get())),
            Selection::Page(None) => query,
        };
        let mut response = query.await?.check()?;
        let rows: Vec<MapMobilityProfileRecord> = response.take(0)?;
        rows.into_iter().map(checked_profile).collect()
    }

    pub async fn complete_mobility_profiles(
        &self,
        scope: &MapAccessContext,
        needle: &str,
    ) -> Result<Vec<MobilityProfileId>> {
        validate_needle(needle)?;
        let mut response = self
            .store()
            .client()
            .query(
                "SELECT VALUE profile_key FROM map_mobility_profile WHERE tenant = $tenant
             AND string::lowercase(profile_key) CONTAINS $needle
             GROUP BY profile_key ORDER BY profile_key ASC LIMIT 101 TIMEOUT 5s;",
            )
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        let values: Vec<String> = response.take(0)?;
        values
            .into_iter()
            .map(|id| MobilityProfileId::parse(id).map_err(Into::into))
            .collect()
    }

    pub async fn complete_mobility_versions(
        &self,
        scope: &MapAccessContext,
        parent: Option<&MobilityProfileId>,
        needle: &str,
    ) -> Result<Vec<MobilityProfileVersion>> {
        validate_needle(needle)?;
        let mut response = self
            .store()
            .client()
            .query(
                "SELECT VALUE profile_version FROM map_mobility_profile WHERE tenant = $tenant
             AND ($parent = NONE OR profile_key = $parent)
             AND type::string(profile_version) CONTAINS $needle
             GROUP BY profile_version ORDER BY profile_version ASC LIMIT 101 TIMEOUT 5s;",
            )
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("parent", parent.map(ToString::to_string)))
            .bind(("needle", needle.to_owned()))
            .await?
            .check()?;
        let values: Vec<i64> = response.take(0)?;
        values
            .into_iter()
            .map(|version| MobilityProfileVersion::try_from(version).map_err(Into::into))
            .collect()
    }
}

fn validate_needle(needle: &str) -> Result<()> {
    ensure!(
        needle.len() <= 512 && !needle.chars().any(char::is_control),
        "invalid completion search text"
    );
    Ok(())
}

fn checked_profile(row: MapMobilityProfileRecord) -> Result<MobilityProfile> {
    let profile: MobilityProfile = decode(&row.canonical_json, "mobility profile")?;
    profile.validate()?;
    let meta = profile.metadata();
    ensure!(
        row.id
            == RecordId::new(
                "map_mobility_profile",
                format!("{}:{}", meta.profile_id, meta.version)
            )
            && row.profile_key == meta.profile_id.as_str()
            && MobilityProfileVersion::try_from(row.profile_version)? == meta.version
            && row.name == meta.name
            && row.family == wire(&profile.family())?
            && row.valid_from == meta.valid_from
            && row.valid_until == meta.valid_until,
        "stored mobility profile identity, version or indexed metadata disagree with its document"
    );
    Ok(profile)
}

#[cfg(test)]
mod tests;
