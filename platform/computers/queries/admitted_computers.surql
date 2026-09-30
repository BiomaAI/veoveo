BEGIN TRANSACTION;
  LET $now = time::now();
  LET $head = SELECT * FROM ONLY gateway_control_active:current;
  LET $revision = SELECT * FROM ONLY $authority_revision;
  LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
  LET $tenant = SELECT * FROM ONLY $authority_tenant;
  LET $source = SELECT * FROM ONLY $authority_source;
  LET $actor = SELECT * FROM ONLY $authority_actor;
  IF $authority_expires_at <= $now
  OR $head = NONE
  OR $head.revision != $authority_revision
  OR $head.revision_id != $authority_revision_id
  OR $revision = NONE
  OR $revision.sha256 != $authority_sha256
  OR $enterprise.enabled != true
  OR $tenant.enabled != true
  OR $source.enabled != true
  OR $actor.enabled != true
  OR $source.tenant != $authority_tenant
  OR $source.kind != $grantee_kind
  OR $source.issuer != $grantee_issuer
  OR $source.subject != $grantee_subject
  OR $actor.tenant != $authority_tenant
  OR $actor.kind != $owner_kind
  OR $actor.issuer != $owner_issuer
  OR $actor.subject != $owner_subject {
    THROW "computer_authority_expired";
  };
  IF $family != NONE {
    LET $family_state = SELECT * FROM ONLY $family;
    IF $family_state = NONE
    OR $family_state.revoked_at != NONE
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
    AND owner_source.enabled = true
    AND owner_actor.enabled = true
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
        AND (
          owner_context.authority.output_policy.data_labels ?? []
        ) ALLINSIDE $owner_labels
        AND (
          owner_context.authority.output_policy.classification = NONE
          OR owner_context.authority.output_policy.classification IN $owner_labels
        )
      )
      OR (
        id IN $granted
        AND tenant_key = $grantee_tenant
        AND owner_context.data_labels ALLINSIDE $grantee_labels
        AND (
          owner_context.authority.output_policy.data_labels ?? []
        ) ALLINSIDE $grantee_labels
        AND (
          owner_context.authority.output_policy.classification = NONE
          OR owner_context.authority.output_policy.classification IN $grantee_labels
        )
      )
    )
  ORDER BY computer_id ASC
  LIMIT $limit;
COMMIT TRANSACTION;
RETURN $admitted;
