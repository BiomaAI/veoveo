use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};
use veoveo_embedding_contract::*;
use veoveo_knowledge_mcp::{
    ServiceError, access::SearchCaller, contract::*, embed::Embeddings, source::*,
};
use veoveo_mcp_knowledge_extension::{self as extension, *};
use veoveo_types::*;
pub(crate) struct SyntheticEmbeddings {
    pub(crate) space: EmbeddingSpace,
    pub(crate) inputs: Mutex<Vec<String>>,
}
impl SyntheticEmbeddings {
    pub(crate) fn new() -> Self {
        Self {
            space: EmbeddingSpace {
                model: EmbeddingModelId::new("synthetic-fixture").unwrap(),
                revision: EmbeddingModelRevision::new("fixture-1").unwrap(),
                dimension: EmbeddingDimension::new(3).unwrap(),
                runtime_image: Sha256Digest::from_bytes([1; 32]),
            },
            inputs: Mutex::new(vec![]),
        }
    }
}
impl Embeddings for SyntheticEmbeddings {
    fn space(&self) -> &EmbeddingSpace {
        &self.space
    }
    async fn documents(&self, batch: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        let mut inputs = self.inputs.lock().unwrap();
        Ok(batch
            .texts()
            .iter()
            .map(|t| {
                inputs.push(t.as_str().to_owned());
                EmbeddingVector::new(self.space.clone(), vec![1.0, 0.0, 0.0]).unwrap()
            })
            .collect())
    }
    async fn queries(
        &self,
        _: EmbeddingTask,
        texts: EmbeddingBatch,
    ) -> Result<Vec<EmbeddingVector>, ServiceError> {
        self.documents(texts).await
    }
    async fn query(
        &self,
        _: EmbeddingTask,
        _: EmbeddingText,
    ) -> Result<EmbeddingVector, ServiceError> {
        Ok(EmbeddingVector::new(self.space.clone(), vec![1.0, 0.0, 0.0]).unwrap())
    }
}
#[derive(Clone)]
pub(crate) struct Record {
    pub(crate) registration: CollectionRegistration,
    pub(crate) title: MemberTitle,
    pub(crate) text: String,
    pub(crate) access: AccessDescriptor,
    pub(crate) revision: u32,
    pub(crate) fail: bool,
}
pub(crate) struct Source(pub(crate) Mutex<BTreeMap<ResourceUri, Record>>);
impl KnowledgeSource for Source {
    async fn enumerate(
        &self,
        _collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> Result<SourcePage, ServiceError> {
        let parts = ResourceUriParts::parse(uri.as_str()).unwrap();
        let after = parts
            .query_parameters()
            .get("cursor")
            .map(|s| s.parse::<usize>().unwrap())
            .unwrap_or_default();
        let records = self.0.lock().unwrap();
        let matches: Vec<_> = records
            .iter()
            .filter(|(_, record)| {
                record.registration.descriptor.collection().name().as_str() == parts.authority()
            })
            .collect();
        let items = matches
            .iter()
            .skip(after)
            .take(100)
            .map(|(uri, record)| MemberLink {
                uri: (*uri).clone(),
                title: Some(record.title.clone()),
            })
            .collect();
        Ok(SourcePage::new(
            items,
            (matches.len() > after + 100).then(|| (after + 100).to_string()),
        )?)
    }
    async fn read(
        &self,
        _collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> Result<SourceDocument, ServiceError> {
        let records = self.0.lock().unwrap();
        let record = records.get(&uri).ok_or(ServiceError::SourceUnavailable)?;
        if record.fail {
            return Err(ServiceError::SourceUnavailable);
        }
        let observation = Observation::builder(
            record.registration.descriptor.collection().clone(),
            Revision::new(record.revision.to_string())?,
            extension::content_digest(&record.text),
            Utc::now(),
        )
        .access(record.access.clone())
        .build(&record.registration.descriptor)?;
        Ok(SourceDocument::new(record.text.clone(), observation)?)
    }
}
pub(crate) fn registration(name: &str, indexing: IndexingMode) -> CollectionRegistration {
    CollectionRegistration {
        source_contract_revision: 3,
        tenant: "knowledge-native".parse().unwrap(),
        descriptor: CollectionDescriptor::new(
            CollectionId::new("fixture".parse().unwrap(), name.parse().unwrap()).unwrap(),
            "finding".parse().unwrap(),
            ResourceTemplateUri::new(format!("fixture://{name}{{?cursor}}")).unwrap(),
            Freshness::max_age(300),
            ChangeSignal::Listen,
            AccessModel::WorkContext,
            indexing,
        )
        .unwrap()
        .with_required_scopes([ScopeName::new("fixture:read").unwrap()]),
        approval: KnowledgeCollectionApproval {
            collection: CollectionId::new("fixture".parse().unwrap(), name.parse().unwrap())
                .unwrap(),
            mode: CollectionApproval::Index,
            stewards: ["stewards".parse().unwrap()].into(),
            authoritative_for: Default::default(),
            data_labels: ["secret".parse().unwrap(), "restricted".parse().unwrap()].into(),
        },
        control_revision: Sha256Digest::from_bytes([2; 32]),
    }
}
pub(crate) fn uri(id: &str) -> ResourceUri {
    ResourceUriBuilder::new("fixture://members")
        .unwrap()
        .segment(UriSegment::new(id).unwrap())
        .build()
        .unwrap()
}
pub(crate) fn record(registration: &CollectionRegistration, text: &str) -> Record {
    Record {
        registration: registration.clone(),
        title: MemberTitle::new("Facility inspection").unwrap(),
        text: text.into(),
        revision: 1,
        fail: false,
        access: AccessDescriptor {
            tenant: registration.tenant.clone(),
            work_context: "operations".parse().unwrap(),
            read_policy: ReadPolicy::SelectedWorkContextMembers {},
            owner: AccessSubject::Principal("author".parse().unwrap()),
            grants: vec![],
            data_labels: vec![],
            expires_at: None,
        },
    }
}
pub(crate) fn caller(registrations: &[CollectionRegistration]) -> SearchCaller {
    SearchCaller {
        principal: "reader".parse().unwrap(),
        tenant: registrations[0].tenant.clone(),
        profile: "operations".parse().unwrap(),
        active_work_context: "operations".parse().unwrap(),
        collections: registrations
            .iter()
            .map(|r| (r.descriptor.collection().clone(), selection()))
            .collect(),
        work_contexts: BTreeSet::from(["operations".parse().unwrap()]),
        memberships: BTreeSet::new(),
        scopes: BTreeSet::from(["fixture:read".parse().unwrap()]),
        clearance: BTreeSet::new(),
    }
}

pub(crate) fn selection() -> veoveo_types::ResourceSelection {
    veoveo_types::ResourceSelection {
        scheme: "fixture".parse().unwrap(),
        selectors: vec![veoveo_types::ResourceSelector::Scheme {
            scheme: "fixture".parse().unwrap(),
        }],
    }
}
