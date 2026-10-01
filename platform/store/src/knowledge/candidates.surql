SELECT
  tenant,
  collection_id,
  uri,
  ordinal,
  text,
  admission,
  observation
FROM __TABLE__
WHERE
  __ADMISSION__
  AND (
    $after_collection = NONE
    OR collection_id > $after_collection
    OR (
      collection_id = $after_collection
      AND uri > $after_uri
    )
    OR (
      collection_id = $after_collection
      AND uri = $after_uri
      AND ordinal > $after_ordinal
    )
  )
ORDER BY collection_id ASC, uri ASC, ordinal ASC
LIMIT $limit;
