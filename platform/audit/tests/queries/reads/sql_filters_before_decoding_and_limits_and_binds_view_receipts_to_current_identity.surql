LET $template = (SELECT * FROM audit_record WHERE partition = 'tenant:alpha' AND trace_id = $trace LIMIT 1)[0];
LET $uuid = rand::uuid::v7();
LET $id = type::record('audit_record', ['tenant:alpha', $uuid]);
CREATE ONLY $id CONTENT object::extend($template, {
    id: $id, record_id: $uuid, class: 'authentication', activity: 'login',
    request_id: rand::uuid::v7(), occurred_at: time::now(), draft: {}
});
