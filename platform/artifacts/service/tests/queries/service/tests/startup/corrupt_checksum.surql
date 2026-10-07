BEGIN TRANSACTION;
DELETE ONLY $row.id;
CREATE ONLY $row.id CONTENT object::extend($row, { checksum: string::repeat('0', 64) });
COMMIT TRANSACTION;
