//! RRD video-boundary inspection shared by recording producers and storage.

use std::io::{BufReader, Cursor};

use anyhow::{Context, Result, ensure};
use re_chunk::Chunk;
use re_log_encoding::rrd::Decoder;
use re_log_types::LogMsg;
use re_sdk_types::archetypes::VideoStream;
use re_sdk_types::components::VideoSample;
use re_sdk_types::external::arrow::array::{Array as _, ListArray};
use re_sdk_types::external::re_types_core::Loggable;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RrdVideoBoundary {
    pub contains_video: bool,
    pub begins_with_keyframe: bool,
}

impl RrdVideoBoundary {
    fn observe(&mut self, sample: &VideoSample) -> Result<()> {
        let keyframe = h264_access_unit_is_idr(sample.0.0.as_ref())?;
        if !self.contains_video {
            self.begins_with_keyframe = keyframe;
        }
        self.contains_video = true;
        Ok(())
    }
}

/// Return whether an Annex B access unit contains an H.264 IDR picture.
pub fn h264_access_unit_is_idr(bytes: &[u8]) -> Result<bool> {
    Ok(annex_b_nals(bytes)?.iter().any(|nal| nal[0] & 0x1f == 5))
}

pub fn inspect_rrd_video_boundary(encoded_rrd: &[u8]) -> Result<RrdVideoBoundary> {
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(encoded_rrd)))
        .context("decoding RRD video boundary")?;
    let mut boundary = RrdVideoBoundary::default();
    for message in decoder {
        inspect_log_message_video_boundary(&message?, &mut boundary)?;
    }
    Ok(boundary)
}

pub fn inspect_log_message_video_boundary(
    message: &LogMsg,
    boundary: &mut RrdVideoBoundary,
) -> Result<()> {
    let LogMsg::ArrowMsg(_, arrow) = message else {
        return Ok(());
    };
    let chunk = Chunk::from_arrow_msg(arrow).context("decoding Rerun video chunk")?;
    let sample_component = VideoStream::descriptor_sample().component;
    let Some(samples) = chunk.components().get_array(sample_component) else {
        return Ok(());
    };
    for row in 0..chunk.num_rows() {
        if let Some(sample) = component_at::<VideoSample>(samples, row)? {
            boundary.observe(&sample)?;
        }
    }
    Ok(())
}

pub fn annex_b_nals(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    let mut starts = Vec::new();
    let mut cursor = 0;
    while cursor + 3 <= bytes.len() {
        let start_code = if bytes[cursor..].starts_with(&[0, 0, 0, 1]) {
            Some(4)
        } else if bytes[cursor..].starts_with(&[0, 0, 1]) {
            Some(3)
        } else {
            None
        };
        if let Some(length) = start_code {
            starts.push((cursor, cursor + length));
            cursor += length;
        } else {
            cursor += 1;
        }
    }
    ensure!(!starts.is_empty(), "H.264 sample is not Annex B");
    let mut nals = Vec::with_capacity(starts.len());
    for (index, (_, payload_start)) in starts.iter().copied().enumerate() {
        let payload_end = starts
            .get(index + 1)
            .map_or(bytes.len(), |(start, _)| *start);
        let mut payload_end = payload_end;
        while payload_end > payload_start && bytes[payload_end - 1] == 0 {
            payload_end -= 1;
        }
        ensure!(payload_start < payload_end, "H.264 contains an empty NAL");
        nals.push(&bytes[payload_start..payload_end]);
    }
    Ok(nals)
}

fn component_at<T: Loggable>(
    array: &dyn re_sdk_types::external::arrow::array::Array,
    row: usize,
) -> Result<Option<T>> {
    let rows = array
        .as_any()
        .downcast_ref::<ListArray>()
        .context("Rerun component column is not a list array")?;
    if rows.is_null(row) || rows.value_length(row) == 0 {
        return Ok(None);
    }
    let values =
        T::from_arrow(rows.value(row).as_ref()).context("deserializing Rerun video component")?;
    ensure!(
        values.len() == 1,
        "video sample row must contain exactly one component instance"
    );
    Ok(values.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_h264_idr_access_units() {
        assert!(h264_access_unit_is_idr(&[0, 0, 0, 1, 0x65, 1]).unwrap());
        assert!(!h264_access_unit_is_idr(&[0, 0, 1, 0x41, 1]).unwrap());
    }
}
