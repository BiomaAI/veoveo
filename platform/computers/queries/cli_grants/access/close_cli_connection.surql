SELECT * FROM ONLY $grant WHERE grant_id = $grant_id
                AND provider_instance_id = $provider AND $connection.connection_id = $connection_id
                AND $connection.grant_id = $grant_id AND $connection.closed_at = NONE;
