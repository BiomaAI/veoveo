use super::admission::Admission;
use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::{CollectionRegistration, GenerationSpec, IndexedMember};
use veoveo_types::{ResourceUri, Sha256Digest};

#[derive(Debug, Clone)]
pub struct MemberReadTicket {
    lease: CoordinatorLease,
    source_epoch: i64,
    previous: Option<veoveo_mcp_knowledge_extension::Observation>,
    title: Option<veoveo_knowledge_contract::MemberTitle>,
    freshness: veoveo_mcp_knowledge_extension::Freshness,
    tenant: TenantId,
    generation: GenerationId,
    collection: CollectionId,
    approval: Sha256Digest,
    specification: Sha256Digest,
    uri: ResourceUri,
    epoch: i64,
}
impl MemberReadTicket {
    pub fn previous(&self) -> Option<&veoveo_mcp_knowledge_extension::Observation> {
        self.previous.as_ref()
    }
    pub fn title(&self) -> Option<&veoveo_knowledge_contract::MemberTitle> {
        self.title.as_ref()
    }

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
        let _mutation = ticket.lease.mutation().await;
        let sql = include_str!("../queries/knowledge/delete_member.surql");
        ticket
            .lease
            .bind(self.client().query(sql))
            .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
            .bind(("source_epoch", ticket.source_epoch))
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("chunk_table", chunk_table(ticket.generation)))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    /// Preserve existing vectors only when the source revalidates every observation field.
    pub async fn revalidate_knowledge_member(
        &self,
        ticket: &MemberReadTicket,
        observation: &veoveo_mcp_knowledge_extension::Observation,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
        let previous = ticket.previous.as_ref().ok_or(StoreError::Knowledge(
            "conditional member has no cached observation",
        ))?;
        let current = observation
            .revalidated(previous)
            .map_err(|error| StoreError::Knowledge(error.0))?;
        ticket
            .lease
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/revalidate.surql")),
            )
            .bind(("chunk_table", chunk_table(ticket.generation)))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
            .bind(("source_epoch", ticket.source_epoch))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("previous", Document(previous.clone())))
            .bind(("observation", Document(current.clone())))
            .bind(("valid_until", valid_until(&current, ticket.freshness)?))
            .await?
            .knowledge_check()?;
        Ok(())
    }

    /// Reserve before the source read. Calling this for an invalidation immediately
    /// hides old chunks and fences every earlier in-flight read of this member.
    pub async fn begin_knowledge_member_read(
        &self,
        lease: &CoordinatorLease,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
        uri: &ResourceUri,
    ) -> Result<MemberReadTicket, StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(&registration.tenant)?;
        let collection = registration.descriptor.collection();
        let mut response = lease
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/begin_read.surql")),
            )
            .bind(("sync", sync_record(generation, collection)))
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
        #[derive(SurrealValue)]
        struct Prior {
            epoch: i64,
            source_epoch: i64,
            observation: Option<Document<veoveo_mcp_knowledge_extension::Observation>>,
            title: Option<String>,
        }
        let prior: Option<Prior> = response.take(index)?;
        let prior = prior.ok_or(StoreError::Knowledge("member read was not reserved"))?;
        Ok(MemberReadTicket {
            lease: lease.clone(),
            freshness: registration.descriptor.freshness(),
            source_epoch: prior.source_epoch,
            previous: prior.observation.map(|d| d.0),
            title: prior
                .title
                .map(veoveo_knowledge_contract::MemberTitle::new)
                .transpose()
                .map_err(|_| StoreError::Knowledge("invalid member title"))?,
            tenant: registration.tenant.clone(),
            generation,
            collection: collection.clone(),
            approval: registration.revision(),
            specification: specification.revision(),
            uri: uri.clone(),
            epoch: prior.epoch,
        })
    }

    pub async fn replace_knowledge_member(
        &self,
        ticket: &MemberReadTicket,
        member: &IndexedMember,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
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
            row.insert(
                "admission",
                Admission::from(member.observation().access()).into_value(),
            );
            row.insert("text", chunk.text().to_owned().into_value());
            row.insert("title", member.title().as_str().to_owned().into_value());
            row.insert("embedding", chunk.vector().values().to_vec().into_value());
            row.insert(
                "observation",
                Document(member.observation().clone()).into_value(),
            );
            chunks.push(Value::Object(row));
        }
        ticket
            .lease
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/replace.surql")),
            )
            .bind(("chunk_target", surrealdb::types::Table::new(table)))
            .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
            .bind(("source_epoch", ticket.source_epoch))
            .bind(("title", member.title().as_str().to_owned()))
            .bind((
                "valid_until",
                valid_until(member.observation(), ticket.freshness)?,
            ))
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind(("specification", ticket.specification.to_string()))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("chunk_table", chunk_table(ticket.generation)))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .bind(("chunks", chunks))
            .bind(("observation", Document(member.observation().clone())))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}

fn valid_until(
    observation: &veoveo_mcp_knowledge_extension::Observation,
    freshness: veoveo_mcp_knowledge_extension::Freshness,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, StoreError> {
    match freshness {
        veoveo_mcp_knowledge_extension::Freshness::Immutable { .. } => Ok(None),
        veoveo_mcp_knowledge_extension::Freshness::MaxAge { max_age_seconds } => observation
            .observed_at()
            .checked_add_signed(chrono::TimeDelta::seconds(i64::from(max_age_seconds)))
            .map(Some)
            .ok_or(StoreError::Knowledge("invalid source freshness deadline")),
    }
}
