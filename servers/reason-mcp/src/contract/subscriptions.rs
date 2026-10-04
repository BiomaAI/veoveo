//! Only an analysis and its results have a Task-backed update source.
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, TaskId, TaskResourceAddress};

use super::{AnalysisId, AnalysisUri, ReasonContractError, ReasonResource, ResultsUri};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AnalysisResource(ReasonResource);

impl AnalysisResource {
    pub fn analysis(id: AnalysisId) -> Self {
        Self(ReasonResource::Analysis(AnalysisUri::new(id)))
    }

    pub fn results(id: AnalysisId) -> Self {
        Self(ReasonResource::Results(ResultsUri::new(id)))
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, ReasonContractError> {
        let address = ReasonResource::parse(value)?;
        if address.subscription_analysis().is_none() {
            return Err(ReasonContractError::InvalidResource);
        }
        Ok(Self(address))
    }

    pub fn analysis_id(&self) -> AnalysisId {
        self.0
            .subscription_analysis()
            .expect("admitted analysis route")
    }
}

impl ResourceAddress for AnalysisResource {
    type Error = ReasonContractError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        self.0.to_uri()
    }
}

impl TaskResourceAddress for AnalysisResource {
    fn task_id(&self) -> TaskId {
        self.analysis_id().task_id()
    }
}

impl TryFrom<String> for AnalysisResource {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<AnalysisResource> for String {
    fn from(value: AnalysisResource) -> Self {
        value.to_uri().expect("admitted analysis route").to_string()
    }
}
impl schemars::JsonSchema for AnalysisResource {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnalysisResource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}
