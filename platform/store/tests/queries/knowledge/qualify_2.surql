SELECT VALUE id FROM knowledge_member WHERE generation = $generation;
SELECT VALUE id FROM knowledge_embedding_batch WHERE generation = $generation ORDER BY id;
SELECT * FROM knowledge_embedding_profile ORDER BY id;
SELECT * FROM knowledge_embedding_qualification ORDER BY id;
