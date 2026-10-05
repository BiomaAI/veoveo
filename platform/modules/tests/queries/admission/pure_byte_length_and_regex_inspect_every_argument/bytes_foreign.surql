DEFINE TABLE own; DEFINE FIELD method ON own TYPE string ASSERT bytes::len(<bytes><string>(SELECT * FROM foreign)) > 0;
