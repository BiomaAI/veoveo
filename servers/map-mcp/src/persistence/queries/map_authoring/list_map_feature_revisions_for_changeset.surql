SELECT * FROM map_feature_revision WHERE tenant = $tenant AND work_context = $context AND changeset_key = $changeset ORDER BY feature_key ASC;
