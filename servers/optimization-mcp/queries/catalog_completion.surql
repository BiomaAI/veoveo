SELECT VALUE /* completion field */ FROM (/* selection */)
WHERE (/* visible */) AND (/* completed */)
    AND /* completion field */ CONTAINS $needle
GROUP BY /* completion field */ ORDER BY /* completion field */ ASC LIMIT $limit;
