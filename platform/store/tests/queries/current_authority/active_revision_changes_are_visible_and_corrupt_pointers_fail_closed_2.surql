UPDATE gateway_control_active:current SET revision = $revision, revision_id = $name, updated_at = time::now();
