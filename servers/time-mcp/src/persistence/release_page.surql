SELECT *
FROM time_authority_release
WHERE
  tenant = $tenant
  AND (
    $after = NONE
    OR release_key > $after
  )
ORDER BY release_key ASC
LIMIT 101;
