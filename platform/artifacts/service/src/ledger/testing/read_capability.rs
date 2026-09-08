use super::*;
use veoveo_mcp_contract::{ArtifactReadCapabilityId, WorkContextId};

pub(super) struct CapabilityState {
    draft: ReadCapabilityDraft,
    admitted: std::collections::BTreeMap<ArtifactId, u64>,
    revoked: bool,
}

fn valid(state: &State, authentication: &ReadCapabilityAuthentication) -> bool {
    state
        .read_capabilities
        .get(&authentication.capability_id)
        .is_some_and(|cap| {
            !cap.revoked
                && cap.draft.expires_at > chrono::Utc::now()
                && cap.draft.token_hash == authentication.token_hash
                && cap.draft.task_id == authentication.task_id
                && state.read_contexts.get(&(
                    cap.draft.actor.tenant.clone(),
                    cap.draft.authority.work_context.clone(),
                )) == Some(&cap.draft.context)
        })
}

impl InMemoryRepository {
    pub(crate) fn set_read_context(
        &self,
        tenant: TenantId,
        context: WorkContextId,
        version: ReadContextVersion,
    ) {
        self.state
            .lock()
            .unwrap()
            .read_contexts
            .insert((tenant, context), version);
    }
}

impl ReadCapabilityRepository for InMemoryRepository {
    async fn read_context(
        &self,
        actor: &RepositoryActor,
        context: &WorkContextId,
    ) -> Result<Option<ReadContextVersion>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .read_contexts
            .get(&(actor.tenant.clone(), context.clone()))
            .cloned())
    }
    async fn create_read_capability(
        &self,
        draft: ReadCapabilityDraft,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if state.read_contexts.get(&(
            draft.actor.tenant.clone(),
            draft.authority.work_context.clone(),
        )) != Some(&draft.context)
            || state.read_capabilities.contains_key(&draft.capability_id)
        {
            return Err(RepositoryError::Conflict(
                "read delegation context changed or identity exists".into(),
            ));
        }
        state.read_capabilities.insert(
            draft.capability_id,
            CapabilityState {
                draft,
                admitted: Default::default(),
                revoked: false,
            },
        );
        Ok(())
    }
    async fn read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
    ) -> Result<Option<ReadCapabilityDraft>, RepositoryError> {
        let state = self.state.lock().unwrap();
        Ok(valid(&state, authentication).then(|| {
            state.read_capabilities[&authentication.capability_id]
                .draft
                .clone()
        }))
    }
    async fn admit_read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
        artifact: ArtifactId,
        byte_len: u64,
    ) -> Result<bool, RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if !valid(&state, authentication) {
            return Ok(false);
        }
        let cap = state
            .read_capabilities
            .get_mut(&authentication.capability_id)
            .unwrap();
        if let Some(previous) = cap.admitted.get(&artifact) {
            return Ok(*previous == byte_len);
        }
        let used = cap
            .admitted
            .values()
            .try_fold(0_u64, |total, bytes| total.checked_add(*bytes));
        if cap.admitted.len() >= cap.draft.max_artifact_count as usize
            || used
                .and_then(|used| used.checked_add(byte_len))
                .is_none_or(|total| total > cap.draft.max_total_bytes)
        {
            return Ok(false);
        }
        cap.admitted.insert(artifact, byte_len);
        Ok(true)
    }
    async fn revoke_read_capability(
        &self,
        capability: ArtifactReadCapabilityId,
        actor: &RepositoryActor,
    ) -> Result<bool, RepositoryError> {
        let mut state = self.state.lock().unwrap();
        let Some(cap) = state.read_capabilities.get_mut(&capability) else {
            return Ok(false);
        };
        if cap.draft.actor.tenant != actor.tenant
            || cap.draft.actor.principal != actor.principal
            || cap.draft.actor.issuer != actor.issuer
            || cap.draft.actor.subject != actor.subject
        {
            return Ok(false);
        }
        cap.revoked = true;
        Ok(true)
    }
}
