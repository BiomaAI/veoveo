SELECT *
FROM ONLY $capability
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
  AND work_context.policy_revision = authority.policy_revision;
