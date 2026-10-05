BEGIN TRANSACTION;
            LET $prior = (SELECT * FROM ONLY $record);
            IF $prior.revision != $expected AND $prior.revision != $revision { THROW 'knowledge_catalog_revision_changed'; };
            UPSERT $record CONTENT {tenant: $tenant, collection: $collection, enumeration_root: $root, revision: $revision, approved: $approved, document: $document};
            COMMIT TRANSACTION;
