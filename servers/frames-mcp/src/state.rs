use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow, bail};
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationProvenance, FrameWorldId, FrameWorldRevision,
    FrameWorldRevisionId, FrameWorldRevisionUri, FrameWorldUri, WorldFrameUri,
};
use veoveo_platform_store::{
    CoordinateOperationDraft, FrameWorldDraft, FrameWorldRecord, FrameWorldRevisionDraft,
    FrameWorldRevisionRecord, OpenObject, PlatformIdentity, PlatformStore, TaskId,
};

use crate::{
    contract::{CreateWorldRequest, FrameWorldSummary, PublishWorldOutput, PublishWorldRequest},
    world::validate_world_tree,
};

#[derive(Clone, Debug)]
pub struct FrameScope {
    pub identity: PlatformIdentity,
    pub data_labels: BTreeSet<String>,
}

#[derive(Clone)]
pub struct FramesState {
    store: PlatformStore,
}

impl FramesState {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub async fn list_worlds(&self, scope: &FrameScope) -> Result<Vec<FrameWorldSummary>> {
        self.store
            .list_frame_worlds(scope.identity.tenant_id)
            .await?
            .into_iter()
            .filter(|world| labels_allow(&world.labels, &scope.data_labels))
            .map(world_summary)
            .collect()
    }

    pub async fn get_world(
        &self,
        scope: &FrameScope,
        world_id: &FrameWorldId,
    ) -> Result<Option<FrameWorldSummary>> {
        let Some(world) = self
            .store
            .frame_world_by_key(scope.identity.tenant_id, world_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&world.labels, &scope.data_labels) {
            return Ok(None);
        }
        Ok(Some(world_summary(world)?))
    }

    pub async fn create_world(
        &self,
        scope: &FrameScope,
        request: CreateWorldRequest,
    ) -> Result<FrameWorldSummary> {
        if request.display_name.trim().is_empty() {
            bail!("display_name must not be blank");
        }
        if let Some(existing) = self.get_world(scope, &request.world_id).await? {
            if existing.display_name == request.display_name
                && existing.description == request.description
            {
                return Ok(existing);
            }
            bail!(
                "frame world `{}` already exists with different metadata",
                request.world_id
            );
        }
        let world = self
            .store
            .create_frame_world(FrameWorldDraft {
                identity: scope.identity.clone(),
                world_key: request.world_id.to_string(),
                display_name: request.display_name,
                description: request.description,
                classification: "gateway_labels".to_owned(),
                labels: scope.data_labels.iter().cloned().collect(),
            })
            .await?;
        world_summary(world)
    }

    pub async fn publish_world(
        &self,
        scope: &FrameScope,
        request: PublishWorldRequest,
    ) -> Result<PublishWorldOutput> {
        let validated = validate_world_tree(request.tree)?;
        let revision_id = FrameWorldRevisionId::new(format!("revision-{}", uuid::Uuid::now_v7()))?;
        let publication = self
            .store
            .publish_frame_world_revision(FrameWorldRevisionDraft {
                identity: scope.identity.clone(),
                world_key: request.world_id.to_string(),
                expected_head_revision_key: request.expected_head_revision_id.map(String::from),
                revision_key: revision_id.to_string(),
                spec_sha256: validated.spec_sha256,
                root_frame_key: validated.root_frame_id.to_string(),
                definition: object_from_value(serde_json::to_value(validated.tree)?)?,
            })
            .await?;
        Ok(PublishWorldOutput {
            world: world_summary(publication.world)?,
            revision: world_revision(publication.revision)?,
            created: publication.created,
        })
    }

    pub async fn get_revision(
        &self,
        scope: &FrameScope,
        revision_uri: &FrameWorldRevisionUri,
    ) -> Result<Option<FrameWorldRevision>> {
        let world_id = revision_uri.world_id();
        let Some(world) = self
            .store
            .frame_world_by_key(scope.identity.tenant_id, world_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&world.labels, &scope.data_labels) {
            return Ok(None);
        }
        let revision_id = revision_uri.revision_id();
        self.store
            .frame_world_revision_by_key(
                scope.identity.tenant_id,
                world_id.as_str(),
                revision_id.as_str(),
            )
            .await?
            .map(world_revision)
            .transpose()
    }

    pub async fn get_head_revision(
        &self,
        scope: &FrameScope,
        world_id: &FrameWorldId,
    ) -> Result<Option<FrameWorldRevision>> {
        let Some(world) = self
            .store
            .frame_world_by_key(scope.identity.tenant_id, world_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&world.labels, &scope.data_labels) {
            return Ok(None);
        }
        self.store
            .frame_world_head_revision(scope.identity.tenant_id, world_id.as_str())
            .await?
            .map(world_revision)
            .transpose()
    }

    pub async fn require_revision(
        &self,
        scope: &FrameScope,
        revision_uri: &FrameWorldRevisionUri,
    ) -> Result<FrameWorldRevision> {
        self.get_revision(scope, revision_uri)
            .await?
            .ok_or_else(|| anyhow!("unknown frame world revision `{revision_uri}`"))
    }

    pub async fn require_frame_revision(
        &self,
        scope: &FrameScope,
        frame_uri: &WorldFrameUri,
    ) -> Result<FrameWorldRevision> {
        let revision = self
            .require_revision(scope, &frame_uri.revision_uri())
            .await?;
        if revision.frame(frame_uri).is_none() {
            bail!("unknown world frame `{frame_uri}`");
        }
        Ok(revision)
    }

    pub async fn record_operation(
        &self,
        scope: &FrameScope,
        task_id: Option<TaskId>,
        provenance: &CoordinateOperationProvenance,
    ) -> Result<()> {
        let kind = serde_json::to_value(&provenance.kind)?
            .as_str()
            .ok_or_else(|| anyhow!("coordinate operation kind did not serialize as a string"))?
            .to_owned();
        self.store
            .upsert_coordinate_operation(CoordinateOperationDraft {
                identity: scope.identity.clone(),
                task_id,
                operation_key: provenance.operation.operation_id.to_string(),
                kind,
                provenance: object_from_value(serde_json::to_value(provenance)?)?,
                classification: "gateway_labels".to_owned(),
                labels: scope.data_labels.iter().cloned().collect(),
                created_at: provenance.operation.created_at,
            })
            .await?;
        Ok(())
    }

    pub async fn get_operation(
        &self,
        scope: &FrameScope,
        operation_id: &CoordinateOperationId,
    ) -> Result<Option<CoordinateOperationProvenance>> {
        let Some(record) = self
            .store
            .coordinate_operation(scope.identity.tenant_id, operation_id.as_str())
            .await?
        else {
            return Ok(None);
        };
        if !labels_allow(&record.labels, &scope.data_labels) {
            return Ok(None);
        }
        Ok(Some(serde_json::from_value(value_from_object(
            record.provenance,
        ))?))
    }
}

fn world_summary(record: FrameWorldRecord) -> Result<FrameWorldSummary> {
    let world_id = FrameWorldId::new(record.world_key)?;
    Ok(FrameWorldSummary {
        world_uri: FrameWorldUri::new(&world_id),
        world_id,
        display_name: record.display_name,
        description: record.description,
        head_revision_id: record
            .head_revision_key
            .map(FrameWorldRevisionId::new)
            .transpose()?,
        revision: u64::try_from(record.revision).context("negative frame world revision")?,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn world_revision(record: FrameWorldRevisionRecord) -> Result<FrameWorldRevision> {
    let world_id = FrameWorldId::new(record.world_key)?;
    let revision_id = FrameWorldRevisionId::new(record.revision_key)?;
    let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
    let root_frame_id = veoveo_mcp_contract::FrameId::new(record.root_frame_key)?;
    Ok(FrameWorldRevision {
        world_uri: FrameWorldUri::new(&world_id),
        world_id,
        revision_id,
        revision_uri: revision_uri.clone(),
        revision: u64::try_from(record.revision).context("negative frame world revision")?,
        spec_sha256: record.spec_sha256,
        root_frame_uri: WorldFrameUri::new(&revision_uri, &root_frame_id),
        tree: serde_json::from_value(value_from_object(record.definition))
            .context("decoding frame world revision")?,
        created_at: record.created_at,
    })
}

fn object_from_value(value: serde_json::Value) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(values.into_iter().collect())),
        _ => bail!("frame world record must serialize as an object"),
    }
}

fn value_from_object(object: OpenObject) -> serde_json::Value {
    serde_json::Value::Object(object.into_map().into_iter().collect())
}

fn labels_allow(required: &[String], caller: &BTreeSet<String>) -> bool {
    required.iter().all(|label| caller.contains(label))
}
