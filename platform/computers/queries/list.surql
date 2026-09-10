SELECT *
FROM computer
WHERE
  owner_key = $owner
  AND (
    $after = NONE
    OR computer_id > $after
  )
ORDER BY computer_id ASC
LIMIT $limit;
