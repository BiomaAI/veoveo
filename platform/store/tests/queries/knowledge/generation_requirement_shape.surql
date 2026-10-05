LET $row = SELECT * FROM ONLY $generation;
CREATE ONLY $probe CONTENT object::extend($row, { id: $probe, collections: $requirements });
