SELECT VALUE activity FROM audit_record WHERE target_ref = $computer AND activity = 'computer_revoke' AND draft.detail.stage = 'grant_revoked';
