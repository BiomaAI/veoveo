UPDATE $artifact SET retention_expires_at = time::now() - 1s; UPDATE artifact_grant SET expires_at = time::now() - 1s WHERE in = $granted AND subject_key = 'bob';
