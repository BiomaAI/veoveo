SELECT *
FROM ONLY $grant
WHERE
  computer_id = $computer
  AND owner_key = $owner_key
  AND provider_instance_id = $provider;
