SELECT * FROM ONLY $pairing WHERE computer_id = $computer_id AND owner_key = $owner_key
                     AND family = $family AND binding_hash = $binding_hash
                     AND consumed_at = NONE AND expires_at > time::now(); SELECT * FROM ONLY $policy;
