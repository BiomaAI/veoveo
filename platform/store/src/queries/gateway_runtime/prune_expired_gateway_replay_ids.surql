DELETE gateway_replay_id WHERE kind = $kind AND expires_at <= $now RETURN BEFORE;
