BEGIN TRANSACTION;
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $policy = SELECT * FROM ONLY $capacity;
  IF $policy = NONE { THROW "computer_policy_conflict"; };
  LET $owners = SELECT * FROM ONLY $owner_usage;
  LET $tenants = SELECT * FROM ONLY $tenant_usage;
  LET $providers = SELECT * FROM ONLY $provider_usage;
  IF ($owners.retained ?? 0) >= $policy.per_owner
    OR ($tenants.retained ?? 0) >= $policy.per_tenant
    OR ($providers.retained ?? 0) >= $policy.provider { THROW "computer_capacity"; };
  -- Writing the policy makes concurrent reductions conflict with old admission.
  UPDATE $capacity SET admissions += 1;
  UPSERT $owner_usage SET retained = ($owners.retained ?? 0) + 1;
  UPSERT $tenant_usage SET retained = ($tenants.retained ?? 0) + 1;
  UPSERT $provider_usage SET retained = ($providers.retained ?? 0) + 1;
  CREATE $computer CONTENT $content;
  CREATE $request CONTENT {computer: $computer, fingerprint: $fingerprint};
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
