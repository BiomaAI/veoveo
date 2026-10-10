//! Speech fixture and result assertions shared by installed work and native controls.
use super::*;
use rmcp::model::{CallToolResult, TaskPayload};
use veoveo_artifact_contract::{ComplianceMetadata, UploadSha256};
use veoveo_speech_contract::{SpeechResource, TranscriptDocument};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Expected {
    source_sha256: UploadSha256,
    text: String,
    model: String,
    model_revision: String,
    minimum_duration_seconds: f64,
    maximum_duration_seconds: f64,
    compliance: ComplianceMetadata,
}
impl Expected {
    pub fn admit(&self) -> Result<()> {
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 16 * 1024,
            "Speech expectation requires text at most16KiB"
        );
        ensure!(
            !self.model.trim().is_empty()
                && self.model.len() <= 256
                && !self.model_revision.trim().is_empty()
                && self.model_revision.len() <= 256,
            "Speech expectation requires bounded model provenance"
        );
        ensure!(
            self.minimum_duration_seconds.is_finite()
                && self.maximum_duration_seconds.is_finite()
                && self.minimum_duration_seconds > 0.0
                && self.maximum_duration_seconds >= self.minimum_duration_seconds
                && self.maximum_duration_seconds <= 7200.1,
            "Speech expected duration interval is invalid"
        );
        ensure!(
            self.compliance.owner.is_some()
                && self.compliance.work_context.is_some()
                && self.compliance.tenant_id.is_some()
                && self.compliance.provenance.is_some(),
            "Speech expectation requires owner, Work Context, tenant and invocation provenance"
        );
        Ok(())
    }
}
pub(super) fn same(id: &CanonicalTaskId, created: &Task, task: &DetailedTask) -> Result<()> {
    ensure!(
        CanonicalTaskId::parse(&task.task.task_id)? == *id
            && task.task.created_at == created.created_at,
        "Speech Task identity/creation time changed"
    );
    ensure!(
        task.task.status == task.payload.status(),
        "Speech Task payload classification disagrees"
    );
    Observation::admit(&task.task)?;
    Ok(())
}
pub(super) fn unfinished(task: &DetailedTask) -> Result<()> {
    ensure!(
        matches!(
            task.status(),
            TaskStatus::Working | TaskStatus::InputRequired
        ),
        "Speech recording completed before unfinished cancellation proof; workload is unqualified"
    );
    Ok(())
}
pub(super) fn terminal(
    mode: Mode,
    id: &CanonicalTaskId,
    created: &Task,
    delivered: &DetailedTask,
    current: &DetailedTask,
) -> Result<()> {
    same(id, created, delivered)?;
    same(id, created, current)?;
    match mode {
        Mode::Complete | Mode::Recover => ensure!(
            delivered.status() == TaskStatus::Completed
                && current.status() == TaskStatus::Completed
                && same_output(&output(delivered)?, &output(current)?),
            "Speech delivered/current completed result differs"
        ),
        Mode::Cancel => ensure!(
            matches!(delivered.payload, TaskPayload::Cancelled)
                && matches!(current.payload, TaskPayload::Cancelled),
            "Speech cancellation did not settle Cancelled without output"
        ),
    }
    Ok(())
}
fn same_output(left: &TranscriptionOutput, right: &TranscriptionOutput) -> bool {
    left.result_uri == right.result_uri
        && left.source_artifact_uri == right.source_artifact_uri
        && left.duration_seconds == right.duration_seconds
        && left.transcript == right.transcript
        && left.captions == right.captions
}
pub(super) fn output(task: &DetailedTask) -> Result<TranscriptionOutput> {
    let TaskPayload::Completed { result } = &task.payload else {
        anyhow::bail!("Speech expected completed payload");
    };
    let result: CallToolResult = serde_json::from_value(serde_json::Value::Object(result.clone()))?;
    ensure!(
        result.is_error != Some(true),
        "Speech Task returned tool error"
    );
    Ok(serde_json::from_value(
        result
            .structured_content
            .context("Speech Task omitted structured output")?,
    )?)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attribution {
    source_artifact_uri: veoveo_artifact_contract::ArtifactUri,
    source_sha256: UploadSha256,
    model: String,
    model_revision: String,
}
fn metadata(
    request: &TranscribeRequest,
    expected: &Expected,
    output: &TranscriptionOutput,
) -> Result<()> {
    ensure!(
        output.source_artifact_uri == request.artifact_uri
            && (expected.minimum_duration_seconds..=expected.maximum_duration_seconds)
                .contains(&output.duration_seconds),
        "Speech source/duration differs from independent expectation"
    );
    for artifact in [&output.transcript, &output.captions] {
        ensure!(
            artifact.compliance == expected.compliance,
            "Speech output owner/policy provenance differs"
        );
        ensure!(
            artifact.byte_len > 0 && artifact.byte_len <= 512 * 1024,
            "Speech selected output exceeds512KiB"
        );
        let attribution: Attribution = serde_json::from_value(artifact.metadata.clone())?;
        ensure!(
            attribution.source_artifact_uri == request.artifact_uri
                && attribution.source_sha256 == expected.source_sha256
                && attribution.model == expected.model
                && attribution.model_revision == expected.model_revision,
            "Speech output attribution differs"
        );
    }
    Ok(())
}
/// Initial/current snapshot delivery only; this does not prove a later mutation.
pub(super) fn resource_delivery(
    notification: &ServerNotification,
    subscription: &rmcp::model::RequestId,
    expected: &veoveo_speech_contract::TranscriptionUri,
) -> Result<()> {
    ensure!(
        notification.get_meta().subscription_id().as_ref() == Some(subscription),
        "Speech resource delivery subscription identity differs"
    );
    let ServerNotification::ResourceUpdatedNotification(update) = notification else {
        anyhow::bail!("Speech resource filter delivered a different notification");
    };
    ensure!(
        veoveo_speech_contract::TranscriptionUri::parse(&update.params.uri)? == *expected,
        "Speech resource delivery identity differs"
    );
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct View {
    task_id: veoveo_speech_contract::TranscriptionId,
    status: veoveo_platform_store::TaskStatus,
    message: Option<String>,
    output: Option<TranscriptionOutput>,
}
fn current_output(view: &View, output: &TranscriptionOutput) -> Result<()> {
    ensure!(
        view.task_id == output.result_uri.id()
            && view.status == veoveo_platform_store::TaskStatus::Succeeded
            && view
                .output
                .as_ref()
                .is_some_and(|current| same_output(current, output)),
        "Speech owning Task resource/current output differs"
    );
    let _ = &view.message;
    Ok(())
}
pub(super) async fn check_output(
    caller: &SmokeMcpClient,
    input: &Input,
    output: &TranscriptionOutput,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
) -> Result<()> {
    let request = &input.request;
    let expected = &input.expected;
    metadata(request, expected, output)?;
    // The domain URI exposes its own typed identity; never decode the opaque Gateway ID as UUID.
    let uri = output.result_uri.to_uri();
    journal.active_resource = Some(uri.clone());
    journal.persist(file)?;
    let view: View = serde_json::from_str(&text(caller, &uri, "application/json").await?)?;
    current_output(&view, output)?;
    let uri = SpeechResource::Artifact(output.transcript.artifact_id()).to_uri();
    journal.active_resource = Some(uri.clone());
    journal.persist(file)?;
    let body = text(caller, &uri, "application/json").await?;
    let document: TranscriptDocument = serde_json::from_str(&body)?;
    ensure!(
        body.len() as u64 == output.transcript.byte_len,
        "Speech transcript byte length differs"
    );
    ensure!(
        document.source_artifact_uri == request.artifact_uri
            && document.source_sha256 == expected.source_sha256
            && document.model == expected.model
            && document.model_revision == expected.model_revision
            && document.transcript.text == expected.text,
        "Speech transcript differs from independent source/text/model expectation"
    );
    ensure!(
        (document.transcript.duration_seconds - output.duration_seconds).abs() <= 0.001,
        "Speech document/output duration differs"
    );
    let uri = SpeechResource::Artifact(output.captions.artifact_id()).to_uri();
    journal.active_resource = Some(uri.clone());
    journal.persist(file)?;
    let captions = text(caller, &uri, "text/vtt").await?;
    ensure!(
        captions.len() as u64 == output.captions.byte_len
            && captions == document.transcript.webvtt()?,
        "Speech captions differ from the admitted transcript"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, payload: TaskPayload) -> DetailedTask {
        DetailedTask::new(
            Task::new(
                id,
                payload.status(),
                "2026-10-09T00:00:00Z",
                "2026-10-09T00:00:01Z",
            ),
            payload,
        )
    }
    #[test]
    fn terminal_and_unfinished_checks_reject_false_cancellation_and_changed_identity() -> Result<()>
    {
        let id = CanonicalTaskId::parse("speech.routed-fixture")?;
        let cancelled = task(id.as_str(), TaskPayload::Cancelled);
        let working = task(id.as_str(), TaskPayload::Working);
        unfinished(&working)?;
        ensure!(unfinished(&cancelled).is_err());
        terminal(Mode::Cancel, &id, &working.task, &cancelled, &cancelled)?;
        ensure!(terminal(Mode::Cancel, &id, &working.task, &working, &cancelled).is_err());
        let other = task("speech.other-fixture", TaskPayload::Cancelled);
        ensure!(terminal(Mode::Cancel, &id, &working.task, &other, &cancelled).is_err());
        let mut changed = cancelled.clone();
        changed.task.created_at = "2026-10-09T00:00:02Z".into();
        ensure!(terminal(Mode::Cancel, &id, &working.task, &changed, &cancelled).is_err());
        Ok(())
    }
    #[test]
    fn resource_snapshot_rejects_foreign_malformed_and_wrong_subscription_delivery() -> Result<()> {
        use rmcp::model::{
            NotificationMetaObject, RequestId, ResourceUpdatedNotification,
            ResourceUpdatedNotificationParam,
        };
        let uri = veoveo_speech_contract::TranscriptionUri::new(
            veoveo_speech_contract::TranscriptionId::new(),
        );
        let id = RequestId::Number(7);
        let update = |uri: &str, subscription: Option<RequestId>| {
            let mut params = ResourceUpdatedNotificationParam::new(uri);
            if let Some(subscription) = subscription {
                let mut meta = NotificationMetaObject::new();
                meta.set_subscription_id(subscription);
                params.meta = Some(meta);
            }
            ServerNotification::ResourceUpdatedNotification(ResourceUpdatedNotification::new(
                params,
            ))
        };
        resource_delivery(&update(uri.to_uri().as_str(), Some(id.clone())), &id, &uri)?;
        for wrong in [
            "speech://transcript/not-an-id",
            "artifact://foreign",
            "speech://dictation/not-an-id",
        ] {
            ensure!(resource_delivery(&update(wrong, Some(id.clone())), &id, &uri).is_err());
        }
        let other = veoveo_speech_contract::TranscriptionUri::new(
            veoveo_speech_contract::TranscriptionId::new(),
        );
        ensure!(
            resource_delivery(
                &update(other.to_uri().as_str(), Some(id.clone())),
                &id,
                &uri
            )
            .is_err()
        );
        for wrong in [None, Some(RequestId::Number(8))] {
            ensure!(resource_delivery(&update(uri.to_uri().as_str(), wrong), &id, &uri).is_err());
        }
        let mut malformed = serde_json::to_value(update(uri.to_uri().as_str(), Some(id.clone())))?;
        malformed["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"] =
            serde_json::json!({"unexpected":"object"});
        let malformed: ServerNotification = serde_json::from_value(malformed)?;
        ensure!(resource_delivery(&malformed, &id, &uri).is_err());
        Ok(())
    }
    #[test]
    fn fixture_is_closed_and_requires_independent_provenance() -> Result<()> {
        let value = serde_json::json!({"sourceSha256":"a".repeat(64),"text":"fixture text","model":"fixture-model","modelRevision":"fixture-revision", "minimumDurationSeconds":1,"maximumDurationSeconds":2,"compliance":{}});
        let expected: Expected = serde_json::from_value(value.clone())?;
        ensure!(expected.admit().is_err());
        let mut unknown = value;
        unknown["unknown"] = true.into();
        ensure!(serde_json::from_value::<Expected>(unknown).is_err());
        Ok(())
    }
    #[test]
    fn independent_output_assertions_reject_source_model_and_owner_substitution() -> Result<()> {
        let fixture = super::super::tests::input()?;
        let metadata_value = |mime| {
            let id = veoveo_artifact_contract::ArtifactId::new();
            serde_json::json!({"artifactId":id,"artifactUri":veoveo_artifact_contract::ArtifactUri::presented(&veoveo_speech_contract::ARTIFACT_SCHEME,id), "byteLen":10, "mimeType":mime, "createdAt":"2026-10-09T00:00:00Z", "compliance":fixture.expected.compliance, "metadata":{"sourceArtifactUri":fixture.request.artifact_uri,"sourceSha256":fixture.expected.source_sha256,"model":fixture.expected.model,"modelRevision":fixture.expected.model_revision}})
        };
        let value = serde_json::json!({"resultUri":veoveo_speech_contract::TranscriptionUri::new(veoveo_speech_contract::TranscriptionId::new()),"sourceArtifactUri":fixture.request.artifact_uri,"transcript":metadata_value("application/json"),"captions":metadata_value("text/vtt"),"durationSeconds":1.5});
        let output: TranscriptionOutput = serde_json::from_value(value.clone())?;
        metadata(&fixture.request, &fixture.expected, &output)?;
        let mut current = View {
            task_id: output.result_uri.id(),
            status: veoveo_platform_store::TaskStatus::Succeeded,
            message: None,
            output: Some(output.clone()),
        };
        current_output(&current, &output)?;
        let mut changed_duration = value.clone();
        changed_duration["durationSeconds"] = serde_json::json!(1.75);
        current.output = Some(serde_json::from_value(changed_duration)?);
        ensure!(current_output(&current, &output).is_err());
        current.output = Some(output.clone());
        current.task_id = veoveo_speech_contract::TranscriptionId::new();
        ensure!(current_output(&current, &output).is_err());
        current.task_id = output.result_uri.id();
        current.status = veoveo_platform_store::TaskStatus::Running;
        ensure!(current_output(&current, &output).is_err());
        current.status = veoveo_platform_store::TaskStatus::Succeeded;
        current.output = None;
        ensure!(current_output(&current, &output).is_err());

        for field in ["model", "sourceSha256"] {
            let mut wrong = value.clone();
            let substitute = if field == "model" {
                serde_json::json!("different-model")
            } else {
                serde_json::json!("b".repeat(64))
            };
            for kind in ["transcript", "captions"] {
                wrong[kind]["metadata"][field] = substitute.clone();
            }
            let admitted: TranscriptionOutput = serde_json::from_value(wrong)?;
            ensure!(metadata(&fixture.request, &fixture.expected, &admitted).is_err());
        }
        let mut wrong = value;
        wrong["transcript"]["compliance"]["owner"]["id"] = "https://installation.test#other".into();
        let admitted: TranscriptionOutput = serde_json::from_value(wrong)?;
        ensure!(metadata(&fixture.request, &fixture.expected, &admitted).is_err());
        Ok(())
    }
}
