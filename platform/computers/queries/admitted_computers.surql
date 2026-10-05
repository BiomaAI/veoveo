BEGIN TRANSACTION;
  LET $now = time::now();
    LET $revision_current = fn::kernel::gateway::control_revision_current_v1($authority_revision, $authority_revision_id, $authority_sha256);
    LET $tenant = fn::kernel::identity::tenant_current_v1($authority_enterprise, $authority_tenant);
  LET $source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_source);
  LET $actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_actor);
  IF $authority_expires_at <= $now
  OR !$revision_current
  OR $tenant = NONE
  OR $source = NONE
  OR $actor = NONE
  OR $source.kind != $grantee_kind
  OR $source.issuer != $grantee_issuer
  OR $source.subject != $grantee_subject
  OR $actor.kind != $owner_kind
  OR $actor.issuer != $owner_issuer
  OR $actor.subject != $owner_subject {
    THROW "computer_authority_expired";
  };
  IF $family != NONE {
    LET $family_state = fn::kernel::gateway::refresh_family_current_v1($family, $now);
    IF $family_state = NONE
    OR $family_state.expires_at <= $now {
      THROW "computer_authority_expired";
    };
  };
  LET $limits = SELECT * FROM ONLY $policy;
  LET $granted = SELECT VALUE computer
  FROM $grants
  WHERE
    expires_at > $now
    AND grant.revision = revision
    AND grant.revoked_at = NONE
    AND grant.expires_at > $now
    AND 'read' IN grant.permissions
    AND grant.computer_id = computer.computer_id
    AND grant.owner_key = computer.owner_key
    AND grant.provider_instance_id = $provider
    AND grant.grantee = $grantee
    AND grant.principal_id = $grantee_principal
    AND grant.grantee_kind = $grantee_kind
    AND grant.grantee_issuer = $grantee_issuer
    AND grant.grantee_subject = $grantee_subject
    AND grant.oauth_client_id = $client
    AND grant.authority.invocation.tenant = $grantee_tenant
    AND grant.authority.invocation.work_context = $context
    AND grant.authority.profile = $profile
    AND grant.authority.actor.id = computer.owner_context.principal_key
    AND grant.authority.actor.kind = computer.owner_context.principal_kind
    AND grant.authority.actor.issuer = computer.owner_context.issuer
    AND grant.authority.actor.subject = computer.owner_context.subject
    AND fn::kernel::identity::principal_current_v1($authority_tenant, owner_source) != NONE
    AND fn::kernel::identity::principal_current_v1($authority_tenant, owner_actor) != NONE
    AND $limits.max_grants > 0
    AND $limits.fingerprint = policy_fingerprint;
  LET $admitted = SELECT *
  FROM computer
  WHERE
    provider_instance_id = $provider
    AND (
      (
        computer_id IN $owned
        AND tenant_key = $owner_tenant
        AND owner_context.authority.tenant = $owner_tenant
        AND (
          owner_context.tenant_key ?? 'installation'
        ) = $owner_tenant
        AND owner_context.principal_key = $owner_principal
        AND owner_context.principal_kind = $owner_kind
        AND owner_context.issuer = $owner_issuer
        AND owner_context.subject = $owner_subject
        AND owner_context.authority.work_context = $owner_context
        AND owner_context.data_labels ALLINSIDE $owner_labels
        AND type::is_object(owner_context.authority)
  AND type::is_object(owner_context.authority.output_policy)
  AND (
    owner_context.authority.output_policy.data_labels
  ) ALLINSIDE $owner_labels
        AND (
          owner_context.authority.output_policy.classification = NONE
          OR owner_context.authority.output_policy.classification = NULL
          OR owner_context.authority.output_policy.classification IN $owner_labels
        )
      )
      OR (
        id IN $granted
        AND tenant_key = $grantee_tenant
        AND owner_context.data_labels ALLINSIDE $grantee_labels
        AND type::is_object(owner_context.authority)
  AND type::is_object(owner_context.authority.output_policy)
  AND (
    owner_context.authority.output_policy.data_labels
  ) ALLINSIDE $grantee_labels
        AND (
          owner_context.authority.output_policy.classification = NONE
          OR owner_context.authority.output_policy.classification = NULL
          OR owner_context.authority.output_policy.classification IN $grantee_labels
        )
      )
    )
  ORDER BY computer_id ASC
  LIMIT $limit;
COMMIT TRANSACTION;
RETURN $admitted;
