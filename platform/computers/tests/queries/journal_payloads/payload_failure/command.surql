DEFINE EVENT qualification_failure ON computer_execution_payload WHEN $event = 'CREATE' THEN { THROW 'qualification_payload_failure'; };
