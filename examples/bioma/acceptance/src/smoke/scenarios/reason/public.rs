//! Opt-in public OAuth consumer admission and private, incremental observations.
use super::*;
use serde::Serialize;
use veoveo_testing_support::final_tasks::public_caller::{
    PrivateCallerJournal, PublicCallerInput, read_private_input,
};
use veoveo_types::{ResourceAddress, ResourceUri};

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/reason-public-caller/v1")]
    V1,
}
type Input = PublicCallerInput<Schema>;

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum EvidenceSchema {
    #[vocabulary(rename = "veoveo.ai/reason-public-consumer/v1")]
    V1,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum ObservationKind {
    Prepared,
    TaskDispatchIntent,
    TaskAcknowledged,
    ResourceSnapshotIntent,
    DeliveredTaskCompleted,
    AcknowledgedInitialCurrentResourceSnapshot,
    Complete,
    Incomplete,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Observation<'a> {
    schema: EvidenceSchema,
    observation: ObservationKind,
    task: Option<&'a veoveo_types::CanonicalTaskId>,
    statuses: &'a [rmcp::model::TaskStatus],
    resource: Option<&'a ResourceUri>,
}
pub(super) struct Profile {
    pub(super) client: FinalTaskSmokeClient,
    journal: PrivateCallerJournal,
    complete: bool,
}
impl Profile {
    pub(super) fn load(
        path: &Path,
        installation: &InstalledTarget,
        candidate: bool,
    ) -> Result<Self> {
        let input: Input = read_private_input(path)?;
        input.admit(&installation.target, candidate, &Schema::V1)?;
        let client = FinalTaskSmokeClient::from_private_token_file(
            &input.endpoint,
            &input.caller_token_file,
        )?;
        let journal = PrivateCallerJournal::create(&input.output)?;
        let profile = Self {
            client,
            journal,
            complete: false,
        };
        profile.record(ObservationKind::Prepared, None, &[], None)?;
        Ok(profile)
    }
    pub(super) fn record(
        &self,
        observation: ObservationKind,
        task: Option<&veoveo_types::CanonicalTaskId>,
        statuses: &[rmcp::model::TaskStatus],
        resource: Option<&ResourceUri>,
    ) -> Result<()> {
        self.journal.append(&Observation {
            schema: EvidenceSchema::V1,
            observation,
            task,
            statuses,
            resource,
        })
    }
    pub(super) fn complete(&mut self) -> Result<()> {
        self.record(ObservationKind::Complete, None, &[], None)?;
        self.complete = true;
        Ok(())
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        if !self.complete {
            let _ = self.record(ObservationKind::Incomplete, None, &[], None);
        }
    }
}

pub(super) fn resource_identity(result: &AnalyzeRecordingOutput) -> Result<ResourceUri> {
    let selected =
        veoveo_reason_mcp::contract::AnalysisResource::parse(result.result_uri().to_string())?;
    ensure!(
        selected.analysis_id() == *result.analysis_uri.id(),
        "Reason result and analysis identities disagree"
    );
    Ok(selected.to_uri()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_deploy_contract::InstallationTarget;
    use veoveo_types::{GatewayProfileId, HttpsUrl};

    #[test]
    fn reason_public_fixture_rejects_cross_installation_profile_and_candidate() -> Result<()> {
        let target = InstallationTarget::decode(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../testing/fixtures/catalog-installation/installation-target.json"
        )))?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("installation.json");
        std::fs::write(&path, serde_json::to_vec(&target)?)?;
        let mut endpoint = target.public_base_url.clone();
        endpoint
            .path_segments_mut()
            .unwrap()
            .clear()
            .extend(["mcp", target.operator.profile.as_str()]);
        let mut input = Input {
            schema: Schema::V1,
            installation_target: path,
            endpoint: HttpsUrl::parse(endpoint.as_str())?,
            profile: GatewayProfileId::parse(&target.operator.profile)?,
            caller_token_file: directory.path().join("unread-token"),
            output: directory.path().join("new-evidence.jsonl"),
        };
        input.admit(&target, false, &Schema::V1)?;
        assert!(input.admit(&target, true, &Schema::V1).is_err());
        input.profile = GatewayProfileId::parse("other-profile")?;
        assert!(input.admit(&target, false, &Schema::V1).is_err());
        input.profile = GatewayProfileId::parse(&target.operator.profile)?;
        input.endpoint = HttpsUrl::parse("https://other.example.invalid/mcp/operator")?;
        assert!(input.admit(&target, false, &Schema::V1).is_err());
        input.endpoint = HttpsUrl::parse(endpoint.as_str())?;
        let mut changed = target.clone();
        changed.kubernetes.namespace = "other-release".into();
        std::fs::write(&input.installation_target, serde_json::to_vec(&changed)?)?;
        assert!(input.admit(&target, false, &Schema::V1).is_err());
        assert!(!input.output.exists());
        assert!(!input.caller_token_file.exists());
        Ok(())
    }

    #[test]
    fn reason_public_resource_identity_comes_from_owner_output_and_checks_request() -> Result<()> {
        let output: AnalyzeRecordingOutput = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/reason-mcp/testdata/analysis-output-v1.json"
        )))?;
        let identity = resource_identity(&output)?;
        assert_eq!(identity, output.result_uri().to_uri());
        let mut request: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/reason-mcp/testdata/task-request.json"
        )))?;
        let mut input = request["input"].take();
        input.as_object_mut().unwrap().remove("operation");
        let mut request: veoveo_reason_mcp::contract::AnalyzeRecordingRequest =
            serde_json::from_value(input)?;
        output.check_request(&request)?;
        request.pipeline_id = veoveo_reason_mcp::contract::PipelineId::parse("other-pipeline")?;
        assert!(output.check_request(&request).is_err());
        let mut crossed = serde_json::to_value(&output)?;
        crossed["resultUri"] = serde_json::to_value(veoveo_reason_mcp::contract::ResultsUri::new(
            veoveo_reason_mcp::contract::AnalysisId::parse("01983da0-0000-7000-8000-000000000099")?,
        ))?;
        assert!(serde_json::from_value::<AnalyzeRecordingOutput>(crossed).is_err());
        Ok(())
    }
}
