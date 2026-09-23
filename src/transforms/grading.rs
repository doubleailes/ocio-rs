//! Grading value types used by the grading transforms (`GradingPrimary`,
//! `GradingTone`, `GradingRgbCurve`, `GradingHueCurve`, B-spline curves).

use crate::types::{BSplineType, GradingStyle, HueCurveType, RgbCurveType};

/// Red / green / blue / master values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingRgbm {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub master: f64,
}

impl GradingRgbm {
    pub const fn new(red: f64, green: f64, blue: f64, master: f64) -> Self {
        Self { red, green, blue, master }
    }
}

/// Primary grading values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingPrimary {
    pub brightness: GradingRgbm,
    pub contrast: GradingRgbm,
    pub gamma: GradingRgbm,
    pub offset: GradingRgbm,
    pub exposure: GradingRgbm,
    pub lift: GradingRgbm,
    pub gain: GradingRgbm,
    pub saturation: f64,
    /// LOG default is -0.2, LIN default is 0.18.
    pub pivot: f64,
    pub pivot_black: f64,
    pub pivot_white: f64,
    pub clamp_black: f64,
    pub clamp_white: f64,
}

impl GradingPrimary {
    /// Value meaning "no black clamp".
    pub const NO_CLAMP_BLACK: f64 = -f64::MAX;
    /// Value meaning "no white clamp".
    pub const NO_CLAMP_WHITE: f64 = f64::MAX;

    /// Default (identity) values for a style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            brightness: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            contrast: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            gamma: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            offset: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            exposure: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            lift: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            gain: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            saturation: 1.0,
            pivot: if style == GradingStyle::Log { -0.2 } else { 0.18 },
            pivot_black: 0.0,
            pivot_white: 1.0,
            clamp_black: Self::NO_CLAMP_BLACK,
            clamp_white: Self::NO_CLAMP_WHITE,
        }
    }
}

/// A control point of a B-spline curve.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingControlPoint {
    pub x: f32,
    pub y: f32,
}

impl GradingControlPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A B-spline curve defined by control points and optional slopes.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingBSplineCurve {
    pub control_points: Vec<GradingControlPoint>,
    /// Slopes at each control point; empty or all zero means "default slopes".
    pub slopes: Vec<f32>,
    pub spline_type: BSplineType,
}

impl GradingBSplineCurve {
    /// `size` points at the origin.
    pub fn with_size(size: usize, spline_type: BSplineType) -> Self {
        Self {
            control_points: vec![GradingControlPoint::default(); size],
            slopes: vec![0.0; size],
            spline_type,
        }
    }

    /// Curve from points.
    pub fn new(points: &[(f32, f32)], spline_type: BSplineType) -> Self {
        Self {
            control_points: points.iter().map(|&(x, y)| GradingControlPoint::new(x, y)).collect(),
            slopes: vec![0.0; points.len()],
            spline_type,
        }
    }

    pub fn num_control_points(&self) -> usize {
        self.control_points.len()
    }

    /// Resize the curve (slopes are kept in sync).
    pub fn set_num_control_points(&mut self, size: usize) {
        self.control_points.resize(size, GradingControlPoint::default());
        self.slopes.resize(size, 0.0);
    }

    pub fn slope(&self, index: usize) -> f32 {
        self.slopes.get(index).copied().unwrap_or(0.0)
    }

    pub fn set_slope(&mut self, index: usize, slope: f32) {
        if self.slopes.len() < self.control_points.len() {
            self.slopes.resize(self.control_points.len(), 0.0);
        }
        if index < self.slopes.len() {
            self.slopes[index] = slope;
        }
    }

    /// True if no custom slopes are set.
    pub fn slopes_are_default(&self) -> bool {
        self.slopes.iter().all(|&s| s == 0.0)
    }
}

/// Default identity curve for RGB curves (non-linear styles).
pub fn default_rgb_curve(style: GradingStyle) -> GradingBSplineCurve {
    if style == GradingStyle::Lin {
        GradingBSplineCurve::new(&[(-7.0, -7.0), (0.0, 0.0), (7.0, 7.0)], BSplineType::BSpline)
    } else {
        GradingBSplineCurve::new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], BSplineType::BSpline)
    }
}

/// Red, green, blue and master curves.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingRgbCurve {
    pub curves: [GradingBSplineCurve; 4],
}

impl GradingRgbCurve {
    /// Identity curves for the style.
    pub fn new(style: GradingStyle) -> Self {
        let c = default_rgb_curve(style);
        Self { curves: [c.clone(), c.clone(), c.clone(), c] }
    }
    pub fn curve(&self, c: RgbCurveType) -> &GradingBSplineCurve {
        &self.curves[c as usize]
    }
    pub fn curve_mut(&mut self, c: RgbCurveType) -> &mut GradingBSplineCurve {
        &mut self.curves[c as usize]
    }
}

/// B-spline type used by a hue curve type.
pub fn bspline_type_for_hue_curve_type(c: HueCurveType) -> BSplineType {
    match c {
        HueCurveType::HueHue => BSplineType::HueHueBSpline,
        HueCurveType::HueSat | HueCurveType::HueLum => BSplineType::Periodic1BSpline,
        HueCurveType::HueFx => BSplineType::Periodic0BSpline,
        HueCurveType::LumSat | HueCurveType::SatLum => BSplineType::Horizontal1BSpline,
        HueCurveType::SatSat | HueCurveType::LumLum => BSplineType::DiagonalBSpline,
    }
}

/// Default identity curve for a hue curve type.
pub fn default_hue_curve(c: HueCurveType, style: GradingStyle) -> GradingBSplineCurve {
    const S: f32 = 1.0 / 6.0;
    let t = bspline_type_for_hue_curve_type(c);
    let lin = style == GradingStyle::Lin;
    match c {
        HueCurveType::HueHue => GradingBSplineCurve::new(
            &[(0.0, 0.0), (S, S), (2.0 * S, 2.0 * S), (0.5, 0.5), (4.0 * S, 4.0 * S), (5.0 * S, 5.0 * S)],
            t,
        ),
        HueCurveType::HueSat | HueCurveType::HueLum => GradingBSplineCurve::new(
            &[(0.0, 1.0), (S, 1.0), (2.0 * S, 1.0), (0.5, 1.0), (4.0 * S, 1.0), (5.0 * S, 1.0)],
            t,
        ),
        HueCurveType::HueFx => GradingBSplineCurve::new(
            &[(0.0, 0.0), (S, 0.0), (2.0 * S, 0.0), (0.5, 0.0), (4.0 * S, 0.0), (5.0 * S, 0.0)],
            t,
        ),
        HueCurveType::LumSat => {
            if lin {
                GradingBSplineCurve::new(&[(-7.0, 1.0), (0.0, 1.0), (7.0, 1.0)], t)
            } else {
                GradingBSplineCurve::new(&[(0.0, 1.0), (0.5, 1.0), (1.0, 1.0)], t)
            }
        }
        HueCurveType::SatSat => GradingBSplineCurve::new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], t),
        HueCurveType::SatLum => GradingBSplineCurve::new(&[(0.0, 1.0), (0.5, 1.0), (1.0, 1.0)], t),
        HueCurveType::LumLum => {
            if lin {
                GradingBSplineCurve::new(&[(-7.0, -7.0), (0.0, 0.0), (7.0, 7.0)], t)
            } else {
                GradingBSplineCurve::new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], t)
            }
        }
    }
}

/// The eight hue curves.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingHueCurve {
    pub curves: [GradingBSplineCurve; 8],
    pub draw_curve_only: bool,
}

impl GradingHueCurve {
    /// Identity curves for the style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            curves: HueCurveType::ALL.map(|c| default_hue_curve(c, style)),
            draw_curve_only: false,
        }
    }
    pub fn curve(&self, c: HueCurveType) -> &GradingBSplineCurve {
        &self.curves[c as usize]
    }
    pub fn curve_mut(&mut self, c: HueCurveType) -> &mut GradingBSplineCurve {
        &mut self.curves[c as usize]
    }
}

/// RGBM + start + width values of a tonal zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingRgbmsw {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub master: f64,
    /// Or center for midtones.
    pub start: f64,
    /// Or pivot for shadows and highlights.
    pub width: f64,
}

impl Default for GradingRgbmsw {
    fn default() -> Self {
        Self { red: 1.0, green: 1.0, blue: 1.0, master: 1.0, start: 0.0, width: 1.0 }
    }
}

impl GradingRgbmsw {
    pub const fn new(red: f64, green: f64, blue: f64, master: f64, start: f64, width: f64) -> Self {
        Self { red, green, blue, master, start, width }
    }
    /// Identity RGBM with the given start / width.
    pub const fn with_start_width(start: f64, width: f64) -> Self {
        Self { red: 1.0, green: 1.0, blue: 1.0, master: 1.0, start, width }
    }
}

/// Tonal grading values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingTone {
    pub blacks: GradingRgbmsw,
    pub shadows: GradingRgbmsw,
    pub midtones: GradingRgbmsw,
    pub highlights: GradingRgbmsw,
    pub whites: GradingRgbmsw,
    pub s_contrast: f64,
}

impl GradingTone {
    /// Default (identity) values for a style.
    pub fn new(style: GradingStyle) -> Self {
        use GradingRgbmsw as W;
        match style {
            GradingStyle::Lin => Self {
                blacks: W::with_start_width(0.0, 4.0),
                shadows: W::with_start_width(2.0, -7.0),
                midtones: W::with_start_width(0.0, 8.0),
                highlights: W::with_start_width(-2.0, 9.0),
                whites: W::with_start_width(0.0, 8.0),
                s_contrast: 1.0,
            },
            GradingStyle::Log => Self {
                blacks: W::with_start_width(0.4, 0.4),
                shadows: W::with_start_width(0.5, 0.0),
                midtones: W::with_start_width(0.4, 0.6),
                highlights: W::with_start_width(0.3, 1.0),
                whites: W::with_start_width(0.4, 0.5),
                s_contrast: 1.0,
            },
            GradingStyle::Video => Self {
                blacks: W::with_start_width(0.4, 0.4),
                shadows: W::with_start_width(0.6, 0.0),
                midtones: W::with_start_width(0.4, 0.7),
                highlights: W::with_start_width(0.2, 1.0),
                whites: W::with_start_width(0.5, 0.5),
                s_contrast: 1.0,
            },
        }
    }
}
