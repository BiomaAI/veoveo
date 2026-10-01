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
        let sql =
            include_str!("remove_generation.surql").replace("__TABLE__", &chunk_table(generation));
        lease
            .bind(self.client().query(fenced(&sql)))
            .bind(("tenant", tenant.to_string()))
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
        let mut response = self.client().query("SELECT VALUE <string> record::id(generation) FROM ONLY $active WHERE generation.tenant = $tenant AND generation.state = 'active';")
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("tenant", tenant.to_string())).await?.knowledge_check()?;
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
        // DDL cannot bind identifiers or index dimensions. Both substitutions
        // come from closed checked types, never a caller-controlled SQL fragment.
        let table = chunk_table(id);
        let schema = include_str!("generation.surql")
            .replace("__TABLE__", &table)
            .replace("__DIMENSION__", &spec.space().dimension.get().to_string());
        let mut collections = Vec::new();
        for (collection, revision) in spec.collections() {
            let mut value = surrealdb::types::Object::new();
            value.insert("record", collection_record(tenant, collection).into_value());
            value.insert("revision", revision.to_string().into_value());
            collections.push(Value::Object(value));
        }
        lease
            .bind(self.client().query(fenced(&schema)))
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
            .query("SELECT VALUE document FROM ONLY $generation WHERE tenant = $tenant;")
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
            .bind(self.client().query(fenced(include_str!("activate.surql"))))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("previous", previous.map(generation_record)))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
