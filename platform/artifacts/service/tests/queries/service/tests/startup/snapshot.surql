RETURN [
    (SELECT * FROM platform_module_lane ORDER BY id),
    (SELECT * FROM platform_module_migration ORDER BY id),
    (SELECT * FROM platform_module_installation ORDER BY id),
    (SELECT * FROM artifact_access_request ORDER BY id),
    (SELECT * FROM artifact_blob ORDER BY id),
    (SELECT * FROM artifact_derived_from ORDER BY id),
    (SELECT * FROM artifact_grant ORDER BY id),
    (SELECT * FROM artifact_occurrence ORDER BY id),
    (SELECT * FROM artifact_read_capability ORDER BY id),
    (SELECT * FROM artifact_storage_usage ORDER BY id),
    (SELECT * FROM artifact_upload ORDER BY id),
    (SELECT * FROM artifact_upload_memory ORDER BY id),
    (SELECT * FROM artifact_upload_part ORDER BY id),
    (SELECT * FROM artifact_write_capability ORDER BY id),
    (SELECT * FROM artifact_write_redemption ORDER BY id),
    (SELECT * FROM share_link ORDER BY id)
];
