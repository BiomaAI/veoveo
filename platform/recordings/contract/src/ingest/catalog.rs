//! Validated owner indexes for one admitted Recording configuration revision.
use super::{RecordingIngestResource, RecordingProducerId, RecordingProducerRegistration};
use crate::RecordingProducerScope;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use veoveo_gateway_contract::{ProtectedResourceName, UpstreamTransportSecurity};
use veoveo_types::{OAuthClientId, ScopeDefinition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingCatalogError {
    pub resource: ProtectedResourceName,
    pub reason: String,
}
impl fmt::Display for RecordingCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Recording ingest resource `{}`: {}",
            self.resource, self.reason
        )
    }
}
impl std::error::Error for RecordingCatalogError {}

#[derive(Debug, Clone)]
pub struct RecordingCatalog {
    resources: Vec<RecordingIngestResource>,
    by_resource: BTreeMap<ProtectedResourceName, usize>,
    by_producer: BTreeMap<RecordingProducerId, (usize, usize)>,
}
impl RecordingCatalog {
    pub fn from_admitted(
        registry: &veoveo_gateway_contract::CatalogRegistry,
        sections: &veoveo_gateway_contract::AdmittedCatalogSections,
    ) -> Result<Self, veoveo_types::ExtensionError> {
        let key = registry.section_key::<super::RecordingCatalogSection>(
            &veoveo_types::ExtensionName::parse(super::RECORDING_INGEST_SECTION)?,
        )?;
        let resources = sections
            .get(&key)?
            .map(|section| section.0)
            .unwrap_or_default();
        Self::new(resources).map_err(|error| veoveo_types::ExtensionError::new(error.to_string()))
    }

    /// Relationship checks against installation auth/policy facts run during section admission.
    pub fn new(resources: Vec<RecordingIngestResource>) -> Result<Self, RecordingCatalogError> {
        let mut by_resource = BTreeMap::new();
        let mut by_producer = BTreeMap::new();
        let mut protected_resources = BTreeSet::new();
        for (resource_index, resource) in resources.iter().enumerate() {
            let reject = |reason: String| RecordingCatalogError {
                resource: resource.id.clone(),
                reason,
            };
            if by_resource
                .insert(resource.id.clone(), resource_index)
                .is_some()
            {
                return Err(reject("duplicate Recording ingest resource".into()));
            }
            if !protected_resources.insert(resource.protected_resource.clone()) {
                return Err(reject("duplicate protected resource".into()));
            }
            if resource.maximum_batch_bytes == 0 {
                return Err(reject("maximum_batch_bytes must be positive".into()));
            }
            if !resource
                .required_scopes
                .contains(RecordingProducerScope::Ingest.name())
            {
                return Err(reject(format!(
                    "required_scopes must contain {}",
                    RecordingProducerScope::Ingest
                )));
            }
            if resource.upstream.security != UpstreamTransportSecurity::ClusterInternalHttp
                || resource
                    .upstream
                    .url
                    .parsed()
                    .ok()
                    .is_none_or(|url| url.scheme() != "http")
            {
                return Err(reject(
                    "upstream must use cluster_internal_http over HTTP".into(),
                ));
            }
            if resource.producers.is_empty() {
                return Err(reject("at least one recording producer is required".into()));
            }
            for (producer_index, producer) in resource.producers.iter().enumerate() {
                if by_producer
                    .insert(producer.id.clone(), (resource_index, producer_index))
                    .is_some()
                {
                    return Err(reject(format!(
                        "duplicate Recording producer `{}`",
                        producer.id
                    )));
                }
                if !producer
                    .single_recording_application_ids
                    .is_subset(&producer.allowed_application_ids)
                {
                    return Err(reject(format!(
                        "producer `{}` names a single-recording application outside its allowlist",
                        producer.id
                    )));
                }
                if producer.allowed_application_ids.is_empty()
                    || producer.classification.trim().is_empty()
                    || producer.quotas.maximum_concurrent_streams == 0
                    || producer.quotas.maximum_batches_per_minute == 0
                    || producer.quotas.maximum_bytes_per_day == 0
                    || producer.quotas.maximum_stream_bytes == 0
                    || (producer.blueprints.enabled
                        && (producer.blueprints.maximum_bytes == 0
                            || producer.blueprints.maximum_bytes > resource.maximum_batch_bytes
                            || producer.blueprints.maximum_messages == 0
                            || producer.blueprints.maximum_revisions == 0))
                    || producer.retention.open_stream_days == 0
                {
                    return Err(reject(format!(
                        "producer `{}` has an empty allowlist or non-positive policy limit",
                        producer.id
                    )));
                }
            }
        }
        Ok(Self {
            resources,
            by_resource,
            by_producer,
        })
    }
    pub fn resource(&self, id: &ProtectedResourceName) -> Option<&RecordingIngestResource> {
        self.by_resource
            .get(id)
            .map(|index| &self.resources[*index])
    }
    pub fn resources(&self) -> impl Iterator<Item = &RecordingIngestResource> {
        self.resources.iter()
    }
    pub fn single_resource(&self) -> Option<&RecordingIngestResource> {
        (self.resources.len() == 1).then(|| &self.resources[0])
    }
    pub fn resource_by_protected_resource(
        &self,
        id: &veoveo_gateway_contract::ProtectedResourceId,
    ) -> Option<&RecordingIngestResource> {
        self.resources
            .iter()
            .find(|resource| &resource.protected_resource == id)
    }
    pub fn producer(
        &self,
        id: &RecordingProducerId,
    ) -> Option<(&RecordingIngestResource, &RecordingProducerRegistration)> {
        self.by_producer.get(id).map(|(resource, producer)| {
            (
                &self.resources[*resource],
                &self.resources[*resource].producers[*producer],
            )
        })
    }
    pub fn producer_for_client<'a>(
        &self,
        resource: &'a RecordingIngestResource,
        client: &OAuthClientId,
    ) -> Option<&'a RecordingProducerRegistration> {
        resource
            .producers
            .iter()
            .find(|producer| &producer.oauth_client == client)
    }
}
