SELECT * FROM artifact_upload WHERE (state IN ['open', 'finalizing', 'verifying'] OR cleanup_pending) AND ($after = NONE OR id > $after) ORDER BY id ASC LIMIT $limit;
