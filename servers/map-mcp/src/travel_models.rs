//! Current owner and Work Context selection for completed travel-model Tasks.
use crate::contract::{
    MapTravelModelCursor, MapTravelModelPage, MapTravelModelsUri, TRAVEL_MODEL_PAGE_SIZE,
    TravelModelId, TravelModelRecord,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, de::DeserializeOwned};
use veoveo_platform_store::{
    PlatformStore, RecordId, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::TaskOwner;

const VISIBLE: &str = "server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND work_context = $context
    AND request.owner.principal_key = $principal_key AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels
    AND authority.context_key = $context_key
    AND request.owner.authority.work_context = $context_key
    AND request.owner.authority.tenant = $authority_tenant
    AND request.input.request.identity.actor.id = $principal_key
    AND request.input.request.identity.profile = $profile_key
    AND (request.input.request.identity.actor.tenant ?? NONE) = $tenant_key
    AND request.input.request.identity.actor.data_labels ALLINSIDE $labels
    AND request.input.request.identity.authority.work_context = $context_key
    AND request.input.request.identity.authority.tenant = $authority_tenant";
const COMPLETED: &str = "task_type = 'build_travel_model'
    AND request.input.kind = 'build_travel_model'
    AND status = 'succeeded' AND (result.payload.isError ?? false) = false
    AND result.payload.structuredContent.created_by = $principal_key
    AND result.payload.structuredContent.work_context = $context_key
    AND result.payload.structuredContent.travel_model_id = request.input.request.travel_model_id";

pub struct TravelModelReads<'a> {
    store: &'a PlatformStore,
}

impl<'a> TravelModelReads<'a> {
    pub fn new(store: &'a PlatformStore) -> Self {
        Self { store }
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        address: &MapTravelModelsUri,
    ) -> Result<MapTravelModelPage> {
        let mut rows = self
            .select::<PageRow>(owner, Selection::Page(address.cursor()))
            .await?;
        let more = rows.len() > TRAVEL_MODEL_PAGE_SIZE;
        rows.truncate(TRAVEL_MODEL_PAGE_SIZE);
        for row in &rows {
            row.record.validate_identity()?;
        }
        let next_cursor = if more {
            Some(MapTravelModelCursor::new(
                rows.last().expect("full page").task_id,
            )?)
        } else {
            None
        };
        Ok(MapTravelModelPage {
            items: rows.into_iter().map(|row| row.record).collect(),
            limit: TRAVEL_MODEL_PAGE_SIZE,
            next_cursor,
        })
    }

    /// ```compile_fail
    /// use veoveo_map_mcp::travel_models::TravelModelReads;
    /// use veoveo_task_runtime::TaskOwner;
    /// async fn wrong(reads: TravelModelReads<'_>, owner: TaskOwner) {
    ///     reads.get(&owner, "travel-model-id").await;
    /// }
    /// ```
    pub async fn get(
        &self,
        owner: &TaskOwner,
        id: &TravelModelId,
    ) -> Result<Option<TravelModelRecord>> {
        let records = self
            .select::<TravelModelRecord>(owner, Selection::Exact(id))
            .await?;
        ensure!(records.len() <= 1, "duplicate Map travel-model identity");
        let record = records.into_iter().next();
        if let Some(record) = &record {
            record.validate_identity()?;
        }
        Ok(record)
    }

    /// The 101st entry is the MCP adapter's lookahead for its 100-value bound.
    pub async fn complete(&self, owner: &TaskOwner, needle: &str) -> Result<Vec<TravelModelId>> {
        ensure!(
            needle.len() <= 512 && !needle.chars().any(char::is_control),
            "invalid completion search text"
        );
        self.select(owner, Selection::Completion(needle)).await
    }

    async fn select<T: DeserializeOwned>(
        &self,
        owner: &TaskOwner,
        selection: Selection<'_>,
    ) -> Result<Vec<T>> {
        ensure!(
            owner.authority.tenant.as_str() == owner.tenant_key(),
            "Map owner and Work Context belong to different tenants"
        );
        let statement = match selection {
            Selection::Page(cursor) => {
                let after = if cursor.is_some() { "AND id > $after" } else { "" };
                format!("SELECT record::id(id) AS task_id, result.payload.structuredContent AS record FROM task WHERE {VISIBLE} AND {COMPLETED} {after} ORDER BY task_id ASC LIMIT 101;")
            }
            Selection::Exact(_) => format!("SELECT VALUE result.payload.structuredContent FROM task
                WHERE {VISIBLE} AND {COMPLETED} AND request.input.request.travel_model_id = $identity LIMIT 2;"),
            Selection::Completion(_) => format!("SELECT VALUE result.payload.structuredContent.travel_model_id FROM task
                WHERE {VISIBLE} AND {COMPLETED} AND result.payload.structuredContent.travel_model_id != NONE
                AND string::lowercase(result.payload.structuredContent.travel_model_id) CONTAINS $needle
                GROUP BY result.payload.structuredContent.travel_model_id ORDER BY result.payload.structuredContent.travel_model_id ASC LIMIT 101;"),
        };
        let query = self
            .store
            .client()
            .query(statement)
            .bind(("server", RecordId::new("mcp_server", "map")))
            .bind((
                "tenant",
                deterministic_tenant_id(owner.tenant_key())?.record_id(),
            ))
            .bind((
                "owner",
                deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
            ))
            .bind(("profile", RecordId::new("profile", owner.profile.clone())))
            .bind((
                "context",
                deterministic_work_context_id(
                    owner.tenant_key(),
                    owner.authority.work_context.as_str(),
                )?
                .record_id(),
            ))
            .bind(("principal_key", owner.principal_key.clone()))
            .bind(("profile_key", owner.profile.clone()))
            .bind(("tenant_key", owner.tenant_key.clone()))
            .bind(("labels", owner.data_labels.clone()))
            .bind(("context_key", owner.authority.work_context.to_string()))
            .bind(("authority_tenant", owner.authority.tenant.to_string()));
        let query = match selection {
            Selection::Page(Some(cursor)) => query.bind((
                "after",
                veoveo_platform_store::task_record_id(cursor.task_id()),
            )),
            Selection::Page(None) => query,
            Selection::Exact(id) => query.bind(("identity", id.to_string())),
            Selection::Completion(needle) => query.bind(("needle", needle.to_lowercase())),
        };
        let mut response = query.await?.check()?;
        let rows: Vec<serde_json::Value> = response.take(0)?;
        rows.into_iter()
            .map(|row| serde_json::from_value(row).map_err(Into::into))
            .collect()
    }
}
enum Selection<'a> {
    Page(Option<&'a MapTravelModelCursor>),
    Exact(&'a TravelModelId),
    Completion(&'a str),
}

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
struct PageRow {
    task_id: veoveo_types::TaskId,
    record: TravelModelRecord,
}
