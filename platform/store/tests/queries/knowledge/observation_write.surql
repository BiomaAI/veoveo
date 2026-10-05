BEGIN TRANSACTION;
UPDATE knowledge_member SET observation = $observation WHERE generation = $generation AND uri = $uri;
UPDATE type::table($table) SET observation = $observation WHERE uri = $uri;
COMMIT TRANSACTION;
