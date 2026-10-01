use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::{CollectionRegistration, GenerationSpec, IndexedMember};
use veoveo_types::{ResourceUri, Sha256Digest};

#[derive(Debug, Clone)]
pub struct MemberReadTicket {
    tenant: TenantId,
    generation: GenerationId,
    collection: CollectionId,
    approval: Sha256Digest,
    specification: Sha256Digest,
    uri: ResourceUri,
    epoch: i64,
}
impl MemberReadTicket {
    fn record(&self) -> RecordId {
        member_record(self.generation, &self.collection, &self.uri)
    }
}
fn member_record(
    generation: GenerationId,
    collection: &CollectionId,
    uri: &ResourceUri,
) -> RecordId {
    RecordId::new(
        "knowledge_member",
        Array::from(vec![
            generation.to_string(),
            collection.to_string(),
            uri.to_string(),
        ]),
    )
}

impl PlatformStore {
    /// Call only after the owner definitively confirms deletion. A failed read,
    /// timeout or disconnected subscription never establishes that fact.
    pub async fn confirm_knowledge_member_deleted(
        &self,
        ticket: &MemberReadTicket,
    ) -> Result<(), StoreError> {
        let sql = include_str!("delete_member.surql")
            .replace("__TABLE__", &chunk_table(ticket.generation));
        self.client()
            .query(sql)
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    /// Reserve before the source read. Calling this for an invalidation immediately
    /// hides old chunks and fences every earlier in-flight read of this member.
    pub async fn begin_knowledge_member_read(
        &self,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
        uri: &ResourceUri,
    ) -> Result<MemberReadTicket, StoreError> {
        let collection = registration.descriptor.collection();
        let mut response = self
            .client()
            .query(include_str!("begin_read.surql"))
            .bind(("member", member_record(generation, collection, uri)))
            .bind(("tenant", registration.tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind((
                "collection",
                collection_record(&registration.tenant, collection),
            ))
            .bind(("coverage", coverage_record(generation, collection)))
            .bind(("approval", registration.revision().to_string()))
            .bind(("specification", specification.revision().to_string()))
            .bind(("uri", uri.to_string()))
            .await?
            .knowledge_check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::Knowledge("read ticket response missing"))?;
        let epoch: Option<i64> = response.take(index)?;
        Ok(MemberReadTicket {
            tenant: registration.tenant.clone(),
            generation,
            collection: collection.clone(),
            approval: registration.revision(),
            specification: specification.revision(),
            uri: uri.clone(),
            epoch: epoch.ok_or(StoreError::Knowledge("member read was not reserved"))?,
        })
    }

    pub async fn replace_knowledge_member(
        &self,
        ticket: &MemberReadTicket,
        member: &IndexedMember,
    ) -> Result<(), StoreError> {
        if member.uri() != &ticket.uri
            || member.observation().collection() != &ticket.collection
            || member.generation_revision() != &ticket.specification
        {
            return Err(StoreError::Knowledge(
                "source read does not match its ticket",
            ));
        }
        let table = chunk_table(ticket.generation);
        let mut chunks = Vec::new();
        for (ordinal, chunk) in member.chunks().iter().enumerate() {
            let access = member.observation().access();
            let mut subjects = access
                .map(|a| {
                    a.grants
                        .iter()
                        .chain(std::iter::once(&a.owner))
                        .map(serialize_subject)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            subjects.sort();
            subjects.dedup();
            let labels = access
                .map(|a| {
                    a.data_labels
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut row = surrealdb::types::Object::new();
            row.insert("member", ticket.record().into_value());
            row.insert("tenant", ticket.tenant.to_string().into_value());
            row.insert(
                "collection",
                collection_record(&ticket.tenant, &ticket.collection).into_value(),
            );
            row.insert("collection_id", ticket.collection.to_string().into_value());
            row.insert(
                "approval_revision",
                ticket.approval.to_string().into_value(),
            );
            row.insert("uri", ticket.uri.to_string().into_value());
            row.insert("ordinal", (ordinal as i64).into_value());
            row.insert("profile_access", access.is_none().into_value());
            row.insert(
                "work_context",
                access.map(|a| a.work_context.to_string()).into_value(),
            );
            row.insert("subjects", subjects.into_value());
            row.insert("labels", labels.into_value());
            row.insert("text", chunk.text().to_owned().into_value());
            row.insert("embedding", chunk.vector().values().to_vec().into_value());
            row.insert(
                "observation",
                Document(member.observation().clone()).into_value(),
            );
            chunks.push(Value::Object(row));
        }
        self.client()
            .query(include_str!("replace.surql").replace("__TABLE__", &table))
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind(("specification", ticket.specification.to_string()))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .bind(("chunks", chunks))
            .bind(("observation", Document(member.observation().clone())))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
