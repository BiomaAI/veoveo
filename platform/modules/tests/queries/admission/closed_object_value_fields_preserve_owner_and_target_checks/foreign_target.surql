DEFINE TABLE own; DEFINE FIELD value ON own TYPE { child: string } ASSERT $value.child = <string>(SELECT * FROM foreign);
