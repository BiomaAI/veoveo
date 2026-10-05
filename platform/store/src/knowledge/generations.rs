use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::GenerationSpec;

impl PlatformStore {
    /// Reclaim a retired or abandoned generation. The active pointer is checked
    /// in the same transaction that cascades member records and drops its indexes.
    pub async fn remove_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        generation: GenerationId,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        let sql = include_str!("../queries/knowledge/remove_generation.surql");
        lease
            .bind(self.client().query(sql))
            .bind(("tenant", tenant.to_string()))
            .bind(("chunk_table", chunk_table(generation)))
            .bind(("generation", generation_record(generation)))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    pub async fn active_knowledge_generation(
        &self,
        tenant: &TenantId,
    ) -> Result<Option<GenerationId>, StoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/knowledge/generations/active_knowledge_generation.surql"
            ))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("tenant", tenant.to_string()))
            .await?
            .knowledge_check()?;
        let id: Option<String> = response.take(0)?;
        id.map(|id| {
            id.try_into()
                .map_err(|_| StoreError::Knowledge("invalid active generation identity"))
        })
        .transpose()
    }
    pub async fn create_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        id: GenerationId,
        spec: &GenerationSpec,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        // The native HNSW grammar requires a literal dimension from the checked spec.
        let table = chunk_table(id);
        let schema = include_str!("../queries/knowledge/generation.surql")
            .replace("__DIMENSION__", &spec.space().dimension.get().to_string());
        let mut collections = Vec::new();
        for (collection, revision) in spec.collections() {
            let mut value = surrealdb::types::Object::new();
            value.insert("record", collection_record(tenant, collection).into_value());
            value.insert("revision", revision.to_string().into_value());
            collections.push(Value::Object(value));
        }
        lease
            .bind(self.client().query(&schema))
            .bind(("chunk_table", table))
            .bind(("generation", generation_record(id)))
            .bind(("tenant", tenant.to_string()))
            .bind(("document", Document(spec.clone())))
            .bind(("revision", spec.revision().to_string()))
            .bind(("collections", collections))
            .await?
            .knowledge_check()?;
        Ok(())
    }

    pub async fn knowledge_generation(
        &self,
        tenant: &TenantId,
        id: GenerationId,
    ) -> Result<Option<GenerationSpec>, StoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/knowledge/generations/knowledge_generation.surql"
            ))
            .bind(("generation", generation_record(id)))
            .bind(("tenant", tenant.to_string()))
            .await?
            .knowledge_check()?;
        let document: Option<Document<GenerationSpec>> = response.take(0)?;
        Ok(document.map(|d| d.0))
    }

    pub async fn activate_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        generation: GenerationId,
        previous: Option<GenerationId>,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        lease
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/activate.surql")),
            )
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("previous", previous.map(generation_record)))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
