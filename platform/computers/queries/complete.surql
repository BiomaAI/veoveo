SELECT * FROM computer
WHERE owner_key = $owner AND string::starts_with(<string>computer_id, $prefix)
ORDER BY computer_id ASC
LIMIT 101;
