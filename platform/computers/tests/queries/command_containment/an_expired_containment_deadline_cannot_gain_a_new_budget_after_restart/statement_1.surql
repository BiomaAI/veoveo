LET $now=time::now(); UPDATE computer_execution SET created_at=$now-300s, dispatched_at=$now-299s, execution_deadline=$now-270s, containment_started_at=$now-240s, containment_deadline=$now-60s;
