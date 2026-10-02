//! Fictional operational scenarios serialized through each source owner's public
//! contract. This fixture measures retrieval, not the truth of sensor findings.
use super::*;
use veoveo_mcp_knowledge_extension::{AccessDescriptor, AccessModel, ReadPolicy, content_digest};
mod map;
mod outputs;
mod time;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scenario {
    key: String,
    title: String,
    finding: String,
    action: String,
    question: String,
    place: String,
    aliases: Vec<String>,
    facility: String,
    facility_kind: veoveo_map_mcp::contract::FacilityKind,
    semantic_query: String,
    spanish_query: String,
    file_query: String,
    time_query: String,
}
#[derive(Default)]
struct Builder {
    registrations: BTreeMap<CollectionId, CollectionRegistration>,
    members: Vec<Member>,
    cases: Vec<RetrievalCase>,
}
impl Builder {
    fn add(
        &mut self,
        descriptor: CollectionDescriptor,
        uri: ResourceUri,
        title: &str,
        text: String,
        policy: ReadPolicy,
    ) -> Result<EvaluationMemberId> {
        let access = (descriptor.access() == AccessModel::WorkContext).then(|| AccessDescriptor {
            tenant: "retrieval-evaluation".parse().unwrap(),
            work_context: "operations".parse().unwrap(),
            read_policy: policy,
            owner: AccessSubject::Principal("evaluator".parse().unwrap()),
            grants: vec![],
            data_labels: vec![],
            expires_at: None,
        });
        let revision = content_digest(&serde_json::to_string(&(&text, &access))?);
        let mut observation = Observation::builder(
            descriptor.collection().clone(),
            revision.to_string().parse()?,
            content_digest(&text),
            observed_at(),
        )
        .modified_at(observed_at());
        if let Some(access) = access {
            observation = observation.access(access);
        }
        let observation = observation.build(&descriptor)?;
        let id = EvaluationMemberId {
            collection: descriptor.collection().clone(),
            uri: uri.clone(),
        };
        self.registrations
            .entry(id.collection.clone())
            .or_insert_with(|| CollectionRegistration {
                tenant: "retrieval-evaluation".parse().unwrap(),
                source_contract_revision: 3,
                control_revision: content_digest("fictional-domain-retrieval-v1"),
                approval: KnowledgeCollectionApproval {
                    collection: id.collection.clone(),
                    mode: CollectionApproval::Index,
                    stewards: ["evaluators".parse().unwrap()].into(),
                    authoritative_for: Default::default(),
                    data_labels: Default::default(),
                },
                descriptor,
            });
        self.members.push(Member {
            link: MemberLink {
                uri,
                title: Some(MemberTitle::new(title)?),
            },
            text,
            observation,
        });
        Ok(id)
    }
    fn query(
        &mut self,
        id: &str,
        query: &str,
        relevant: impl IntoIterator<Item = EvaluationMemberId>,
    ) -> Result<()> {
        self.cases.push(RetrievalCase::new(
            EvaluationCaseId::new(id)?,
            EmbeddingText::new(query)?,
            Default::default(),
            relevant.into_iter().collect(),
        )?);
        Ok(())
    }
}
fn observed_at() -> chrono::DateTime<Utc> {
    "2026-10-01T00:00:00Z".parse().unwrap()
}

pub(super) fn build() -> Result<Corpus> {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("../../../evaluation/scenarios.json"))?;
    ensure!(
        scenarios.len() == 12
            && scenarios
                .iter()
                .map(|s| &s.key)
                .collect::<BTreeSet<_>>()
                .len()
                == 12,
        "expected twelve distinct scenarios"
    );
    let mut builder = Builder::default();
    time::authorities(&mut builder)?;
    for (index, scenario) in scenarios.iter().enumerate() {
        let mapped = map::add(&mut builder, scenario)?;
        let (artifact, analysis, result) = outputs::add(&mut builder, scenario, index)?;
        let scheduled = time::add(&mut builder, scenario, index)?;
        let relevant = vec![
            mapped.layer.clone(),
            mapped.feature.clone(),
            result.clone(),
            artifact.clone(),
        ];
        builder.query(
            &format!("{}-semantic", scenario.key),
            &scenario.semantic_query,
            relevant.clone(),
        )?;
        builder.query(
            &format!("{}-spanish", scenario.key),
            &scenario.spanish_query,
            relevant,
        )?;
        builder.query(
            &format!("{}-file", scenario.key),
            &scenario.file_query,
            [artifact],
        )?;
        builder.query(
            &format!("{}-event", scenario.key),
            &scenario.time_query,
            [scheduled.event],
        )?;
        // These cases target the source's domain role rather than its title alone.
        if index < 3 {
            builder.query(
                &format!("{}-analysis", scenario.key),
                &format!("Which completed analysis asked: {}", scenario.question),
                [analysis],
            )?;
            builder.query(
                &format!("{}-calendar", scenario.key),
                &format!(
                    "Find the recurring local operating hours and excluded dates for work on {}.",
                    scenario.title
                ),
                [scheduled.calendar],
            )?;
            builder.query(
                &format!("{}-epoch", scenario.key),
                &format!(
                    "Which mission reference instant anchors elapsed time for {}?",
                    scenario.title
                ),
                [scheduled.epoch],
            )?;
            builder.query(
                &format!("{}-publication", scenario.key),
                &format!(
                    "Find the published revision of the reviewed feature layer for {}.",
                    scenario.title
                ),
                [mapped.publication],
            )?;
            builder.query(
                &format!("{}-facility", scenario.key),
                &format!(
                    "Which facility record gives the location and transport function of {}?",
                    scenario.facility
                ),
                [mapped.facility],
            )?;
            builder.query(
                &format!("{}-location", scenario.key),
                &format!(
                    "Find the geographic position and alternate names for {}.",
                    scenario.place
                ),
                [mapped.location],
            )?;
            builder.query(&format!("{}-release", scenario.key), &format!("Which active geographic dataset release covers {} and records its attribution?", scenario.place), [mapped.release])?;
        }
    }
    for (server, text, query) in [
        (
            "map",
            include_str!("../../../../map-mcp/DESIGN.md"),
            "Which service explains Earth geography, coordinate conversion and travel models used for logistics routing?",
        ),
        (
            "artifact",
            include_str!("../../../../artifact-mcp/DESIGN.md"),
            "Where is the policy for discovering stored files, reading metadata and delivering byte ranges described?",
        ),
        (
            "time",
            include_str!("../../../../time-mcp/DESIGN.md"),
            "Which service explains interpreting instants with leap seconds and time zones and expanding operational calendars?",
        ),
        (
            "reason",
            include_str!("../../../../reason-mcp/DESIGN.md"),
            "Where is the contract for answering questions about recorded video with model provenance and grounded output described?",
        ),
        (
            "charts",
            include_str!("../../../../chart-mcp/DESIGN.md"),
            "Which service creates chart artifacts from data and exposes an interactive chart editing app?",
        ),
    ] {
        let scheme: ResourceScheme = server.parse()?;
        let descriptor =
            veoveo_mcp_knowledge_extension::docs::collection(&server.parse()?, &scheme);
        let uri = veoveo_mcp_knowledge_extension::docs::member_uri(&scheme, &"design".parse()?);
        let id = builder.add(
            descriptor,
            uri,
            &format!("{server} design"),
            text.into(),
            ReadPolicy::Tenant {},
        )?;
        builder.query(&format!("{server}-design"), query, [id])?;
    }
    let registrations: Vec<_> = builder.registrations.into_values().collect();
    let mut scopes = BTreeSet::new();
    let mut collections = BTreeMap::new();
    for r in &registrations {
        scopes.extend(r.descriptor.required_scopes().iter().cloned());
        let root = veoveo_knowledge_mcp::source::enumeration_uri(&r.descriptor, None)?;
        let scheme: ResourceScheme = ResourceUriParts::parse(root.as_str())?.scheme().parse()?;
        collections.insert(
            r.descriptor.collection().clone(),
            ResourceSelection {
                scheme: scheme.clone(),
                selectors: vec![ResourceSelector::Scheme { scheme }],
            },
        );
    }
    let corpus = Corpus {
        registrations,
        members: builder.members,
        cases: builder.cases,
        audience: Audience {
            tenant: "retrieval-evaluation".parse()?,
            principal: "evaluator".parse()?,
            profile: "evaluation".parse()?,
            active_work_context: "operations".parse()?,
            work_contexts: ["operations".parse()?].into(),
            memberships: Default::default(),
            scopes,
            clearance: Default::default(),
            collections,
        },
    };
    corpus.validate()?;
    Ok(corpus)
}

#[test]
fn domain_models_produce_nineteen_judged_collections_without_runtime_dependencies() {
    let corpus = build().unwrap();
    assert_eq!(corpus.registrations.len(), 19);
    assert_eq!(corpus.members.len(), 153);
    assert_eq!(corpus.cases.len(), 78);
    assert_eq!(corpus.revision(), build().unwrap().revision());
    for member in &corpus.members {
        assert!(member.text.len() <= 64 * 1024);
    }
}
