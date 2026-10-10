//! Opt-in public OAuth consumer admission and private, incremental observations.
use super::*;
use serde::Serialize;
use veoveo_testing_support::final_tasks::public_caller::{
    PrivateCallerJournal, PublicCallerInput, read_private_input,
};
use veoveo_types::ResourceUri;

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Schema {
    #[vocabulary(rename = "veoveo.ai/stream-public-caller/v1")]
    V1,
}
type Input = PublicCallerInput<Schema>;

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum EvidenceSchema {
    #[vocabulary(rename = "veoveo.ai/stream-public-consumer/v1")]
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
    pub(super) journal: std::sync::Arc<PrivateCallerJournal>,
    pub(super) recovery: Option<super::recovery::Fixture>,
    complete: bool,
}
impl Profile {
    pub(super) fn load(
        path: &Path,
        installation: &InstalledTarget,
        candidate: bool,
    ) -> Result<Self> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Selection {
            Complete(Input),
            Recover(Box<super::recovery::Input>),
        }
        let (input, recovery) = match read_private_input::<Selection>(path)? {
            Selection::Complete(input) => (input, None),
            Selection::Recover(input) => {
                let input = *input;
                input.admit(installation)?;
                (input.caller, Some(input.fixture))
            }
        };
        input.admit(&installation.target, candidate, &Schema::V1)?;
        let client = FinalTaskSmokeClient::from_private_token_file(
            &input.endpoint,
            &input.caller_token_file,
        )?;
        let journal = std::sync::Arc::new(PrivateCallerJournal::create(&input.output)?);
        let profile = Self {
            client,
            journal,
            recovery,
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

pub(super) fn resource_identity(
    result: &veoveo_stream_mcp::contract::RunRecordingOutput,
) -> Result<ResourceUri> {
    let resource =
        veoveo_stream_mcp::contract::StreamResource::parse(result.result_uri().to_uri())?;
    let veoveo_stream_mcp::contract::StreamResource::RunResults(selected) = resource else {
        bail!("Stream output did not select a run-results resource");
    };
    ensure!(
        *selected.id() == result.run_id(),
        "Stream result and run identities disagree"
    );
    Ok(selected.to_uri())
}
#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_deploy_contract::InstallationTarget;
    use veoveo_types::{GatewayProfileId, HttpsUrl};

    #[test]
    fn stream_public_fixture_rejects_cross_installation_profile_and_candidate() -> Result<()> {
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
    fn stream_public_resource_identity_comes_from_owner_output() -> Result<()> {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/stream-mcp/testdata/run-output.json"
        ));
        let output: veoveo_stream_mcp::contract::RunRecordingOutput =
            serde_json::from_slice(bytes)?;
        assert_eq!(resource_identity(&output)?, output.result_uri().to_uri());
        let mut crossed: serde_json::Value = serde_json::from_slice(bytes)?;
        crossed["resultUri"] =
            serde_json::json!("stream://run/01983da0-0000-7000-8000-000000000002/results");
        assert!(
            serde_json::from_value::<veoveo_stream_mcp::contract::RunRecordingOutput>(crossed)
                .map(|output| resource_identity(&output).is_err())
                .unwrap_or(true)
        );
        Ok(())
    }
}
