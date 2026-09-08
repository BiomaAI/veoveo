BEGIN TRANSACTION;
  LET $context = SELECT * FROM ONLY $content.work_context;
  IF $context = NONE
  OR crypto::sha256(
    <string> [
      $context.policy_revision,
      $context.memberships,
      $context.output_policy,
    ]
  ) != $content.context_digest
  OR $context.policy_revision != $content.authority.policy_revision {
    THROW 'artifact read context changed';
  };
  CREATE ONLY $record CONTENT $content RETURN NONE;
COMMIT TRANSACTION;
