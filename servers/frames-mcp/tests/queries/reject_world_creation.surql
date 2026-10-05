DEFINE EVENT fail_world ON frame_world WHEN $event = 'CREATE' THEN { THROW 'injected creation event failure'; };
