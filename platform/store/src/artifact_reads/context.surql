SELECT policy_revision,
crypto::sha256(<string> [policy_revision, memberships, output_policy]) AS digest
FROM ONLY $context;
