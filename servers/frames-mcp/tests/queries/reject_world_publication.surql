DEFINE EVENT fail_world ON frame_world WHEN $event = 'UPDATE' THEN { THROW 'injected publication event failure'; };
