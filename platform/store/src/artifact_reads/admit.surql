UPDATE $capability SET
  admitted_artifacts += { artifact: $artifact, byte_len: $byte_len },
  used_total_bytes += $byte_len
WHERE
  token_hash = $token_hash
  AND task_id = $task_id
  AND revoked_at = NONE
  AND expires_at > time::now()
  AND crypto::sha256(
    <string> [
      work_context.policy_revision,
      work_context.memberships,
      work_context.output_policy,
    ]
  ) = context_digest
  AND work_context.policy_revision = authority.policy_revision
  AND admitted_artifacts.artifact CONTAINSNOT $artifact
  AND array::len(admitted_artifacts) < max_artifact_count
  AND $byte_len >= 0
  AND used_total_bytes + $byte_len <= max_total_bytes
RETURN AFTER;
