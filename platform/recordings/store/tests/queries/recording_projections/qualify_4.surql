DEFINE EVENT projection_failure ON recording_projection_receipt WHEN $event = 'UPDATE' AND $after.state = 'materializing' THEN { THROW 'injected_projection_failure'; };
