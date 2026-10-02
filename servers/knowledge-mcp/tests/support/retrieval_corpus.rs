//! Immutable source captures for model comparison, separate from live source
//! conformance. Re-observation updates only observedAt within this owned fixture.
use super::*;
use chrono::Utc;
#[path = "retrieval_domains/mod.rs"]
mod domains;
pub(super) fn domain_corpus() -> Result<Corpus> {
    domains::build()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Corpus {
    pub registrations: Vec<CollectionRegistration>,
    pub members: Vec<Member>,
    pub cases: Vec<RetrievalCase>,
    pub audience: Audience,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Member {
    link: MemberLink,
    text: String,
    observation: Observation,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Audience {
    tenant: TenantId,
    principal: PrincipalId,
    profile: GatewayProfileId,
    collections: BTreeMap<CollectionId, ResourceSelection>,
    active_work_context: WorkContextId,
    work_contexts: BTreeSet<WorkContextId>,
    memberships: BTreeSet<veoveo_mcp_contract::access::GroupMembership>,
    scopes: BTreeSet<ScopeName>,
    clearance: BTreeSet<DataLabelId>,
}
pub(super) struct Source<'a>(BTreeMap<EvaluationMemberId, &'a Member>);

impl Corpus {
    pub fn revision(&self) -> Sha256Digest {
        use sha2::Digest;
        Sha256Digest::from_bytes(
            sha2::Sha256::digest(serde_json::to_vec(self).expect("typed source corpus")).into(),
        )
    }
    pub fn validate(&self) -> Result<Source<'_>> {
        ensure!(
            !self.registrations.is_empty() && self.registrations.len() <= 1024,
            "invalid source registration count"
        );
        let registrations: BTreeMap<_, _> = self
            .registrations
            .iter()
            .map(|r| (r.descriptor.collection(), r))
            .collect();
        ensure!(
            registrations.len() == self.registrations.len(),
            "duplicate source registration"
        );
        for registration in &self.registrations {
            registration.validate()?;
            ensure!(
                registration.tenant == self.audience.tenant
                    && registration.approval.mode == CollectionApproval::Index,
                "source registration must be indexable in the evaluation tenant"
            );
        }
        let dataset = self.dataset()?;
        let judged_collections: BTreeSet<_> = dataset
            .cases()
            .iter()
            .flat_map(|case| case.relevant().iter().map(|m| m.collection.clone()))
            .collect();
        ensure!(
            judged_collections == dataset.collections(),
            "each benchmark collection needs a relevance judgment"
        );
        let mut members = BTreeMap::new();
        for member in &self.members {
            let registration = registrations
                .get(member.observation.collection())
                .context("member has no registered collection")?;
            registration.admit_observation(&member.observation)?;
            SourceDocument::new(member.text.clone(), member.observation.clone())?;
            members.insert(
                EvaluationMemberId {
                    collection: member.observation.collection().clone(),
                    uri: member.link.uri.clone(),
                },
                member,
            );
        }
        ensure!(
            dataset.collections().len() == registrations.len(),
            "each benchmark collection must contain source members"
        );
        Ok(Source(members))
    }
    pub fn dataset(&self) -> Result<RetrievalDataset> {
        Ok(RetrievalDataset::new(
            self.members
                .iter()
                .map(|m| EvaluationMember {
                    member: EvaluationMemberId {
                        collection: m.observation.collection().clone(),
                        uri: m.link.uri.clone(),
                    },
                    revision: m.observation.revision().clone(),
                    content_sha256: m.observation.content_sha256().clone(),
                })
                .collect(),
            self.cases.clone(),
        )?)
    }
    pub fn caller(&self) -> Result<SearchCaller> {
        ensure!(
            self.registrations.iter().all(|r| self
                .audience
                .collections
                .contains_key(r.descriptor.collection())),
            "audience must select every benchmark collection"
        );
        Ok(SearchCaller {
            tenant: self.audience.tenant.clone(),
            principal: self.audience.principal.clone(),
            profile: self.audience.profile.clone(),
            active_work_context: self.audience.active_work_context.clone(),
            work_contexts: self.audience.work_contexts.clone(),
            memberships: self.audience.memberships.clone(),
            scopes: self.audience.scopes.clone(),
            clearance: self.audience.clearance.clone(),
            collections: self.audience.collections.clone(),
        })
    }
}
impl KnowledgeSource for Source<'_> {
    async fn enumerate(
        &self,
        descriptor: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> Result<SourcePage, ServiceError> {
        let parts = ResourceUriParts::parse(uri.as_str())
            .map_err(|_| KnowledgeError("invalid corpus page URI"))?;
        let after = parts
            .query_parameters()
            .get("cursor")
            .map(|v| v.parse::<usize>())
            .transpose()
            .map_err(|_| KnowledgeError("invalid corpus cursor"))?
            .unwrap_or_default();
        let matches: Vec<_> = self
            .0
            .iter()
            .filter(|(id, _)| &id.collection == descriptor.collection())
            .map(|(_, member)| *member)
            .collect();
        if after > matches.len() {
            return Err(KnowledgeError("corpus cursor exceeds collection").into());
        }
        Ok(SourcePage::new(
            matches
                .iter()
                .skip(after)
                .take(100)
                .map(|member| member.link.clone())
                .collect(),
            (matches.len() > after + 100).then(|| (after + 100).to_string()),
        )?)
    }
    async fn read(
        &self,
        descriptor: &CollectionDescriptor,
        uri: ResourceUri,
        _: Option<&Observation>,
    ) -> Result<SourceRead, ServiceError> {
        let member = self
            .0
            .get(&EvaluationMemberId {
                collection: descriptor.collection().clone(),
                uri,
            })
            .ok_or(ServiceError::SourceUnavailable)?;
        let previous = &member.observation;
        let mut builder = Observation::builder(
            previous.collection().clone(),
            previous.revision().clone(),
            previous.content_sha256().clone(),
            Utc::now(),
        );
        if let Some(access) = previous.access() {
            builder = builder.access(access.clone());
        }
        if let Some(modified) = previous.modified_at() {
            builder = builder.modified_at(modified);
        }
        if let Some(actor) = previous.modified_by() {
            builder = builder.modified_by(actor.clone());
        }
        if let Some(external) = previous.external() {
            builder = builder.external(external.clone());
        }
        Ok(SourceRead::Modified(SourceDocument::new(
            member.text.clone(),
            builder.build(descriptor)?,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_knowledge_extension::{
        AccessModel, ChangeSignal, Freshness, IndexingMode, Revision, content_digest,
    };

    #[tokio::test]
    async fn corpus_checks_source_bytes_and_preserves_revision_during_reobservation() {
        let collection: CollectionId = "fixture.findings".parse().unwrap();
        let descriptor = CollectionDescriptor::new(
            collection.clone(),
            "finding".parse().unwrap(),
            ResourceTemplateUri::new("fixture://index{?cursor}").unwrap(),
            Freshness::immutable(),
            ChangeSignal::Immutable,
            AccessModel::Profile,
            IndexingMode::Content,
        )
        .unwrap();
        let registration = CollectionRegistration {
            tenant: "fixture".parse().unwrap(),
            descriptor,
            source_contract_revision: 3,
            control_revision: Sha256Digest::from_bytes([1; 32]),
            approval: KnowledgeCollectionApproval {
                collection: collection.clone(),
                mode: CollectionApproval::Index,
                stewards: ["reviewers".parse().unwrap()].into(),
                authoritative_for: Default::default(),
                data_labels: Default::default(),
            },
        };
        let link = MemberLink {
            uri: ResourceUri::new("fixture://finding/one").unwrap(),
            title: Some(MemberTitle::new("Inspection").unwrap()),
        };
        let observed_at = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let text = "The bridge deck requires inspection before reopening.";
        let observation = Observation::builder(
            collection.clone(),
            Revision::new("revision-one").unwrap(),
            content_digest(text),
            observed_at,
        )
        .modified_at(observed_at)
        .build(&registration.descriptor)
        .unwrap();
        let mut corpus = Corpus {
            registrations: vec![registration],
            members: vec![Member {
                link: link.clone(),
                text: text.into(),
                observation: observation.clone(),
            }],
            cases: vec![
                RetrievalCase::new(
                    EvaluationCaseId::new("bridge-inspection").unwrap(),
                    EmbeddingText::new("Which crossing needs a structural check?").unwrap(),
                    Default::default(),
                    [EvaluationMemberId {
                        collection: collection.clone(),
                        uri: link.uri.clone(),
                    }]
                    .into(),
                )
                .unwrap(),
            ],
            audience: Audience {
                tenant: "fixture".parse().unwrap(),
                principal: "reader".parse().unwrap(),
                profile: "operator".parse().unwrap(),
                collections: [(
                    collection.clone(),
                    ResourceSelection {
                        scheme: "fixture".parse().unwrap(),
                        selectors: vec![ResourceSelector::Scheme {
                            scheme: "fixture".parse().unwrap(),
                        }],
                    },
                )]
                .into(),
                active_work_context: "operations".parse().unwrap(),
                work_contexts: ["operations".parse().unwrap()].into(),
                memberships: Default::default(),
                scopes: Default::default(),
                clearance: Default::default(),
            },
        };
        let source = corpus.validate().unwrap();
        let descriptor = &corpus.registrations[0].descriptor;
        let page = source
            .enumerate(descriptor, ResourceUri::new("fixture://index").unwrap())
            .await
            .unwrap();
        assert_eq!(page.items()[0].uri, link.uri);
        let SourceRead::Modified(read) = source.read(descriptor, link.uri, None).await.unwrap()
        else {
            panic!("full rebuild requires content");
        };
        assert_eq!(read.text(), text);
        assert_eq!(read.observation().revision(), observation.revision());
        assert_eq!(
            read.observation().content_sha256(),
            observation.content_sha256()
        );
        assert_eq!(read.observation().modified_at(), observation.modified_at());
        assert!(read.observation().observed_at() > observed_at);
        assert_eq!(corpus.caller().unwrap().collections.len(), 1);
        corpus.members[0].text.push_str("altered");
        assert!(
            corpus.validate().is_err(),
            "capture corruption must fail before embedding"
        );
    }
}
