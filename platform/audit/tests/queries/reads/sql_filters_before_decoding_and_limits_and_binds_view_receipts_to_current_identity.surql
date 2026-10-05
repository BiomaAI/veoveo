LET $uuid = rand::uuid::v7();
            CREATE ONLY type::record('audit_record', ['tenant:alpha', $uuid]) SET
                partition = 'tenant:alpha', record_id = $uuid, class = 'authentication',
                activity = 'login', outcome = 'allowed', trace_id = $trace,
                request_id = rand::uuid::v7(), occurred_at = time::now(), draft = {};
