BEGIN TRANSACTION; LET $old = SELECT * FROM ONLY $definition;
            IF $old = NONE { CREATE ONLY $definition SET tenant = $tenant, work_context = $context, owner = $owner,
                key = $key, name = $key, description = 'Store fixture', revision = 1, status = 'enabled', disabled = false,
                audience = [], draft = $content, draft_digest = $digest; };
            LET $old_revision = SELECT * FROM ONLY $revision;
            IF $old_revision = NONE { CREATE ONLY $revision SET definition = $definition, digest = $digest, content = $content, created_by = $owner; };
            UPDATE ONLY $definition SET published = $revision, audience = array::union(audience, [$context]); COMMIT TRANSACTION;
