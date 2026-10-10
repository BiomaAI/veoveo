//! Source-byte ranges preserve the digest binding through chunk admission.
use std::ops::Range;
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_contract::{ChunkSettings, KnowledgeError};

pub const VERSION: &str = "structure-v1";

/// Splits at Markdown headings or top-level JSON fields, then applies character
/// and UTF-8 byte caps. Overlap never crosses a structural section boundary.
pub fn ranges(text: &str, settings: &ChunkSettings) -> Result<Vec<Range<usize>>, KnowledgeError> {
    if text.len() > veoveo_knowledge_contract::MAX_SOURCE_MEMBER_BYTES {
        return Err(KnowledgeError("source exceeds 256 KiB"));
    }
    let boundaries = sections(text);
    let mut output = Vec::new();
    for section in boundaries.windows(2) {
        let mut start = section[0];
        let stop = section[1];
        while start < stop {
            let mut end = start;
            let mut chars = 0;
            for (offset, ch) in text[start..stop].char_indices() {
                let next = start + offset + ch.len_utf8();
                if chars == settings.max_characters() || next - start > EmbeddingText::MAX_BYTES {
                    break;
                }
                end = next;
                chars += 1;
            }
            if end == start {
                return Err(KnowledgeError("chunk cap cannot advance"));
            }
            if !text[start..end].trim().is_empty() {
                output.push(start..end);
            }
            if output.len() > 256 {
                return Err(KnowledgeError("source requires more than 256 chunks"));
            }
            if end == stop {
                break;
            }
            // Retain at most cap-1 characters; a byte cap can bind before the
            // configured character cap for non-ASCII sources.
            let overlap = settings.overlap_characters().min(chars.saturating_sub(1));
            start = text[start..end]
                .char_indices()
                .rev()
                .nth(overlap.saturating_sub(1) as usize)
                .filter(|_| overlap > 0)
                .map_or(end, |(offset, _)| start + offset);
        }
    }
    if output.is_empty() {
        return Err(KnowledgeError("source contains no indexable text"));
    }
    Ok(output)
}

fn sections(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    if serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| v.is_object()) {
        let (mut depth, mut quoted, mut escaped) = (0_u32, false, false);
        for (index, byte) in text.bytes().enumerate() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
            } else {
                match byte {
                    b'"' => quoted = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => depth -= 1,
                    b',' if depth == 1 => offsets.push(index + 1),
                    _ => {}
                }
            }
        }
    } else {
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let hashes = line.bytes().take_while(|b| *b == b'#').count();
            if offset > 0 && (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ')
            {
                offsets.push(offset);
            }
            offset += line.len();
        }
    }
    offsets.push(text.len());
    offsets
}
