BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $next, request = $request, progress = $progress, result = $result, error = $error, cancel_requested_at = $cancel_requested_at, completed_at = $completed_at, lease_owner = IF $terminal { NONE } ELSE { lease_owner }, lease_expires_at = IF $terminal { NONE } ELSE { lease_expires_at }, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND server = $server AND tenant = $tenant AND owner = $owner AND ($control_transition OR (lease_owner = $worker AND lease_expires_at > $now) OR ($expired_cancellation AND (lease_expires_at = NONE OR lease_expires_at <= $now))) AND server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels AND work_context = $work_context
                AND authority.context_key = $work_context_key
                AND request.owner.authority.work_context = $work_context_key
                AND request.owner.authority.tenant = $authority_tenant RETURN AFTER); RETURN $updated; COMMIT TRANSACTION;