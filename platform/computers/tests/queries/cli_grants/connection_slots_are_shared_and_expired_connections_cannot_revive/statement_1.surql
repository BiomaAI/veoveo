UPDATE computer_cli_connection SET expires_at = time::now() - 1s WHERE grant_id = $grant;
