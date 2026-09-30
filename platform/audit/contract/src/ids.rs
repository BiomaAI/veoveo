use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid audit identity: {0}")]
pub struct AuditIdentityError(&'static str);
macro_rules! uuid_id {
 ($name:ident) => {
  #[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Hash,Serialize,Deserialize)]
  #[serde(try_from="String",into="String")]
  pub struct $name(Uuid);
  impl JsonSchema for $name {
   fn schema_name()->std::borrow::Cow<'static,str>{std::borrow::Cow::Borrowed(stringify!($name))}
   fn json_schema(_: &mut schemars::SchemaGenerator)->schemars::Schema {
    schemars::json_schema!({"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"})
   }
  }
  impl $name {
   pub fn new()->Self {Self(Uuid::now_v7())}
   pub fn as_uuid(self)->Uuid {self.0}
  }
  impl Default for $name {fn default()->Self {Self::new()}}
  impl TryFrom<Uuid> for $name {
   type Error=AuditIdentityError;
   fn try_from(value:Uuid)->Result<Self,Self::Error>{
    if value.get_version_num()!=7 || value.get_variant()!=uuid::Variant::RFC4122 {return Err(AuditIdentityError(stringify!($name)))}
    Ok(Self(value))
   }
  }
  impl FromStr for $name {
   type Err=AuditIdentityError;
   fn from_str(s:&str)->Result<Self,Self::Err>{
    let value=Uuid::parse_str(s).map_err(|_|AuditIdentityError(stringify!($name)))?;
    if value.to_string()!=s{return Err(AuditIdentityError(stringify!($name)))}
    Self::try_from(value)
   }
  }
  impl TryFrom<String> for $name {type Error=AuditIdentityError;fn try_from(value:String)->Result<Self,Self::Error>{value.parse()}}
  impl From<$name> for String {fn from(value:$name)->Self{value.to_string()}}
  impl fmt::Display for $name {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{self.0.fmt(f)}}
 };
}
uuid_id!(AuditRecordId);
uuid_id!(AuditRequestId);
uuid_id!(AuditEpisodeId);

macro_rules! hex_id {
 ($name:ident,$length:expr) => {
  #[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord,Hash,Serialize,Deserialize)]
  #[serde(try_from="String",into="String")]
  pub struct $name(String);
  impl JsonSchema for $name {
   fn schema_name()->std::borrow::Cow<'static,str>{std::borrow::Cow::Borrowed(stringify!($name))}
   fn json_schema(_: &mut schemars::SchemaGenerator)->schemars::Schema {
    schemars::json_schema!({"type":"string","pattern":format!("^[0-9a-f]{{{}}}$",$length),"not":{"const":"0".repeat($length)}})
   }
  }
  impl $name {
   pub fn as_str(&self)->&str{&self.0}
   pub fn parse(value:impl Into<String>)->Result<Self,AuditIdentityError>{
    let value=value.into();
    if value.len()!=$length || !value.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c)) || value.bytes().all(|c|c==b'0') {return Err(AuditIdentityError(stringify!($name)))}
    Ok(Self(value))
   }
  }
  impl TryFrom<String> for $name {type Error=AuditIdentityError;fn try_from(v:String)->Result<Self,Self::Error>{Self::parse(v)}}
  impl FromStr for $name {type Err=AuditIdentityError;fn from_str(v:&str)->Result<Self,Self::Err>{Self::parse(v)}}
  impl From<$name> for String {fn from(v:$name)->Self{v.0}}
  impl fmt::Display for $name {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(&self.0)}}
 };
}
hex_id!(AuditTraceId, 32);
hex_id!(AuditSpanId, 16);
