SELECT policy_revision,
  crypto::sha256(<string> [policy_revision, memberships, output_policy]) AS context_digest,
  fn::artifact_upload_profile_digest($profile_key) AS profile_policy_digest
FROM ONLY $context;
