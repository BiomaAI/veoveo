SELECT * FROM recording WHERE tenant = $tenant AND $clearance CONTAINSALL labels
             AND ($after_id = NONE OR started_at < $after_start
                 OR (started_at = $after_start AND id < $after_id))
             ORDER BY started_at DESC, id DESC LIMIT $limit;
