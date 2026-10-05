SELECT * FROM gateway_task_route WHERE server = $server AND source_task_id = $source
             AND owner = $owner AND profile = $profile LIMIT 1;
