
DEFINE FUNCTION fn::store::first($value: string) -> string { RETURN fn::store::second($value); };
DEFINE FUNCTION fn::store::second($value: string) -> string { RETURN string::lowercase($value); };
DEFINE FUNCTION fn::store::exists($id: record<fixture_marker>) -> bool { RETURN record::exists($id); };
DEFINE TABLE fixture_marker SCHEMAFULL;
DEFINE FIELD state ON fixture_marker TYPE 'queued' | 'running';
DEFINE FIELD tags ON fixture_marker TYPE array<string> VALUE $value.distinct() ASSERT $value.all(|$item: any| string::len($item)>0);
DEFINE FIELD digest ON fixture_marker TYPE string ASSERT $value.len()=64;
DEFINE FIELD sealed ON fixture_marker TYPE {nonce:string, version:int};
DEFINE FIELD target ON fixture_marker TYPE option<record> ASSERT $value=NONE OR record::tb($value)!='';
DEFINE FIELD nonce ON fixture_marker TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD replacement ON fixture_marker TYPE option<uuid> ASSERT $value=NONE OR $value!=$this.nonce;
DEFINE FIELD sequence ON fixture_marker TYPE int DEFAULT 0;
DEFINE TABLE fixture_uuid SCHEMAFULL;
DEFINE FIELD id ON fixture_uuid TYPE uuid DEFAULT rand::uuid();
DEFINE TABLE fixture_relation TYPE RELATION IN fixture_marker OUT fixture_marker ENFORCED;
DEFINE TABLE fixture_daily TYPE ANY SCHEMALESS AS SELECT state, count() AS count FROM fixture_marker GROUP BY state;
DEFINE TABLE fixture_head SCHEMAFULL;
DEFINE FIELD last_sequence ON fixture_head TYPE int;
CREATE fixture_head:current SET last_sequence=0;
DEFINE EVENT advance ON fixture_marker WHEN $after.sequence>0 AND $before.sequence!=$after.sequence THEN {
    LET $head=SELECT * FROM ONLY fixture_head:current;
    IF type::is_object($head) THEN {
        IF $after.sequence<=$head.last_sequence { THROW 'sequence conflict'; };
        UPDATE ONLY fixture_head:current SET last_sequence=$after.sequence;
    } ELSE { THROW 'missing head'; } END;
};
CREATE fixture_marker:one SET state=fn::store::first('QUEUED'), tags=['pin','pin'], digest=crypto::sha256('input'), sealed={nonce:'opaque',version:1}, target=fixture_other:one, sequence=1;
CREATE fixture_uuid;
