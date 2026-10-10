use super::*;
use rmcp::model::{RequestId, ResourceUpdatedNotification, ResourceUpdatedNotificationParam};

#[test]
fn delivered_completion_requires_stable_identity_and_payload_but_allows_updated_hints() -> Result<()>
{
    let id = CanonicalTaskId::parse("gateway-opaque-task")?;
    let created = Task::new(
        id.as_str(),
        TaskStatus::Working,
        "2026-10-09T00:00:00Z",
        "2026-10-09T00:00:00Z",
    );
    let output = CallToolResult::structured(serde_json::json!({"answer": 7}));
    let Value::Object(result) = serde_json::to_value(output)? else {
        unreachable!()
    };
    let delivered = DetailedTask::new(created.clone(), TaskPayload::Completed { result });
    let mut current = delivered.clone();
    current.task.ttl_ms = Some(5000);
    current.task.poll_interval_ms = Some(1000);
    current.task.last_updated_at = "2026-10-09T00:00:01Z".into();
    completed(&created, &delivered, &current, &id)?;
    current.task.task_id = "another-task".into();
    assert!(completed(&created, &delivered, &current, &id).is_err());
    current = delivered.clone();
    current.task.created_at = "2026-10-10T00:00:00Z".into();
    assert!(completed(&created, &delivered, &current, &id).is_err());
    current = DetailedTask::new(created.clone(), TaskPayload::Cancelled);
    assert!(completed(&created, &delivered, &current, &id).is_err());
    current = delivered.clone();
    let TaskPayload::Completed { result } = &mut current.payload else {
        unreachable!()
    };
    result.insert("structuredContent".into(), serde_json::json!({"answer": 8}));
    assert!(completed(&created, &delivered, &current, &id).is_err());
    Ok(())
}

#[test]
fn snapshot_requires_acknowledged_filter_subscription_and_resource_identity() -> Result<()> {
    let uri = ResourceUri::new("test://snapshot/selected")?;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri.to_string()])
        .build();
    acknowledged(&filter, &filter)?;
    assert!(
        acknowledged(
            &SubscriptionFilter::builder()
                .task_ids(["other-task"])
                .build(),
            &filter
        )
        .is_err()
    );
    let id = RequestId::Number(7);
    let mut params = ResourceUpdatedNotificationParam::new(uri.as_str());
    let mut notification = ServerNotification::ResourceUpdatedNotification(
        ResourceUpdatedNotification::new(params.clone()),
    );
    notification.get_meta_mut().set_subscription_id(id.clone());
    resource_delivery(&notification, &id, &uri)?;
    assert!(resource_delivery(&notification, &RequestId::Number(8), &uri).is_err());
    assert!(
        resource_delivery(
            &notification,
            &id,
            &ResourceUri::new("test://snapshot/foreign")?
        )
        .is_err()
    );
    params.meta = None;
    assert!(
        resource_delivery(
            &ServerNotification::ResourceUpdatedNotification(ResourceUpdatedNotification::new(
                params
            )),
            &id,
            &uri
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn interrupted_close_retains_original_future_and_failed_close_is_sticky() -> Result<()> {
    let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = polls.clone();
    let mut slot: Option<Closing> = Some(Box::pin(async move {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(30)).await;
        Ok(())
    }));
    let end = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut failed = false;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(1),
            finish(&mut slot, end, &mut failed)
        )
        .await
        .is_err()
    );
    assert!(slot.is_some());
    finish(&mut slot, end, &mut failed).await?;
    assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst), 1);
    slot = Some(Box::pin(async { anyhow::bail!("static close failure") }));
    assert!(finish(&mut slot, end, &mut failed).await.is_err());
    finish(&mut slot, end, &mut failed).await?;
    assert!(failed);
    Ok(())
}

#[tokio::test]
async fn expired_cleanup_deadline_remains_failed_after_original_close_becomes_ready() -> Result<()>
{
    let (ready, waiting) = tokio::sync::oneshot::channel::<()>();
    let mut slot: Option<Closing> = Some(Box::pin(async move {
        waiting.await.context("close readiness sender dropped")?;
        Ok(())
    }));
    let end = tokio::time::Instant::now();
    let mut failed = false;
    assert!(finish(&mut slot, end, &mut failed).await.is_err());
    assert!(slot.is_some());
    assert!(failed);
    ready.send(()).unwrap();
    // The retained original future may finish during a later drain. That cannot
    // erase the expired deadline recorded by Handles::close's final failure gate.
    finish(&mut slot, end, &mut failed).await?;
    assert!(slot.is_none());
    assert!(failed, "late successful close must remain unqualified");
    Ok(())
}

#[test]
fn diagnostic_hash_does_not_expose_error_payload() {
    let error = safe::<(), _>("tasks/get", Err("token=secret provider-body"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("tasks/get failed (diagnostic sha256:"));
    assert!(!error.contains("secret"));
    assert!(!error.contains("provider-body"));
}
