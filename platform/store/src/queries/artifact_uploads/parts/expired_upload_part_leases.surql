SELECT * FROM artifact_upload_part WHERE lease_owner != NONE AND lease_until <= $now LIMIT $limit;
