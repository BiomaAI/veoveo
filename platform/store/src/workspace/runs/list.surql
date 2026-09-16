RETURN SELECT * FROM workspace_run WHERE chat = $command ORDER BY sequence DESC LIMIT 64;
