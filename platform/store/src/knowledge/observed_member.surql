SELECT VALUE true FROM knowledge_member
            WHERE tenant = $tenant AND generation = $active.generation
                AND collection = $collection AND collection.approved = true
                AND collection.revision = $revision AND uri = $uri
                AND deleted = false AND stale = false AND observation != NONE
            LIMIT 1;
