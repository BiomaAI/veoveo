//! Stable Computer/home identity and an explicitly named replacement instance.
//! This is an identity, not permission to replace or concurrently start compute.
use crate::{Result, RuntimeFailure, models::valid_fingerprint};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    computer_id: Uuid,
    template_fingerprint: String,
    replacement: Option<Uuid>,
}

impl Binding {
    /// The first instance keeps its published name and complete original labels.
    pub fn new(computer_id: Uuid, template_fingerprint: String) -> Result<Self> {
        if !valid_fingerprint(&template_fingerprint) {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(Self {
            computer_id,
            template_fingerprint,
            replacement: None,
        })
    }

    /// A controller supplies one durable instance UUID for every retry.
    /// The original Computer UUID continues to select the home and its owner.
    /// The caller must fence lifecycle work, stop the previous writer, restore
    /// the existing home and preserve policy before admitting this instance.
    pub fn replacement(
        computer_id: Uuid,
        instance_id: Uuid,
        template_fingerprint: String,
    ) -> Result<Self> {
        if computer_id.is_nil() || instance_id.is_nil() || computer_id == instance_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let mut binding = Self::new(computer_id, template_fingerprint)?;
        binding.replacement = Some(instance_id);
        Ok(binding)
    }

    pub fn computer_id(&self) -> Uuid {
        self.computer_id
    }
    pub fn template_fingerprint(&self) -> &str {
        &self.template_fingerprint
    }
    pub fn replacement_instance_id(&self) -> Option<Uuid> {
        self.replacement
    }

    pub fn name(&self) -> String {
        let mut hash = Sha256::new();
        if let Some(instance) = self.replacement {
            hash.update(b"veoveo-computer-instance\0");
            hash.update(self.computer_id.as_bytes());
            hash.update(instance.as_bytes());
        } else {
            hash.update(self.computer_id.as_bytes());
        }
        format!("c{}", base32(&hash.finalize()[..11]))
    }

    pub fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::from([
            ("veoveo-computer".into(), self.computer_id.to_string()),
            (
                "veoveo-template".into(),
                base32(&hex::decode(&self.template_fingerprint).expect("validated hash")),
            ),
        ]);
        if let Some(instance) = self.replacement {
            labels.insert("veoveo-instance".into(), instance.to_string());
        }
        labels
    }

    pub(crate) fn labels_match(&self, observed: &BTreeMap<String, String>) -> bool {
        // Additional provider labels are permitted, but the reserved instance
        // identity must be exact in both directions, including initial absence.
        let expected = self.labels();
        observed.get("veoveo-instance") == expected.get("veoveo-instance")
            && expected
                .iter()
                .all(|(key, value)| observed.get(key) == Some(value))
    }
}

fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let (mut acc, mut bits) = (0u32, 0);
    let mut out = String::new();
    for &byte in bytes {
        acc = (acc << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits != 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}
