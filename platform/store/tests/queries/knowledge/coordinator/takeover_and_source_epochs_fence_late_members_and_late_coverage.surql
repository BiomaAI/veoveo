UPDATE knowledge_coordinator SET expires_at = time::now() - 1s;
