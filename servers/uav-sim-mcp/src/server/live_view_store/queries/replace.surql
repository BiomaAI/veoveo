UPDATE $key CONTENT $grant WHERE generation = $generation AND issuer = $issuer AND closed_at = NONE RETURN AFTER;
