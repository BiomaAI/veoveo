SELECT
  tenant,
  collection_id,
  uri,
  ordinal,
  text,
  admission,
  observation
FROM __TABLE__
WHERE
  tenant = $tenant
  AND $generation.tenant = $tenant
  AND $generation.state = 'active'
  AND $active.generation = $generation
  AND collection_id IN $collections
  AND collection.approved = true
  AND collection.tenant = $tenant
  AND collection.collection = collection_id
  AND collection.revision = approval_revision
  AND member.stale = false
  AND member.deleted = false
  AND member.generation = $generation
  AND member.tenant = $tenant
  AND member.uri = uri
  AND $clearance CONTAINSALL admission.labels
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
    OR admission.subjects CONTAINSANY $subjects
  )
  AND (
    $after_collection = NONE
    OR collection_id > $after_collection
    OR (
      collection_id = $after_collection
      AND uri > $after_uri
    )
    OR (
      collection_id = $after_collection
      AND uri = $after_uri
      AND ordinal > $after_ordinal
    )
  )
ORDER BY collection_id ASC, uri ASC, ordinal ASC
LIMIT $limit;
