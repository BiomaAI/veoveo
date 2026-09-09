//! Hash compatibility with Python protobuf's deterministic string-map order.
//! Field shapes come exclusively from the descriptor generated from vendored
//! originals. This orders encoded fields; it defines no provider wire models.
use prost::Message;
use prost_types::{DescriptorProto, FileDescriptorSet};
use std::{collections::BTreeMap, sync::LazyLock};

static SCHEMA: LazyLock<BTreeMap<String, DescriptorProto>> = LazyLock::new(|| {
    fn walk(
        parent: &str,
        items: Vec<DescriptorProto>,
        out: &mut BTreeMap<String, DescriptorProto>,
    ) {
        for message in items {
            let name = format!("{parent}.{}", message.name.as_deref().unwrap());
            walk(&name, message.nested_type.clone(), out);
            out.insert(name, message);
        }
    }
    let set = FileDescriptorSet::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/openshell.bin")).as_slice(),
    )
    .expect("build-generated descriptor");
    let mut out = BTreeMap::new();
    for file in set.file {
        walk(
            &format!(".{}", file.package.unwrap()),
            file.message_type,
            &mut out,
        );
    }
    out
});

fn varint(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let b = bytes[*offset];
        *offset += 1;
        value |= ((b & 127) as u64) << shift;
        if b < 128 {
            return value;
        }
    }
    unreachable!("only generated prost encodings enter canonicalization")
}
fn push_varint(mut n: u64, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push((n as u8) | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
struct Field {
    number: i32,
    wire: u8,
    bytes: Vec<u8>,
    key: Option<Vec<u8>>,
}
fn fields(bytes: &[u8]) -> Vec<Field> {
    let mut offset = 0;
    let mut out = Vec::new();
    while offset < bytes.len() {
        let tag = varint(bytes, &mut offset);
        let wire = (tag & 7) as u8;
        let start = offset;
        let data = match wire {
            0 => {
                varint(bytes, &mut offset);
                bytes[start..offset].to_vec()
            }
            1 | 5 => {
                offset += if wire == 1 { 8 } else { 4 };
                bytes[start..offset].to_vec()
            }
            2 => {
                let len = varint(bytes, &mut offset) as usize;
                let start = offset;
                offset += len;
                bytes[start..offset].to_vec()
            }
            _ => unreachable!("generated protocol has no groups"),
        };
        out.push(Field {
            number: (tag >> 3) as i32,
            wire,
            bytes: data,
            key: None,
        });
    }
    out
}
pub(crate) fn encode<M: Message>(message: &M, name: &str) -> Vec<u8> {
    reorder(&message.encode_to_vec(), name)
}
fn reorder(bytes: &[u8], name: &str) -> Vec<u8> {
    let schema = &SCHEMA[name];
    let mut items = fields(bytes);
    for item in &mut items {
        let descriptor = schema
            .field
            .iter()
            .find(|f| f.number == Some(item.number))
            .expect("generated field");
        if descriptor.r#type == Some(11) {
            let target = descriptor.type_name.as_deref().unwrap();
            if SCHEMA[target]
                .options
                .as_ref()
                .is_some_and(|o| o.map_entry())
            {
                // All maps reachable from SandboxSpec have string keys. upb
                // compares UTF-8 bytes, treating end-of-key as greater than a
                // following byte: longer shared prefixes precede shorter ones.
                item.key = Some(
                    fields(&item.bytes)
                        .into_iter()
                        .find(|f| f.number == 1)
                        .map_or_else(Vec::new, |f| f.bytes),
                );
            }
            item.bytes = reorder(&item.bytes, target);
        }
    }
    items.sort_by(|a, b| {
        a.number
            .cmp(&b.number)
            .then_with(|| match (&a.key, &b.key) {
                (Some(a), Some(b)) => a
                    .iter()
                    .zip(b.iter())
                    .map(|(a, b)| a.cmp(b))
                    .find(|x| !x.is_eq())
                    .unwrap_or_else(|| b.len().cmp(&a.len())),
                _ => std::cmp::Ordering::Equal,
            })
    });
    let mut out = Vec::with_capacity(bytes.len());
    for item in items {
        push_varint((item.number as u64) << 3 | item.wire as u64, &mut out);
        if item.wire == 2 {
            push_varint(item.bytes.len() as u64, &mut out);
        }
        out.extend(item.bytes);
    }
    out
}
