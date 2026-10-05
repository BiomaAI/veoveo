UPDATE ONLY $task SET lease_owner = 'replacement', lease_expires_at = time::now() - 1s;
