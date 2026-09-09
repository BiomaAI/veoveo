use crate::{Result, RuntimeFailure, canonical, protocol::sandbox::v1::SandboxPolicy};
use prost_types::{ListValue, Struct, Value, value::Kind};
use std::collections::BTreeSet;
use uuid::Uuid;
pub const PERSISTENT_HOME: &str = "/sandbox/persistent";
pub const PERSISTENT_COMMAND: [&str; 4] = [
    "/usr/bin/env",
    "HOME=/sandbox/persistent",
    "/bin/bash",
    "-l",
];
pub const PERSISTENT_BUILD_COMMAND: [&str; 4] = [
    "/usr/bin/env",
    "HOME=/sandbox/persistent",
    "/bin/sleep",
    "infinity",
];
pub(crate) fn object<const N: usize>(fields: [(&str, Value); N]) -> Struct {
    Struct {
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}
pub(crate) fn string_value(s: impl Into<String>) -> Value {
    Value {
        kind: Some(Kind::StringValue(s.into())),
    }
}
pub(crate) fn struct_value(s: Struct) -> Value {
    Value {
        kind: Some(Kind::StructValue(s)),
    }
}
fn number(n: u64) -> Value {
    Value {
        kind: Some(Kind::NumberValue(n as f64)),
    }
}
fn list(values: Vec<Value>) -> Value {
    Value {
        kind: Some(Kind::ListValue(ListValue { values })),
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentHome {
    capacity_mib: u32,
    temporary_mib: u32,
}
impl PersistentHome {
    pub fn new(capacity_mib: u32, temporary_mib: u32) -> Result<Self> {
        if !(512..=262144).contains(&capacity_mib) || !(16..=4096).contains(&temporary_mib) {
            return Err(RuntimeFailure::InvalidTemplate);
        }
        Ok(Self {
            capacity_mib,
            temporary_mib,
        })
    }
    pub fn capacity_mib(&self) -> u32 {
        self.capacity_mib
    }
    pub fn temporary_mib(&self) -> u32 {
        self.temporary_mib
    }
    pub fn volume_name(id: Uuid) -> Result<String> {
        if id.is_nil() {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(format!("veoveo-computer-{}", id.simple()))
    }
    pub(crate) fn validate(
        &self,
        policy: &SandboxPolicy,
        command: &[String],
        memory: u32,
    ) -> Result<()> {
        let fs = policy
            .filesystem
            .as_ref()
            .ok_or(RuntimeFailure::InvalidTemplate)?;
        let paths: BTreeSet<&str> = fs.read_write.iter().map(String::as_str).collect();
        if fs.include_workdir
            || paths.len() != fs.read_write.len()
            || !paths.contains(PERSISTENT_HOME)
            || !paths.contains("/tmp")
            || paths.iter().any(|p| {
                ![PERSISTENT_HOME, "/tmp", "/dev/null", "/dev/pts", "/dev/tty"].contains(p)
            })
            || !fs.read_only.iter().any(|p| p == "/sandbox")
            || (command != PERSISTENT_COMMAND && command != PERSISTENT_BUILD_COMMAND)
            || self.temporary_mib + 32 > memory / 2
        {
            return Err(RuntimeFailure::InvalidTemplate);
        }
        Ok(())
    }
    pub fn driver_config(&self, id: Uuid) -> Result<Struct> {
        Ok(object([(
            "docker",
            struct_value(object([
                (
                    "mounts",
                    list(vec![
                        struct_value(object([
                            ("type", string_value("volume")),
                            ("source", string_value(Self::volume_name(id)?)),
                            ("target", string_value(PERSISTENT_HOME)),
                            (
                                "read_only",
                                Value {
                                    kind: Some(Kind::BoolValue(false)),
                                },
                            ),
                            ("subpath", string_value("home")),
                        ])),
                        struct_value(object([
                            ("type", string_value("tmpfs")),
                            ("target", string_value("/tmp")),
                            (
                                "size_bytes",
                                number(self.temporary_mib as u64 * 1024 * 1024),
                            ),
                            ("mode", number(0o1777)),
                            ("options", list(vec![string_value("exec")])),
                        ])),
                    ]),
                ),
                (
                    "log_limits",
                    struct_value(object([
                        ("max_file_bytes", number(10 * 1024 * 1024)),
                        ("max_files", number(3)),
                        ("supervisor_tmpfs_bytes", number(32 * 1024 * 1024)),
                    ])),
                ),
            ])),
        )]))
    }
    pub(crate) fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = b"veoveo.io/computer-persistent-home/v1\0".to_vec();
        bytes.extend(self.capacity_mib.to_be_bytes());
        bytes.extend(canonical::encode(
            &self
                .driver_config(Uuid::from_u128(1))
                .expect("fixed hash identity"),
            ".google.protobuf.Struct",
        ));
        bytes
    }
}
