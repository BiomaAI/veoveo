//! Only a run and its results have a Task-backed update source.
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, TaskId, TaskResourceAddress};

use super::{
    RunId, RunResultsUri, RunUri, StreamContractError, StreamResource, ids::string_schema,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RunResource(StreamResource);

impl RunResource {
    pub fn run(id: RunId) -> Self {
        Self(StreamResource::Run(RunUri::new(id)))
    }

    pub fn results(id: RunId) -> Self {
        Self(StreamResource::RunResults(RunResultsUri::new(id)))
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let address = StreamResource::parse(value)?;
        if address.subscription_run().is_none() {
            return Err(StreamContractError::InvalidResource);
        }
        Ok(Self(address))
    }

    pub fn run_id(&self) -> RunId {
        self.0.subscription_run().expect("admitted run route")
    }
}

impl ResourceAddress for RunResource {
    type Error = StreamContractError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        self.0.to_uri()
    }
}

impl TaskResourceAddress for RunResource {
    fn task_id(&self) -> TaskId {
        self.run_id().task_id()
    }
}

impl TryFrom<String> for RunResource {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<RunResource> for String {
    fn from(value: RunResource) -> Self {
        value.to_uri().expect("admitted run route").to_string()
    }
}
string_schema!(RunResource);
