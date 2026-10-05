SELECT * FROM principal WHERE tenant = $tenant ORDER BY display_name ASC LIMIT $limit;
SELECT * FROM task WHERE tenant = $tenant ORDER BY updated_at DESC LIMIT $limit;
SELECT * FROM artifact_occurrence WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
SELECT * FROM share_link WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
SELECT * FROM artifact_grant WHERE in IN (SELECT VALUE id FROM artifact_occurrence WHERE tenant = $tenant) LIMIT $limit;
SELECT * FROM agent WHERE tenant = $tenant ORDER BY updated_at DESC LIMIT $limit;
SELECT * FROM wake WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
SELECT * FROM recording WHERE tenant = $tenant ORDER BY started_at DESC LIMIT $limit;
SELECT * FROM recording_layer
    WHERE tenant = $tenant
    AND recording IN (
        SELECT VALUE id FROM recording
        WHERE tenant = $tenant
        ORDER BY started_at DESC
        LIMIT $limit
    )
    ORDER BY created_at DESC
    LIMIT $layer_limit;
