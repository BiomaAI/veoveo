SELECT * FROM artifact_upload_part WHERE upload = $upload AND part_number > $after AND (!$accepted_only OR state = 'accepted') ORDER BY part_number ASC LIMIT $limit;
