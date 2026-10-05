SELECT VALUE true FROM knowledge_member
            WHERE tenant = $tenant AND generation = $active.generation
                AND collection = $collection AND collection.approved = true
                AND collection.revision = $revision AND uri = $uri
                AND sync.ready = true AND source_epoch = sync.epoch
                AND sync.owner_epoch = sync.coordinator.epoch
                AND (collection.document.descriptor.changeSignal = 'immutable' OR sync.coordinator.expires_at > time::now())
                AND (valid_until = NONE OR valid_until > time::now())
                AND deleted = false AND stale = false AND observation != NONE
            LIMIT 1;
