BEGIN TRANSACTION;
LET $rows = SELECT * FROM platform_module_migration WHERE module = $module;
FOR $row IN $rows {
    DELETE ONLY $row.id;
    CREATE ONLY $row.id CONTENT {
        module: $row.module,
        version: $row.version,
        name: $row.name,
        filename: $row.filename,
        checksum: string::repeat('0', 64),
        requires_checksum: $row.requires_checksum,
        requirements: $row.requirements
    };
};
COMMIT TRANSACTION;
