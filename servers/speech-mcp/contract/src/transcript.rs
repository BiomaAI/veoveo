use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_TRANSCRIPT_BYTES: usize = 512 * 1024;
pub const MAX_SEGMENTS: usize = 10_000;
pub const MAX_WORDS: usize = 100_000;
pub const MAX_RECORDING_SECONDS: u32 = 7_200;
pub const MAX_DICTATION_SECONDS: u32 = 120;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Transcript")]
pub struct TranscriptValue {
    pub text: String,
    pub duration_seconds: f64,
    pub segments: Vec<Segment>,
}

/// Immutable transcript admitted on construction and decoding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "TranscriptValue", into = "TranscriptValue")]
pub struct Transcript(veoveo_types::Checked<TranscriptValue>);

impl std::ops::Deref for Transcript {
    type Target = TranscriptValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for Transcript {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Transcript".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TranscriptValue::json_schema(generator)
    }
}
impl veoveo_types::Check for TranscriptValue {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        self.validate(MAX_RECORDING_SECONDS)
    }
}
impl TryFrom<TranscriptValue> for Transcript {
    type Error = anyhow::Error;
    fn try_from(value: TranscriptValue) -> Result<Self> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<Transcript> for TranscriptValue {
    fn from(value: Transcript) -> Self {
        value.0.into_inner()
    }
}
impl TranscriptValue {
    pub fn build(self) -> Result<Transcript> {
        self.try_into()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub words: Vec<Word>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

fn interval(start: f64, end: f64, duration: f64) -> bool {
    start.is_finite() && end.is_finite() && start >= 0.0 && end >= start && end <= duration + 0.1
}

impl TranscriptValue {
    pub fn validate(&self, max_seconds: u32) -> Result<()> {
        ensure!(
            self.duration_seconds.is_finite()
                && self.duration_seconds >= 0.0
                && self.duration_seconds <= f64::from(max_seconds) + 0.1,
            "invalid transcript duration"
        );
        ensure!(
            self.text.len() <= MAX_TRANSCRIPT_BYTES,
            "transcript text exceeds limit"
        );
        ensure!(
            self.segments.len() <= MAX_SEGMENTS,
            "transcript segment count exceeds limit"
        );
        let mut words = 0usize;
        let mut bytes = self.text.len();
        let mut previous = 0.0;
        for segment in &self.segments {
            ensure!(
                interval(segment.start, segment.end, self.duration_seconds)
                    && segment.start >= previous,
                "invalid segment interval"
            );
            previous = segment.start;
            bytes = bytes.saturating_add(segment.text.len());
            words = words.saturating_add(segment.words.len());
            let mut previous_word = segment.start;
            for word in &segment.words {
                ensure!(
                    interval(word.start, word.end, segment.end) && word.start >= previous_word,
                    "invalid word interval"
                );
                previous_word = word.start;
                bytes = bytes.saturating_add(word.word.len());
            }
        }
        ensure!(
            words <= MAX_WORDS && bytes <= MAX_TRANSCRIPT_BYTES * 3,
            "transcript exceeds limit"
        );
        Ok(())
    }
}

impl Transcript {
    pub fn validate(&self, max_seconds: u32) -> Result<()> {
        self.0.validate(max_seconds)
    }
    pub fn webvtt(&self) -> Result<String> {
        self.validate(MAX_RECORDING_SECONDS)?;
        let mut output = String::from("WEBVTT\n\n");
        for segment in &self.segments {
            output.push_str(&format!(
                "{} --> {}\n{}\n\n",
                timestamp(segment.start),
                timestamp(segment.end),
                segment
                    .text
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        Ok(output)
    }
}

fn timestamp(seconds: f64) -> String {
    let millis = (seconds * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        millis / 3_600_000,
        (millis / 60_000) % 60,
        (millis / 1000) % 60,
        millis % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript() -> Transcript {
        TranscriptValue {
            text: "A <voice> & text".into(),
            duration_seconds: 1.25,
            segments: vec![Segment {
                text: "A <voice> & text".into(),
                start: 0.0,
                end: 1.25,
                words: vec![Word {
                    word: "A".into(),
                    start: 0.0,
                    end: 0.2,
                }],
            }],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn caption_export_escapes_markup_and_preserves_time() {
        assert_eq!(
            transcript().webvtt().unwrap(),
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.250\nA &lt;voice&gt; &amp; text\n\n"
        );
    }

    #[test]
    fn rejects_nonfinite_and_out_of_source_timestamps() {
        let mut value = TranscriptValue::from(transcript());
        value.duration_seconds = f64::NAN;
        assert!(value.validate(120).is_err());
        value.duration_seconds = 1.25;
        value.segments[0].words[0].end = 5.0;
        assert!(value.validate(120).is_err());
    }

    #[test]
    fn transcript_constructor_and_decoder_share_interval_and_size_admission() {
        let valid = serde_json::to_value(transcript()).unwrap();
        for (pointer, bad) in [
            ("/duration_seconds", serde_json::json!(-1)),
            ("/segments/0/start", serde_json::json!(-0.1)),
            ("/segments/0/words/0/end", serde_json::json!(2)),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(pointer).unwrap() = bad;
            assert!(serde_json::from_value::<Transcript>(invalid.clone()).is_err());
            let draft: TranscriptValue = serde_json::from_value(invalid).unwrap();
            assert!(draft.build().is_err());
        }
        let mut draft = TranscriptValue::from(transcript());
        draft.text = "a".repeat(MAX_TRANSCRIPT_BYTES + 1);
        assert!(draft.build().is_err());
        assert!(serde_json::from_value::<Transcript>(valid).is_ok());
    }

    #[test]
    fn rejects_invented_output_fields() {
        let mut value = serde_json::to_value(transcript()).unwrap();
        value["speaker"] = "Alice".into();
        assert!(serde_json::from_value::<Transcript>(value).is_err());
    }
}
