DEFINE TABLE own; DEFINE FIELD method ON own TYPE string ASSERT bytes::len(<bytes><string>(DELETE own)) > 0;
