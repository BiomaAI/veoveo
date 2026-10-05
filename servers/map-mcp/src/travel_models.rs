//! Current owner and Work Context selection for completed travel-model Tasks.
use crate::contract::MapTaskKind;
use crate::contract::{
    MapTravelModelCursor, MapTravelModelPage, MapTravelModelsUri, TRAVEL_MODEL_PAGE_SIZE,
    TravelModelId, TravelModelRecord,
};
use anyhow::{Context, Result, ensure};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::{
    PlatformStore, RecordId, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskTypeDefinition;

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
            .select(owner, Selection::Page(address.cursor()))
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
        let records = self.select(owner, Selection::Exact(id)).await?;
        ensure!(records.len() <= 1, "duplicate Map travel-model identity");
        let record = records.into_iter().next().map(|row| row.record);
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
        Ok(self
            .select(owner, Selection::Completion(needle))
            .await?
            .into_iter()
            .map(|row| row.record.travel_model_id)
            .collect())
    }

    async fn select(&self, owner: &TaskOwner, selection: Selection<'_>) -> Result<Vec<PageRow>> {
        ensure!(
            owner.authority.tenant.as_str() == owner.tenant_key(),
            "Map owner and Work Context belong to different tenants"
        );
        let statement = match selection {
            Selection::Page(None) => include_str!("queries/travel_models/first_page.surql"),
            Selection::Page(Some(_)) => include_str!("queries/travel_models/after_page.surql"),
            Selection::Exact(_) => include_str!("queries/travel_models/exact.surql"),
            Selection::Completion(_) => include_str!("queries/travel_models/completion.surql"),
        };
        let query = self
            .store
            .client()
            .query(statement)
            .bind((
                "task_type",
                MapTaskKind::BuildTravelModel.name().to_string(),
            ))
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
        let rows = if matches!(selection, Selection::Completion(_)) {
            let groups: Vec<CompletionRow> = response.take(0)?;
            groups
                .into_iter()
                .map(|group| {
                    ensure!(
                        group.model_id == group.record.identity.travel_model_id,
                        "Map completion group differs from its lookup"
                    );
                    decode(group.record)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            let rows: Vec<crate::task_lookup::Row> = response.take(0)?;
            rows.into_iter().map(decode).collect::<Result<Vec<_>>>()?
        };
        Ok(rows)
    }
}
enum Selection<'a> {
    Page(Option<&'a MapTravelModelCursor>),
    Exact(&'a TravelModelId),
    Completion(&'a str),
}

#[cfg(test)]
mod tests;

struct PageRow {
    task_id: veoveo_types::TaskId,
    record: TravelModelRecord,
}

#[derive(SurrealValue)]
struct CompletionRow {
    #[surreal(wrap)]
    model_id: TravelModelId,
    record: crate::task_lookup::Row,
}
fn decode(row: crate::task_lookup::Row) -> Result<PageRow> {
    crate::task_lookup::verify_projection(&row.identity)?;
    let record = row
        .settlement
        .expected_result
        .context("Map product has no integrity result")?
        .into_output();
    ensure!(
        record.travel_model_id == row.identity.travel_model_id
            && record.created_by == row.identity.created_by
            && record.work_context == row.identity.work_context,
        "Map travel lookup differs from its retained product"
    );
    ensure!(
        row.task.table.as_str() == "task",
        "Map lookup has wrong Task table"
    );
    let veoveo_platform_store::RecordIdKey::Uuid(id) = row.task.key else {
        anyhow::bail!("Map lookup has wrong Task identity")
    };
    Ok(PageRow {
        task_id: veoveo_types::TaskId::parse(id.to_string())?,
        record,
    })
}
