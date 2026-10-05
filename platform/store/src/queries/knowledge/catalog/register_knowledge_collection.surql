BEGIN TRANSACTION;
            LET $prior = (SELECT * FROM ONLY $record);
            IF $prior.revision != $expected AND $prior.revision != $row.revision { THROW 'knowledge_catalog_revision_changed'; };
            UPSERT $record CONTENT $row;
            COMMIT TRANSACTION;
