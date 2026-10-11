DELETE (SELECT VALUE id FROM uav_live_view WHERE closed_at != NONE AND closed_at <= $now - 72h AND expires_at < $now LIMIT 1000);
