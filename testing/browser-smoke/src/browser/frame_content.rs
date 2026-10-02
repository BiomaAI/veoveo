//! Image-content checks for the displayed App canvas, independent of scene exposure.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

const MINIMUM_STANDARD_DEVIATION: f64 = 5.0;
const MINIMUM_CELL_RANGE: u8 = 16;
const MINIMUM_TEXTURED_CELLS: usize = 4;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LumaCell {
    minimum: u8,
    maximum: u8,
    standard_deviation: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameContent {
    mean_luma: f64,
    luma_standard_deviation: f64,
    minimum_luma: u8,
    maximum_luma: u8,
    // The browser divides its existing 64x36 sample into a fixed 4x4 grid.
    // A fixed-size array rejects incomplete samples at deserialization.
    luma_cells: [LumaCell; 16],
    pixel_sample_error: String,
}

impl FrameContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.pixel_sample_error.is_empty(),
            "canvas pixel sampling failed"
        );
        ensure!(
            self.mean_luma.is_finite()
                && (0.0..=255.0).contains(&self.mean_luma)
                && self.luma_standard_deviation.is_finite()
                && (MINIMUM_STANDARD_DEVIATION..=127.5).contains(&self.luma_standard_deviation)
                && self.minimum_luma < self.maximum_luma,
            "frame is uniform or its luminance statistics are invalid"
        );
        ensure!(
            self.luma_cells.iter().all(|cell| {
                cell.minimum <= cell.maximum
                    && cell.standard_deviation.is_finite()
                    && (0.0..=127.5).contains(&cell.standard_deviation)
            }),
            "frame contains invalid region statistics"
        );
        let textured = self
            .luma_cells
            .iter()
            .filter(|cell| {
                cell.maximum.saturating_sub(cell.minimum) >= MINIMUM_CELL_RANGE
                    && cell.standard_deviation >= MINIMUM_STANDARD_DEVIATION
            })
            .count();
        ensure!(
            textured >= MINIMUM_TEXTURED_CELLS,
            "only {textured}/16 frame regions contain contrast; at least {MINIMUM_TEXTURED_CELLS} are required"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured(mean_luma: f64) -> FrameContent {
        FrameContent {
            mean_luma,
            luma_standard_deviation: 25.32,
            minimum_luma: 0,
            maximum_luma: 196,
            luma_cells: [LumaCell {
                minimum: 0,
                maximum: 196,
                standard_deviation: 25.32,
            }; 16],
            pixel_sample_error: String::new(),
        }
    }

    #[test]
    fn detailed_dark_and_bright_scenes_do_not_require_midrange_exposure() {
        // The installed landing sample failed the former mean >= 25 gate.
        assert!(textured(20.50).validate().is_ok());
        let mut bright = textured(235.0);
        bright.minimum_luma = 200;
        bright.maximum_luma = 255;
        bright.luma_standard_deviation = 15.0;
        bright.luma_cells.fill(LumaCell {
            minimum: 200,
            maximum: 255,
            standard_deviation: 12.0,
        });
        assert!(bright.validate().is_ok());
    }

    #[test]
    fn uniform_black_dark_gray_and_white_frames_are_rejected() {
        for luma in [0, 20, 128, 255] {
            let mut frame = textured(f64::from(luma));
            frame.minimum_luma = luma;
            frame.maximum_luma = luma;
            frame.luma_standard_deviation = 0.0;
            frame.luma_cells.fill(LumaCell {
                minimum: luma,
                maximum: luma,
                standard_deviation: 0.0,
            });
            assert!(frame.validate().is_err());
        }
    }

    #[test]
    fn a_small_bright_patch_cannot_qualify_an_otherwise_blank_frame() {
        let mut frame = textured(20.50);
        let contrast = frame.luma_cells[0];
        frame.luma_cells.fill(LumaCell {
            minimum: 0,
            maximum: 0,
            standard_deviation: 0.0,
        });
        frame.luma_cells[..3].fill(contrast);
        assert!(frame.validate().is_err());
        frame.luma_cells[3] = contrast;
        assert!(frame.validate().is_ok());
    }

    #[test]
    fn large_uniform_regions_do_not_pass_on_global_contrast_alone() {
        let mut frame = textured(127.5);
        frame.minimum_luma = 0;
        frame.maximum_luma = 255;
        frame.luma_standard_deviation = 127.5;
        frame.luma_cells[..8].fill(LumaCell {
            minimum: 0,
            maximum: 0,
            standard_deviation: 0.0,
        });
        frame.luma_cells[8..].fill(LumaCell {
            minimum: 255,
            maximum: 255,
            standard_deviation: 0.0,
        });
        assert!(frame.validate().is_err());
    }

    #[test]
    fn sampler_failures_and_nonfinite_statistics_fail_closed() {
        let mut frame = textured(20.50);
        frame.pixel_sample_error = "canvas unavailable".into();
        assert!(frame.validate().is_err());
        frame.pixel_sample_error.clear();
        frame.mean_luma = f64::NAN;
        assert!(frame.validate().is_err());
        frame.mean_luma = 20.50;
        frame.luma_cells[0].standard_deviation = f64::INFINITY;
        assert!(frame.validate().is_err());
    }

    #[test]
    fn incomplete_region_observations_are_rejected_on_entry() {
        let mut wire = serde_json::to_value(textured(20.50)).unwrap();
        wire["lumaCells"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<FrameContent>(wire).is_err());
    }
}
