BEGIN TRANSACTION;
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $policy = SELECT * FROM ONLY $capacity;
  IF $policy = NONE { THROW "computer_policy_conflict"; };
  LET $policy_guard = (UPDATE ONLY $capacity SET admissions += 1
    WHERE fingerprint = $policy.fingerprint RETURN AFTER);
  IF $policy_guard = NONE { THROW "computer_policy_conflict"; };
  LET $owners = (UPSERT ONLY $owner_usage SET retained = (retained ?? 0) + 1
    WHERE (retained ?? 0) < $policy.per_owner RETURN AFTER);
  IF $owners = NONE { THROW "computer_capacity"; };
  LET $tenants = (UPSERT ONLY $tenant_usage SET retained = (retained ?? 0) + 1
    WHERE (retained ?? 0) < $policy.per_tenant RETURN AFTER);
  IF $tenants = NONE { THROW "computer_capacity"; };
  LET $providers = (UPSERT ONLY $provider_usage SET retained = (retained ?? 0) + 1
    WHERE (retained ?? 0) < $policy.provider RETURN AFTER);
  IF $providers = NONE { THROW "computer_capacity"; };
  CREATE $computer CONTENT $content;
  CREATE $request CONTENT {computer: $computer, fingerprint: $fingerprint};
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
