use super::*;
use rmcp::{model::*, service::Subscription};

fn completion(template: &str, argument: &str, value: &str) -> CompleteRequestParams {
    serde_json::from_value(serde_json::json!({"ref":{"type":"ref/resource","uri":template},"argument":{"name":argument,"value":value}})).unwrap()
}

async fn update(subscription: &mut Subscription, uri: &str) {
    let notification = tokio::time::timeout(Duration::from_secs(10), subscription.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ServerNotification::ResourceUpdatedNotification(notification) = notification else {
        panic!("expected resource update");
    };
    assert_eq!(notification.params.uri, uri);
}

async fn statistics(client: &rmcp::Peer<rmcp::RoleClient>, uri: &str) -> CollectionStatistics {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri.to_owned()))
        .await
        .unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &result.contents[0] else {
        panic!("expected JSON")
    };
    serde_json::from_str::<CollectionCatalogEntry>(text)
        .unwrap()
        .statistics
        .unwrap()
}

#[tokio::test]
async fn catalog_completion_and_live_statistics_preserve_caller_visibility() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let content = collection("records");
        let mut hidden_collection = collection("private");
        hidden_collection.descriptor = hidden_collection.descriptor.with_required_scopes(["media:private".parse().unwrap()]);
        let registrations = vec![content.clone(), hidden_collection.clone()];
        let plane = plane(&registrations);
        install(&db.a, &plane).await;
        for r in &registrations {db.a.register_knowledge_collection(r, None).await.unwrap();}
        let identity = identity(&plane);
        directory(&db.a, &identity).await;
        let signing = Signing::new();
        let embeddings = Arc::new(SyntheticEmbeddings::new());
        let lease = db.a.claim_knowledge_coordinator(&content.tenant, veoveo_platform_store::knowledge::CoordinatorId::new()).await.unwrap().unwrap();
        let mut hidden = record(&content, "private investigation");
        hidden.access.read_policy = ReadPolicy::Subjects {};
        let source = Source(Mutex::new(BTreeMap::from([
            (ResourceUri::new("media://members/visible").unwrap(), record(&content, "public facility inspection")),
            (ResourceUri::new("media://members/hidden").unwrap(), hidden),
        ])));
        let spec = GenerationSpec::new(embeddings.space().clone(), "Find passages", ChunkSettings::new("structure-v1", 500, 0).unwrap(),
            registrations.iter().map(|r| (r.descriptor.collection().clone(), r.revision())).collect()).unwrap();
        let generation = Indexer {lease: &lease, store: &db.a, source: &source, embeddings: embeddings.as_ref()}
            .build(&content.tenant, &registrations, &spec).await.unwrap();
        db.a.activate_knowledge_generation(&lease, &content.tenant, generation, None).await.unwrap();
        // Neither completion nor statistics may decode an excluded document.
        db.a.client().query("UPDATE knowledge_collection SET document.sourceContractRevision = 'malformed' WHERE collection = 'media.private';").await.unwrap().check().unwrap();
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client().query(format!("UPDATE {table} SET observation.observedAt = 'malformed' WHERE uri = 'media://members/hidden';"))
            .await.unwrap().check().unwrap();
        let server = Server::new(db.b.clone(), embeddings.clone(), &signing).await;
        let replica = Server::new(db.a.clone(), embeddings, &signing).await;
        let mut client = server.sdk(signing.issue(identity.clone()).bearer_token).await;
        let mut other = replica.sdk(signing.issue(identity.clone()).bearer_token).await;
        let values = client.complete(completion("knowledge://collection/{collection}", "collection", "media.")).await.unwrap();
        assert_eq!(values.completion.values, ["media.records"]);
        assert_eq!(client.complete(completion("knowledge://source/{server}", "server", "me")).await.unwrap().completion.values, ["media"]);
        assert!(client.complete(completion("knowledge://collection/{collection}", "collection", "private")).await.unwrap().completion.values.is_empty());
        assert!(client.complete(completion("knowledge://docs/{doc_id}", "doc_id", "des")).await.unwrap().completion.values.contains(&"design".into()));
        assert!(client.complete(completion("knowledge://collection/{collection}", "forged", "")).await.is_err());
        let uri = KnowledgeResource::Collection(content.descriptor.collection().clone()).to_uri().unwrap().to_string();
        let initial = statistics(client.peer(), &uri).await;
        assert_eq!(initial.indexed_members(), 1);
        assert_eq!(initial.indexed_chunks(), 1);
        assert!(initial.last_observed_at().is_some());
        assert_eq!(initial.last_modified_at(), None);
        let filter = SubscriptionFilter::builder().resource_subscription(uri.clone()).build();
        let mut first = client.listen(filter.clone()).await.unwrap();
        let mut second = other.listen(filter).await.unwrap();
        update(&mut first, &uri).await; update(&mut second, &uri).await;
        let mut inventory = client.listen(SubscriptionFilter::builder().resources_list_changed().build()).await.unwrap();
        assert!(matches!(tokio::time::timeout(Duration::from_secs(10), inventory.next()).await.unwrap().unwrap(),
            Some(ServerNotification::ResourceListChangedNotification(_))));
        db.a.client().query("UPDATE knowledge_member SET title = 'hidden title changed' WHERE uri = 'media://members/hidden';").await.unwrap().check().unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(500), first.next()).await.is_err(), "hidden activity must not invalidate readable statistics");
        db.a.client().query("UPDATE knowledge_sync SET ready = false WHERE collection.collection = 'media.records';")
            .await.unwrap().check().unwrap();
        update(&mut first, &uri).await; update(&mut second, &uri).await;
        assert_eq!(statistics(client.peer(), &uri).await.indexed_members(), 0);
        db.a.client().query("UPDATE knowledge_sync SET ready = true WHERE collection.collection = 'media.records';").await.unwrap().check().unwrap();
        update(&mut first, &uri).await; update(&mut second, &uri).await;
        assert_eq!(statistics(other.peer(), &uri).await, initial);
        assert!(tokio::time::timeout(Duration::from_millis(300), inventory.next()).await.is_err(), "member updates do not change static discovery");
        inventory.cancel().await.unwrap();
        // One real lease deadline must invalidate results without timer polling.
        db.a.client().query("UPDATE knowledge_coordinator SET expires_at = time::now() + 2s;").await.unwrap().check().unwrap();
        update(&mut first, &uri).await; update(&mut second, &uri).await;
        assert_eq!(statistics(client.peer(), &uri).await, CollectionStatistics::empty());
        revoke(&db.a, &identity).await;
        assert!(tokio::time::timeout(Duration::from_secs(10), first.next()).await.unwrap().is_err());
        let _ = second.cancel().await;
        client.close().await.unwrap(); other.close().await.unwrap();
    }).await.expect("catalog qualification exceeded 180 seconds");
}
