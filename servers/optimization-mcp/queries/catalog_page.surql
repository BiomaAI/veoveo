SELECT * FROM (/* selection */)
WHERE (/* visible */) AND (/* completed */)
/* after cursor */
ORDER BY created_at ASC, task ASC LIMIT $limit;
