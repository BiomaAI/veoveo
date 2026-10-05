BEGIN TRANSACTION;
FOR $record IN $records {
    CREATE ONLY $record.id CONTENT $record;
};
COMMIT TRANSACTION;
