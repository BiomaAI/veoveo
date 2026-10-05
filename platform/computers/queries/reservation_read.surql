RETURN {
  LET $receipt = SELECT computer FROM ONLY $request;
  IF $receipt = NONE { RETURN NONE; };
  LET $admitted = SELECT * FROM ONLY $receipt.computer
  WHERE provider_instance_id = $provider AND
    tenant_key = $owner_tenant
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
    );
  IF $admitted = NONE { THROW "computer_not_found"; };
  $admitted;
};
