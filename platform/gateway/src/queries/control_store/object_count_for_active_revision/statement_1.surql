SELECT VALUE count FROM (SELECT count() AS count FROM gateway_control_object WHERE revision = $revision GROUP ALL);
