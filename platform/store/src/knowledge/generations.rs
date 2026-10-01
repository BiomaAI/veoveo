use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::GenerationSpec;

impl PlatformStore {
    /// Reclaim a retired or abandoned generation. The active pointer is checked
    /// in the same transaction that cascades member records and drops its indexes.
    pub async fn remove_knowledge_generation(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
    ) -> Result<(), StoreError> {
        let sql =
            include_str!("remove_generation.surql").replace("__TABLE__", &chunk_table(generation));
        self.client()
            .query(sql)
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
        tenant: &TenantId,
        id: GenerationId,
        spec: &GenerationSpec,
    ) -> Result<(), StoreError> {
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
        self.client()
            .query(schema)
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

    /// The caller finished a complete source enumeration, including deletions.
    /// Any unresolved or invalidated member prevents this receipt and activation.
    pub async fn complete_knowledge_collection(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
        collection: &CollectionId,
        revision: &veoveo_types::Sha256Digest,
    ) -> Result<(), StoreError> {
        self.client()
            .query(include_str!("coverage.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind(("collection", collection_record(tenant, collection)))
            .bind(("coverage", coverage_record(generation, collection)))
            .bind(("revision", revision.to_string()))
            .await?
            .knowledge_check()?;
        Ok(())
    }

    pub async fn activate_knowledge_generation(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
        previous: Option<GenerationId>,
    ) -> Result<(), StoreError> {
        self.client()
            .query(include_str!("activate.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("previous", previous.map(generation_record)))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
