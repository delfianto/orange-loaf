use serde::{Deserialize, Serialize};

/// A crop rectangle expressed as pixels trimmed from each edge (matching how
/// the eventual visual crop tool — M6 — will report a selection), not as an
/// absolute width/height/offset rectangle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crop {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl Crop {
    fn is_noop(&self) -> bool {
        self.top == 0 && self.bottom == 0 && self.left == 0 && self.right == 0
    }
}

/// The filter-graph inputs `OrangeLoaf` supports today. Denoise, eq, deblock, and
/// HDR tonemap join this (in the same fixed position in `render`'s stage
/// order, per notes/plan.md Appendix A) as their own UI lands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterSpec {
    pub deinterlace: bool,
    pub crop: Option<Crop>,
    pub scale_width: Option<u32>,
    pub scale_height: Option<u32>,
    /// Clockwise rotation in degrees; only 0/90/180/270 are meaningful.
    pub rotate_degrees: u16,
    pub horizontal_flip: bool,
    pub vertical_flip: bool,
}

impl FilterSpec {
    /// Renders the enabled stages into a single ffmpeg `-vf` filter-graph
    /// string, in a fixed order (deinterlace, crop, scale, rotate, vflip,
    /// hflip), or `None` if nothing is enabled — callers should omit `-vf`
    /// entirely in that case rather than passing an empty filter graph.
    #[must_use]
    pub fn render(&self) -> Option<String> {
        let mut stages: Vec<String> = Vec::new();

        if self.deinterlace {
            stages.push("yadif".to_owned());
        }

        if let Some(crop) = &self.crop {
            if !crop.is_noop() {
                stages.push(format!(
                    "crop=iw-{}-{}:ih-{}-{}:{}:{}",
                    crop.left, crop.right, crop.top, crop.bottom, crop.left, crop.top
                ));
            }
        }

        if self.scale_width.is_some() || self.scale_height.is_some() {
            let width = self
                .scale_width
                .map_or_else(|| "-2".to_owned(), |w| w.to_string());
            let height = self
                .scale_height
                .map_or_else(|| "-2".to_owned(), |h| h.to_string());
            stages.push(format!("scale={width}:{height}"));
        }

        match self.rotate_degrees % 360 {
            90 => stages.push("transpose=1".to_owned()),
            180 => stages.extend(["transpose=2".to_owned(), "transpose=2".to_owned()]),
            270 => stages.push("transpose=2".to_owned()),
            _ => {}
        }

        if self.vertical_flip {
            stages.push("vflip".to_owned());
        }
        if self.horizontal_flip {
            stages.push("hflip".to_owned());
        }

        if stages.is_empty() {
            None
        } else {
            Some(stages.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Crop, FilterSpec};

    #[test]
    fn renders_none_when_nothing_is_enabled() {
        assert_eq!(FilterSpec::default().render(), None);
    }

    #[test]
    fn a_zeroed_crop_is_a_noop() {
        let spec = FilterSpec {
            crop: Some(Crop::default()),
            ..FilterSpec::default()
        };
        assert_eq!(spec.render(), None);
    }

    #[test]
    fn renders_crop() {
        let spec = FilterSpec {
            crop: Some(Crop {
                top: 10,
                bottom: 20,
                left: 5,
                right: 15,
            }),
            ..FilterSpec::default()
        };
        assert_eq!(spec.render(), Some("crop=iw-5-15:ih-10-20:5:10".to_owned()));
    }

    #[test]
    fn renders_scale_with_aspect_preserving_dash_two_for_the_unset_dimension() {
        let spec = FilterSpec {
            scale_width: Some(1280),
            ..FilterSpec::default()
        };
        assert_eq!(spec.render(), Some("scale=1280:-2".to_owned()));
    }

    #[test]
    fn renders_rotation_variants() {
        assert_eq!(
            FilterSpec {
                rotate_degrees: 90,
                ..FilterSpec::default()
            }
            .render(),
            Some("transpose=1".to_owned())
        );
        assert_eq!(
            FilterSpec {
                rotate_degrees: 180,
                ..FilterSpec::default()
            }
            .render(),
            Some("transpose=2,transpose=2".to_owned())
        );
        assert_eq!(
            FilterSpec {
                rotate_degrees: 270,
                ..FilterSpec::default()
            }
            .render(),
            Some("transpose=2".to_owned())
        );
        assert_eq!(
            FilterSpec {
                rotate_degrees: 0,
                ..FilterSpec::default()
            }
            .render(),
            None
        );
    }

    #[test]
    fn renders_deinterlace_as_yadif() {
        let spec = FilterSpec {
            deinterlace: true,
            ..FilterSpec::default()
        };
        assert_eq!(spec.render(), Some("yadif".to_owned()));
    }

    #[test]
    fn stage_order_is_deinterlace_then_crop_then_scale_then_rotate_then_vflip_then_hflip() {
        let spec = FilterSpec {
            deinterlace: true,
            crop: Some(Crop {
                top: 1,
                bottom: 1,
                left: 1,
                right: 1,
            }),
            scale_width: Some(640),
            rotate_degrees: 90,
            vertical_flip: true,
            horizontal_flip: true,
            ..FilterSpec::default()
        };
        assert_eq!(
            spec.render(),
            Some("yadif,crop=iw-1-1:ih-1-1:1:1,scale=640:-2,transpose=1,vflip,hflip".to_owned())
        );
    }
}
