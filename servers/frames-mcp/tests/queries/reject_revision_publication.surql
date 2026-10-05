DEFINE EVENT fail_world ON frame_world_revision WHEN $event = 'CREATE' THEN { THROW 'injected publication event failure'; };
