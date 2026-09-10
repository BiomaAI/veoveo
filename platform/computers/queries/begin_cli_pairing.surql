BEGIN TRANSACTION;
LET $now = time::now();
LET $family_state = SELECT * FROM ONLY $family;
LET $current = SELECT * FROM ONLY $computer;
IF $current = NONE OR $current.owner_key != $owner_key { THROW "computer_not_found"; };
IF $admission_expires_at <= $now OR $family_state = NONE OR $family_state.revoked_at != NONE
  OR $family_state.expires_at <= $now { THROW "computer_authority_expired"; };
UPSERT $guard SET revision = (revision ?? 0) + 1;
LET $recent = SELECT VALUE pairing_id FROM computer_cli_pairing
  WHERE owner_key = $owner_key AND issued_at > $now - 1m LIMIT 5;
IF array::len($recent) >= 5 { THROW "computer_access_limit"; };
LET $replayed = SELECT VALUE pairing_id FROM computer_cli_pairing
  WHERE owner_key = $owner_key AND computer_id = $computer_id AND code_hash = $code_hash LIMIT 1;
IF array::len($replayed) > 0 { THROW "computer_authority_expired"; };
CREATE $pairing CONTENT {pairing_id:$pairing_id, computer_id:$computer_id, owner_key:$owner_key,
  family:$family, binding_hash:$binding_hash, code_hash:$code_hash, name:$name,
  callback_port:$callback_port, issued_at:$now, expires_at:$now + 120s};
COMMIT TRANSACTION;
SELECT VALUE expires_at FROM ONLY $pairing;
