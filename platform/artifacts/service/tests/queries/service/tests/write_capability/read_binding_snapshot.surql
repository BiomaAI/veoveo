SELECT * FROM ONLY $capability;
SELECT * FROM artifact_write_redemption WHERE capability = $capability ORDER BY id;
SELECT VALUE id FROM artifact_occurrence ORDER BY id;
