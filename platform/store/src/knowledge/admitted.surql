  tenant = $tenant
  AND $generation.tenant = $tenant
  AND $generation.state = 'active'
  AND $active.generation = $generation
  AND $resource_selected(collection_id, uri, $resource_selections)
  AND collection.approved = true
  AND $scopes CONTAINSALL collection.document.descriptor.requiredScopes
  AND collection.tenant = $tenant
  AND collection.collection = collection_id
  AND collection.revision = approval_revision
  AND member.sync.ready = true
  AND member.source_epoch = member.sync.epoch
  AND member.sync.owner_epoch = member.sync.coordinator.epoch
  AND (collection.document.descriptor.changeSignal = 'immutable' OR member.sync.coordinator.expires_at > time::now())
  AND (member.valid_until = NONE OR member.valid_until > time::now())
  AND member.stale = false
  AND member.deleted = false
  AND member.generation = $generation
  AND member.tenant = $tenant
  AND member.uri = uri
  AND $clearance CONTAINSALL admission.labels
  AND (admission.expires_at = NONE OR admission.expires_at > time::now())
  AND (
    admission.required_context = NONE
    OR admission.required_context = $active_context
  )
  AND (
    admission.required_profile = NONE
    OR admission.required_profile = $profile
  )
  AND (
    admission.tenant_read = true
    OR (
      admission.context_read = true
      AND admission.work_context IN $contexts
    )
    OR (
      admission.selected_context_read = true
      AND admission.work_context = $active_context
      AND admission.work_context IN $contexts
    )
    OR array::len(admission.grants[WHERE subject IN $subjects
      AND (expires_at = NONE OR expires_at > time::now())]) > 0
  )
