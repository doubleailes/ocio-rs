//! Grading value types used by the grading transforms (`GradingPrimary`,
//! `GradingTone`, `GradingRgbCurve`, `GradingHueCurve`, B-spline curves).
//!
//! Port of the value parts of `GradingPrimary.cpp`, `GradingTone.cpp`,
//! `GradingBSplineCurve.cpp`, `GradingRGBCurve.cpp`, `GradingHueCurve.cpp`
//! and of the stream operators of the `Grading*Transform.cpp` files.

use crate::error::{Error, Result};
use crate::types::{BSplineType, GradingStyle, HueCurveType, RgbCurveType};
use std::fmt;

// ---------------------------------------------------------------------------
// Number formatting

/// Format a number like a C++ `std::ostream` with the given precision and the
/// default float field (i.e. like `printf("%.*g")`): `precision` significant
/// digits, trailing zeros removed, scientific notation for very small or
/// very large magnitudes.
///
/// This is used to reproduce OCIO's messages and cache identifiers.
pub fn fmt_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-nan".to_string()
        } else {
            "nan".to_string()
        };
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let p = precision.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    // Round to `p` significant digits using the scientific formatter.
    let sci = format!("{:.*e}", p - 1, v);
    let (mantissa, exp) = match sci.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (sci.clone(), 0),
    };
    if exp < -4 || exp >= p as i32 {
        let m = strip_trailing_zeros(&mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{m}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip_trailing_zeros(&format!("{:.*}", decimals, v))
    }
}

fn strip_trailing_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Default C++ stream precision.
const DEFAULT_PRECISION: usize = 6;

fn prec(f: &fmt::Formatter<'_>) -> usize {
    f.precision().unwrap_or(DEFAULT_PRECISION)
}

// ---------------------------------------------------------------------------
// GradingRgbm / GradingPrimary

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
        Self {
            red,
            green,
            blue,
            master,
        }
    }
}

impl fmt::Display for GradingRgbm {
    /// `<r=.., g=.., b=.., m=..>` (the formatter precision, default 6, is used
    /// for the numbers).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(
            f,
            "<r={}, g={}, b={}, m={}>",
            fmt_g(self.red, p),
            fmt_g(self.green, p),
            fmt_g(self.blue, p),
            fmt_g(self.master, p)
        )
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
            pivot: if style == GradingStyle::Log {
                -0.2
            } else {
                0.18
            },
            pivot_black: 0.0,
            pivot_white: 1.0,
            clamp_black: Self::NO_CLAMP_BLACK,
            clamp_white: Self::NO_CLAMP_WHITE,
        }
    }

    /// Validate the values for the given style (port of
    /// `GradingPrimary::validate`).
    pub fn validate(&self, style: GradingStyle) -> Result<()> {
        const LOWER_BOUND: f64 = 0.01;
        const BOUND_ERROR: f64 = 0.000001;
        const MIN: f64 = LOWER_BOUND - BOUND_ERROR;

        let below =
            |v: &GradingRgbm| v.red < MIN || v.green < MIN || v.blue < MIN || v.master < MIN;

        if style != GradingStyle::Lin && below(&self.gamma) {
            crate::bail!(
                "GradingPrimary gamma '{}' are below lower bound ({}).",
                self.gamma,
                fmt_g(LOWER_BOUND, DEFAULT_PRECISION)
            );
        }
        if style == GradingStyle::Lin && below(&self.contrast) {
            crate::bail!(
                "GradingPrimary contrast '{}' are below lower bound ({}).",
                self.contrast,
                fmt_g(LOWER_BOUND, DEFAULT_PRECISION)
            );
        }
        if (self.pivot_white - self.pivot_black) < MIN {
            crate::bail!("GradingPrimary black pivot should be smaller than white pivot.");
        }
        if self.clamp_black > self.clamp_white {
            crate::bail!("GradingPrimary black clamp should be smaller than white clamp.");
        }
        Ok(())
    }
}

impl fmt::Display for GradingPrimary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(f, "<brightness={:.p$}", self.brightness)?;
        write!(f, ", contrast={:.p$}", self.contrast)?;
        write!(f, ", gamma={:.p$}", self.gamma)?;
        write!(f, ", offset={:.p$}", self.offset)?;
        write!(f, ", exposure={:.p$}", self.exposure)?;
        write!(f, ", lift={:.p$}", self.lift)?;
        write!(f, ", gain={:.p$}", self.gain)?;
        write!(f, ", saturation={}", fmt_g(self.saturation, p))?;
        write!(f, ", pivot=<contrast={}", fmt_g(self.pivot, p))?;
        write!(f, ", black={}", fmt_g(self.pivot_black, p))?;
        write!(f, ", white={}", fmt_g(self.pivot_white, p))?;
        f.write_str(">")?;
        if self.clamp_black != Self::NO_CLAMP_BLACK {
            write!(f, ", clampBlack={}", fmt_g(self.clamp_black, p))?;
        }
        if self.clamp_white != Self::NO_CLAMP_WHITE {
            write!(f, ", clampWhite={}", fmt_g(self.clamp_white, p))?;
        }
        f.write_str(">")
    }
}

// ---------------------------------------------------------------------------
// B-spline curves

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

impl fmt::Display for GradingControlPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(
            f,
            "<x={}, y={}>",
            fmt_g(self.x as f64, p),
            fmt_g(self.y as f64, p)
        )
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

    /// `size` points at the origin, using the spline type of a hue curve type.
    pub fn with_size_for_hue_curve(size: usize, curve: HueCurveType) -> Self {
        Self::with_size(size, bspline_type_for_hue_curve_type(curve))
    }

    /// Curve from points.
    pub fn new(points: &[(f32, f32)], spline_type: BSplineType) -> Self {
        Self {
            control_points: points
                .iter()
                .map(|&(x, y)| GradingControlPoint::new(x, y))
                .collect(),
            slopes: vec![0.0; points.len()],
            spline_type,
        }
    }

    /// Curve from points, using the spline type of a hue curve type.
    pub fn new_for_hue_curve(points: &[(f32, f32)], curve: HueCurveType) -> Self {
        Self::new(points, bspline_type_for_hue_curve_type(curve))
    }

    /// Curve from control points (slopes are default).
    pub fn from_control_points(points: Vec<GradingControlPoint>, spline_type: BSplineType) -> Self {
        let n = points.len();
        Self {
            control_points: points,
            slopes: vec![0.0; n],
            spline_type,
        }
    }

    pub fn num_control_points(&self) -> usize {
        self.control_points.len()
    }

    /// Resize the curve (slopes are kept in sync).
    pub fn set_num_control_points(&mut self, size: usize) {
        self.control_points
            .resize(size, GradingControlPoint::default());
        self.slopes.resize(size, 0.0);
    }

    /// Check that `index` is a valid control point index.
    pub fn validate_index(&self, index: usize) -> Result<()> {
        let n = self.control_points.len();
        if index >= n {
            crate::bail!("There are '{n}' control points. '{index}' is out of bounds.");
        }
        Ok(())
    }

    /// Control point at `index` (error if out of bounds).
    pub fn control_point(&self, index: usize) -> Result<&GradingControlPoint> {
        self.validate_index(index)?;
        Ok(&self.control_points[index])
    }

    /// Mutable control point at `index` (error if out of bounds).
    pub fn control_point_mut(&mut self, index: usize) -> Result<&mut GradingControlPoint> {
        self.validate_index(index)?;
        Ok(&mut self.control_points[index])
    }

    /// Slope at `index`, 0 if not set.
    pub fn slope(&self, index: usize) -> f32 {
        self.slopes.get(index).copied().unwrap_or(0.0)
    }

    /// Slope at `index` (error if out of bounds).
    pub fn try_slope(&self, index: usize) -> Result<f32> {
        self.validate_index(index)?;
        Ok(self.slope(index))
    }

    /// Set the slope at `index` (ignored if out of bounds, see
    /// [`try_set_slope`](Self::try_set_slope)).
    pub fn set_slope(&mut self, index: usize, slope: f32) {
        let _ = self.try_set_slope(index, slope);
    }

    /// Set the slope at `index` (error if out of bounds).
    pub fn try_set_slope(&mut self, index: usize, slope: f32) -> Result<()> {
        self.validate_index(index)?;
        if self.slopes.len() < self.control_points.len() {
            self.slopes.resize(self.control_points.len(), 0.0);
        }
        self.slopes[index] = slope;
        Ok(())
    }

    /// True if no custom slopes are set.
    pub fn slopes_are_default(&self) -> bool {
        self.slopes.iter().all(|&s| s == 0.0)
    }

    /// Validate the curve (port of `GradingBSplineCurveImpl::validate`).
    pub fn validate(&self) -> Result<()> {
        let num_points = self.control_points.len();
        if num_points < 2 {
            crate::bail!("There must be at least 2 control points.");
        }
        if num_points != self.slopes.len() {
            crate::bail!("The slopes array must be the same length as the control points.");
        }

        // Make sure the x-coordinates are non-decreasing.
        let mut last_x = -f32::MAX;
        for (i, cp) in self.control_points.iter().enumerate() {
            if cp.x < last_x {
                crate::bail!(
                    "Control point at index {i} has a x coordinate '{}' that is less than previous control \
                     point x coordinate '{}'.",
                    fmt_g(cp.x as f64, DEFAULT_PRECISION),
                    fmt_g(last_x as f64, DEFAULT_PRECISION)
                );
            }
            last_x = cp.x;
        }

        // The x-coordinates for a hue-hue spline must be in [0,1].
        if self.spline_type == BSplineType::HueHueBSpline {
            if self.control_points[0].x < 0.0 {
                crate::bail!("The HUE-HUE spline may not have negative x coordinates.");
            } else if self.control_points[num_points - 1].x > 1.0 {
                crate::bail!("The HUE-HUE spline may not have x coordinates greater than one.");
            }
        }

        // Make sure the y-coordinates are non-decreasing, for diagonal spline types.
        if matches!(
            self.spline_type,
            BSplineType::BSpline | BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
        ) {
            let mut last_y = -f32::MAX;
            if self.spline_type == BSplineType::HueHueBSpline {
                // The curve is diagonal but continues in a periodic way, so wrap the last
                // point around and ensure the first point would preserve monotonicity.
                last_y = self.control_points[num_points - 1].y - 1.0;
            }
            for (i, cp) in self.control_points.iter().enumerate() {
                if cp.y < last_y {
                    crate::bail!(
                        "Control point at index {i} has a y coordinate '{}' that is less than previous \
                         control point y coordinate '{}'.",
                        fmt_g(cp.y as f64, DEFAULT_PRECISION),
                        fmt_g(last_y as f64, DEFAULT_PRECISION)
                    );
                }
                last_y = cp.y;
            }
        }

        // Don't allow only x values of 0 and 1 for periodic curves (since they are
        // essentially only one point).
        if num_points == 2
            && matches!(
                self.spline_type,
                BSplineType::Periodic1BSpline
                    | BSplineType::Periodic0BSpline
                    | BSplineType::HueHueBSpline
            )
        {
            let del_x = self.control_points[1].x - self.control_points[0].x;
            if ((1.0 - del_x).abs() as f64) < 1e-3 {
                crate::bail!("The periodic spline x coordinates may not wrap to the same value.");
            }
        }
        Ok(())
    }

    /// True if the curve does not modify its input (port of
    /// `GradingBSplineCurveImpl::isIdentity`).
    pub fn is_identity(&self) -> bool {
        let points_identity = match self.spline_type {
            BSplineType::DiagonalBSpline | BSplineType::BSpline | BSplineType::HueHueBSpline => {
                self.control_points.iter().all(|cp| cp.x == cp.y)
            }
            BSplineType::Periodic0BSpline => self.control_points.iter().all(|cp| cp.y == 0.0),
            BSplineType::Horizontal1BSpline | BSplineType::Periodic1BSpline => {
                self.control_points.iter().all(|cp| cp.y == 1.0)
            }
        };
        points_identity && self.slopes_are_default()
    }
}

impl fmt::Display for GradingBSplineCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        f.write_str("<control_points=[")?;
        let default_slopes = self.slopes_are_default();
        for (i, cp) in self.control_points.iter().enumerate() {
            if default_slopes {
                write!(f, "{cp:.p$}")?;
            } else {
                write!(
                    f,
                    "<x={}, y={}, slp={}>",
                    fmt_g(cp.x as f64, p),
                    fmt_g(cp.y as f64, p),
                    fmt_g(self.slope(i) as f64, p)
                )?;
            }
        }
        f.write_str("]>")
    }
}

// ---------------------------------------------------------------------------
// GradingRgbCurve

/// Default identity curve for RGB curves.
pub fn default_rgb_curve(style: GradingStyle) -> GradingBSplineCurve {
    if style == GradingStyle::Lin {
        GradingBSplineCurve::new(
            &[(-7.0, -7.0), (0.0, 0.0), (7.0, 7.0)],
            BSplineType::BSpline,
        )
    } else {
        GradingBSplineCurve::new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], BSplineType::BSpline)
    }
}

/// Name of an RGB curve (as used in error messages).
pub fn rgb_curve_type_name(c: RgbCurveType) -> &'static str {
    match c {
        RgbCurveType::Red => "red",
        RgbCurveType::Green => "green",
        RgbCurveType::Blue => "blue",
        RgbCurveType::Master => "master",
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
        Self {
            curves: [c.clone(), c.clone(), c.clone(), c],
        }
    }

    /// Curves from the four red, green, blue and master splines.
    pub fn from_curves(
        red: GradingBSplineCurve,
        green: GradingBSplineCurve,
        blue: GradingBSplineCurve,
        master: GradingBSplineCurve,
    ) -> Self {
        Self {
            curves: [red, green, blue, master],
        }
    }

    pub fn curve(&self, c: RgbCurveType) -> &GradingBSplineCurve {
        &self.curves[c as usize]
    }
    pub fn curve_mut(&mut self, c: RgbCurveType) -> &mut GradingBSplineCurve {
        &mut self.curves[c as usize]
    }

    /// Validate the curves (port of `GradingRGBCurveImpl::validate`).
    pub fn validate(&self) -> Result<()> {
        for c in RgbCurveType::ALL {
            let curve = self.curve(c);
            curve.validate().map_err(|e| {
                Error::msg(format!(
                    "GradingRGBCurve validation failed for '{}' curve with: {}",
                    rgb_curve_type_name(c),
                    e.message()
                ))
            })?;
            if curve.spline_type != BSplineType::BSpline {
                crate::bail!(
                    "GradingRGBCurve validation failed: '{}' curve is of the wrong BSplineType.",
                    rgb_curve_type_name(c)
                );
            }
        }
        Ok(())
    }

    /// True if all curves are identities.
    pub fn is_identity(&self) -> bool {
        self.curves.iter().all(|c| c.is_identity())
    }
}

impl fmt::Display for GradingRgbCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(f, "<red={:.p$}", self.curve(RgbCurveType::Red))?;
        write!(f, ", green={:.p$}", self.curve(RgbCurveType::Green))?;
        write!(f, ", blue={:.p$}", self.curve(RgbCurveType::Blue))?;
        write!(f, ", master={:.p$}", self.curve(RgbCurveType::Master))?;
        f.write_str(">")
    }
}

// ---------------------------------------------------------------------------
// GradingHueCurve

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

/// Name of a hue curve (as used in error messages and serialization).
pub fn hue_curve_type_name(c: HueCurveType) -> &'static str {
    match c {
        HueCurveType::HueHue => "hue_hue",
        HueCurveType::HueSat => "hue_sat",
        HueCurveType::HueLum => "hue_lum",
        HueCurveType::LumSat => "lum_sat",
        HueCurveType::SatSat => "sat_sat",
        HueCurveType::LumLum => "lum_lum",
        HueCurveType::SatLum => "sat_lum",
        HueCurveType::HueFx => "hue_fx",
    }
}

/// Default identity curve for a hue curve type.
pub fn default_hue_curve(c: HueCurveType, style: GradingStyle) -> GradingBSplineCurve {
    const S1: f32 = 1.0 / 6.0;
    const S2: f32 = 2.0 / 6.0;
    const S4: f32 = 4.0 / 6.0;
    const S5: f32 = 5.0 / 6.0;
    let t = bspline_type_for_hue_curve_type(c);
    let lin = style == GradingStyle::Lin;
    match c {
        HueCurveType::HueHue => GradingBSplineCurve::new(
            &[
                (0.0, 0.0),
                (S1, S1),
                (S2, S2),
                (0.5, 0.5),
                (S4, S4),
                (S5, S5),
            ],
            t,
        ),
        HueCurveType::HueSat | HueCurveType::HueLum => GradingBSplineCurve::new(
            &[
                (0.0, 1.0),
                (S1, 1.0),
                (S2, 1.0),
                (0.5, 1.0),
                (S4, 1.0),
                (S5, 1.0),
            ],
            t,
        ),
        HueCurveType::HueFx => GradingBSplineCurve::new(
            &[
                (0.0, 0.0),
                (S1, 0.0),
                (S2, 0.0),
                (0.5, 0.0),
                (S4, 0.0),
                (S5, 0.0),
            ],
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

    /// Hue curves from the eight splines (in `HueCurveType` order). The result
    /// is validated (like OCIO's `GradingHueCurve::Create`).
    #[allow(clippy::too_many_arguments)]
    pub fn from_curves(
        hue_hue: GradingBSplineCurve,
        hue_sat: GradingBSplineCurve,
        hue_lum: GradingBSplineCurve,
        lum_sat: GradingBSplineCurve,
        sat_sat: GradingBSplineCurve,
        lum_lum: GradingBSplineCurve,
        sat_lum: GradingBSplineCurve,
        hue_fx: GradingBSplineCurve,
    ) -> Result<Self> {
        let res = Self {
            curves: [
                hue_hue, hue_sat, hue_lum, lum_sat, sat_sat, lum_lum, sat_lum, hue_fx,
            ],
            draw_curve_only: false,
        };
        res.validate()?;
        Ok(res)
    }

    pub fn curve(&self, c: HueCurveType) -> &GradingBSplineCurve {
        &self.curves[c as usize]
    }
    pub fn curve_mut(&mut self, c: HueCurveType) -> &mut GradingBSplineCurve {
        &mut self.curves[c as usize]
    }

    /// Validate the curves (port of `GradingHueCurveImpl::validate`).
    pub fn validate(&self) -> Result<()> {
        for c in HueCurveType::ALL {
            let curve = self.curve(c);
            curve.validate().map_err(|e| {
                Error::msg(format!(
                    "GradingHueCurve validation failed for '{}' curve with: {}",
                    hue_curve_type_name(c),
                    e.message()
                ))
            })?;
            // Unless drawCurveOnly is enabled, check that the spline type is correct for
            // the given hue curve type.
            if !self.draw_curve_only && curve.spline_type != bspline_type_for_hue_curve_type(c) {
                crate::bail!(
                    "GradingHueCurve validation failed: '{}' curve is of the wrong BSplineType.",
                    hue_curve_type_name(c)
                );
            }
        }
        Ok(())
    }

    /// True if all curves are identities.
    pub fn is_identity(&self) -> bool {
        self.curves.iter().all(|c| c.is_identity())
    }
}

impl fmt::Display for GradingHueCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        for (i, c) in HueCurveType::ALL.iter().enumerate() {
            let sep = if i == 0 { "<" } else { ", " };
            write!(f, "{sep}{}={:.p$}", hue_curve_type_name(*c), self.curve(*c))?;
        }
        f.write_str(">")
    }
}

// ---------------------------------------------------------------------------
// GradingRgbmsw / GradingTone

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
        Self {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            master: 1.0,
            start: 0.0,
            width: 1.0,
        }
    }
}

impl GradingRgbmsw {
    pub const fn new(red: f64, green: f64, blue: f64, master: f64, start: f64, width: f64) -> Self {
        Self {
            red,
            green,
            blue,
            master,
            start,
            width,
        }
    }
    /// Identity RGBM with the given start / width.
    pub const fn with_start_width(start: f64, width: f64) -> Self {
        Self {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            master: 1.0,
            start,
            width,
        }
    }
    /// True if the red, green, blue and master values are all 1.
    pub fn is_identity(&self) -> bool {
        self.red == 1.0 && self.green == 1.0 && self.blue == 1.0 && self.master == 1.0
    }
}

impl fmt::Display for GradingRgbmsw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(
            f,
            "<red={} green={} blue={} master={} start={} width={}>",
            fmt_g(self.red, p),
            fmt_g(self.green, p),
            fmt_g(self.blue, p),
            fmt_g(self.master, p),
            fmt_g(self.start, p),
            fmt_g(self.width, p)
        )
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

    /// True if the values do not modify the image (port of `IsIdentity`).
    pub fn is_identity(&self) -> bool {
        self.blacks.is_identity()
            && self.shadows.is_identity()
            && self.midtones.is_identity()
            && self.highlights.is_identity()
            && self.whites.is_identity()
            && self.s_contrast == 1.0
    }

    /// Validate the values (port of `GradingTone::validate`).
    ///
    /// Client apps are expected to limit the values to these bounds:
    /// blacks, mids, whites: [0.1, 1.9]; shadows, highlights: [0.2, 1.8];
    /// min width: 0.01; s-contrast: [0.01, 1.99].
    pub fn validate(&self) -> Result<()> {
        const MIN_BMW: f64 = 0.1;
        const MAX_BMW: f64 = 1.9;
        const MIN_SH: f64 = 0.2;
        const MAX_SH: f64 = 1.8;
        const MIN_WSC: f64 = 0.01;
        const MAX_SC: f64 = 1.99;
        // The bounds are widened slightly to avoid failures due to precision issues.
        const ERR: f64 = 0.000001;
        const MIN_BMW_TOL: f64 = MIN_BMW - ERR;
        const MAX_BMW_TOL: f64 = MAX_BMW + ERR;
        const MIN_SH_TOL: f64 = MIN_SH - ERR;
        const MAX_SH_TOL: f64 = MAX_SH + ERR;
        const MIN_WSC_TOL: f64 = MIN_WSC - ERR;
        const MAX_SC_TOL: f64 = MAX_SC + ERR;

        let g = |v: f64| fmt_g(v, DEFAULT_PRECISION);
        let below =
            |v: &GradingRgbmsw, m: f64| v.red < m || v.green < m || v.blue < m || v.master < m;
        let above =
            |v: &GradingRgbmsw, m: f64| v.red > m || v.green > m || v.blue > m || v.master > m;

        // Blacks, midtones, whites.
        for (name, above_name, v) in [
            ("blacks", "blacks", &self.blacks),
            ("midtones", "midtones", &self.midtones),
            ("whites", "white", &self.whites),
        ] {
            if below(v, MIN_BMW_TOL) {
                crate::bail!(
                    "GradingTone {name} '{v}' are below lower bound ({}).",
                    g(MIN_BMW)
                );
            }
            if v.width < MIN_WSC_TOL {
                crate::bail!(
                    "GradingTone {name} width '{}' is below lower bound ({}).",
                    g(v.width),
                    g(MIN_WSC)
                );
            }
            if above(v, MAX_BMW_TOL) {
                crate::bail!(
                    "GradingTone {above_name} '{v}' are above upper bound ({}).",
                    g(MAX_BMW)
                );
            }
        }
        {
            let v = &self.shadows;
            if below(v, MIN_SH_TOL) {
                crate::bail!(
                    "GradingTone shadows '{v}' are below lower bound ({}).",
                    g(MIN_SH)
                );
            }
            // Check that pivot is not overlapping start.
            if v.start < v.width + MIN_WSC_TOL {
                crate::bail!(
                    "GradingTone shadows start '{}' is less than pivot ('{}' + {}).",
                    g(v.start),
                    g(v.width),
                    g(MIN_WSC)
                );
            }
            if above(v, MAX_SH_TOL) {
                crate::bail!(
                    "GradingTone shadows '{v}' are above upper bound ({}).",
                    g(MAX_SH)
                );
            }
        }
        {
            let v = &self.highlights;
            if below(v, MIN_SH_TOL) {
                crate::bail!(
                    "GradingTone highlights '{v}' are below lower bound ({}).",
                    g(MIN_SH)
                );
            }
            // Check that pivot is not overlapping start.
            if v.start > v.width - MIN_WSC_TOL {
                crate::bail!(
                    "GradingTone highlights start '{}' is greater than pivot ('{}' - {}).",
                    g(v.start),
                    g(v.width),
                    g(MIN_WSC)
                );
            }
            if above(v, MAX_SH_TOL) {
                crate::bail!(
                    "GradingTone highlights '{v}' are above upper bound ({}).",
                    g(MAX_SH)
                );
            }
        }
        if self.s_contrast < MIN_WSC_TOL {
            crate::bail!(
                "GradingTone s-contrast '{}' is below lower bound ({}).",
                g(self.s_contrast),
                g(MIN_WSC)
            );
        }
        if self.s_contrast > MAX_SC_TOL {
            crate::bail!(
                "GradingTone s-contrast '{}' is above upper bound ({}).",
                g(self.s_contrast),
                g(MAX_SC)
            );
        }
        Ok(())
    }
}

impl fmt::Display for GradingTone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = prec(f);
        write!(f, "<blacks={:.p$}", self.blacks)?;
        write!(f, " shadows={:.p$}", self.shadows)?;
        write!(f, " midtones={:.p$}", self.midtones)?;
        write!(f, " highlights={:.p$}", self.highlights)?;
        write!(f, " whites={:.p$}", self.whites)?;
        write!(f, " s_contrast={}", fmt_g(self.s_contrast, p))?;
        f.write_str(">")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_g() {
        assert_eq!(fmt_g(0.0001, 6), "0.0001");
        assert_eq!(fmt_g(1.0, 6), "1");
        assert_eq!(fmt_g(0.01, 6), "0.01");
        assert_eq!(fmt_g(-0.2, 7), "-0.2");
        assert_eq!(fmt_g((1.0f32 / 6.0) as f64, 6), "0.166667");
        assert_eq!(fmt_g((5.0f32 / 6.0) as f64, 6), "0.833333");
        assert_eq!(fmt_g(0.2f32 as f64, 6), "0.2");
        assert_eq!(fmt_g(0.2f32 as f64, 7), "0.2");
        assert_eq!(fmt_g(1e-5, 6), "1e-05");
        assert_eq!(fmt_g(1234567.0, 6), "1.23457e+06");
        assert_eq!(fmt_g(123456.0, 6), "123456");
        assert_eq!(fmt_g(f64::MAX, 6), "1.79769e+308");
        assert_eq!(fmt_g(-7.0, 6), "-7");
    }

    // GradingPrimary_tests.cpp

    #[test]
    fn grading_primary_basic() {
        let rgbm0 = GradingRgbm::default();
        assert_eq!(rgbm0, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
        let rgbm1 = GradingRgbm::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(rgbm1.red, 1.0);
        assert_eq!(rgbm1.green, 2.0);
        assert_eq!(rgbm1.blue, 3.0);
        assert_eq!(rgbm1.master, 4.0);
        let mut rgbm2 = rgbm1;
        assert_eq!(rgbm1, rgbm2);
        rgbm2.red += 0.1111;
        assert_ne!(rgbm1, rgbm2);

        let gp_log = GradingPrimary::new(GradingStyle::Log);
        let gp_lin = GradingPrimary::new(GradingStyle::Lin);
        let gp_vid = GradingPrimary::new(GradingStyle::Video);
        assert_eq!(gp_lin, gp_vid);
        assert_ne!(gp_log, gp_lin);
    }

    #[test]
    fn grading_primary_validate() {
        let mut gp = GradingPrimary::new(GradingStyle::Log);
        assert!(gp.validate(GradingStyle::Log).is_ok());

        // LOG & VIDEO have to keep gamma above a threshold.
        gp.gamma.red = 0.0001;
        let msg = "GradingPrimary gamma '<r=0.0001, g=1, b=1, m=1>' are below lower bound (0.01)";
        assert!(gp
            .validate(GradingStyle::Log)
            .unwrap_err()
            .message()
            .contains(msg));
        assert!(gp
            .validate(GradingStyle::Video)
            .unwrap_err()
            .message()
            .contains(msg));
        // LIN does not use gamma.
        assert!(gp.validate(GradingStyle::Lin).is_ok());
        gp.gamma.red = 1.0;

        // LIN has to keep contrast above a threshold.
        gp.contrast.green = 0.0001;
        let msg =
            "GradingPrimary contrast '<r=1, g=0.0001, b=1, m=1>' are below lower bound (0.01)";
        assert!(gp
            .validate(GradingStyle::Lin)
            .unwrap_err()
            .message()
            .contains(msg));
        // LOG can use any contrast value and VIDEO does not use contrast.
        assert!(gp.validate(GradingStyle::Log).is_ok());
        assert!(gp.validate(GradingStyle::Video).is_ok());
        gp.contrast.green = 1.0;

        gp.pivot_black = 0.5;
        gp.pivot_white = 0.4;
        assert!(gp
            .validate(GradingStyle::Log)
            .unwrap_err()
            .message()
            .contains("black pivot should be smaller than white pivot"));
        gp.pivot_black = 0.0;
        gp.clamp_black = 0.5;
        gp.clamp_white = 0.4;
        assert!(gp
            .validate(GradingStyle::Log)
            .unwrap_err()
            .message()
            .contains("black clamp should be smaller than white clamp"));
    }

    #[test]
    fn grading_primary_display() {
        let mut gp = GradingPrimary::new(GradingStyle::Log);
        assert_eq!(
            format!("{gp}"),
            "<brightness=<r=0, g=0, b=0, m=0>, contrast=<r=1, g=1, b=1, m=1>, \
             gamma=<r=1, g=1, b=1, m=1>, offset=<r=0, g=0, b=0, m=0>, \
             exposure=<r=0, g=0, b=0, m=0>, lift=<r=0, g=0, b=0, m=0>, \
             gain=<r=1, g=1, b=1, m=1>, saturation=1, pivot=<contrast=-0.2, black=0, white=1>>"
        );
        gp.clamp_black = 0.1;
        gp.clamp_white = 0.9;
        assert!(format!("{gp}").ends_with("white=1>, clampBlack=0.1, clampWhite=0.9>"));
    }

    // GradingTone_tests.cpp

    #[test]
    fn grading_tone_basic() {
        let rgbm0 = GradingRgbmsw::default();
        assert_eq!(rgbm0, GradingRgbmsw::new(1.0, 1.0, 1.0, 1.0, 0.0, 1.0));
        let rgbm1 = GradingRgbmsw::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        let mut rgbm2 = rgbm1;
        assert_eq!(rgbm1, rgbm2);
        rgbm2.red += 0.1111;
        assert_ne!(rgbm1, rgbm2);

        let tone_log = GradingTone::new(GradingStyle::Log);
        assert_eq!(
            tone_log.blacks,
            GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.4)
        );
        assert_eq!(
            tone_log.shadows,
            GradingRgbmsw::new(1., 1., 1., 1., 0.5, 0.)
        );
        assert_eq!(
            tone_log.midtones,
            GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.6)
        );
        assert_eq!(
            tone_log.highlights,
            GradingRgbmsw::new(1., 1., 1., 1., 0.3, 1.)
        );
        assert_eq!(
            tone_log.whites,
            GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.5)
        );
        assert_eq!(tone_log.s_contrast, 1.);
        let tone_lin = GradingTone::new(GradingStyle::Lin);
        assert_eq!(tone_lin.blacks, GradingRgbmsw::new(1., 1., 1., 1., 0., 4.));
        assert_eq!(
            tone_lin.shadows,
            GradingRgbmsw::new(1., 1., 1., 1., 2., -7.)
        );
        assert_eq!(
            tone_lin.midtones,
            GradingRgbmsw::new(1., 1., 1., 1., 0., 8.)
        );
        assert_eq!(
            tone_lin.highlights,
            GradingRgbmsw::new(1., 1., 1., 1., -2., 9.)
        );
        assert_eq!(tone_lin.whites, GradingRgbmsw::new(1., 1., 1., 1., 0., 8.));
        assert_eq!(tone_lin.s_contrast, 1.);
        let tone_vid = GradingTone::new(GradingStyle::Video);
        assert_eq!(
            tone_vid.blacks,
            GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.4)
        );
        assert_eq!(
            tone_vid.shadows,
            GradingRgbmsw::new(1., 1., 1., 1., 0.6, 0.)
        );
        assert_eq!(
            tone_vid.midtones,
            GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.7)
        );
        assert_eq!(
            tone_vid.highlights,
            GradingRgbmsw::new(1., 1., 1., 1., 0.2, 1.)
        );
        assert_eq!(
            tone_vid.whites,
            GradingRgbmsw::new(1., 1., 1., 1., 0.5, 0.5)
        );
        assert_eq!(tone_vid.s_contrast, 1.);

        let mut gt1 = GradingTone::new(GradingStyle::Log);
        gt1.midtones.start = 0.1;
        let gt2 = gt1;
        assert_eq!(gt1, gt2);
        gt1.highlights.red += 0.1111;
        assert_ne!(gt1, gt2);
    }

    #[test]
    fn grading_tone_validate() {
        let mut tone = GradingTone::new(GradingStyle::Log);
        assert!(tone.validate().is_ok());
        let check = |t: &GradingTone, s: &str| {
            let e = t.validate().unwrap_err();
            assert!(e.message().contains(s), "{}", e.message());
        };

        let temp = tone.blacks.red;
        tone.blacks.red = 0.08;
        check(&tone, "are below lower bound");
        tone.blacks.red = temp;

        let temp = tone.midtones.width;
        tone.midtones.width = 0.001;
        check(&tone, "is below lower bound");
        tone.midtones.width = temp;

        let temp = tone.whites.blue;
        tone.whites.blue = 2.;
        check(&tone, "are above upper bound");
        tone.whites.blue = temp;

        let temp = tone.shadows.master;
        tone.shadows.master = 0.15;
        check(&tone, "are below lower bound");
        tone.shadows.master = temp;

        let temp = tone.highlights.green;
        tone.highlights.green = 1.9;
        check(&tone, "are above upper bound");
        tone.highlights.green = temp;

        let temp = tone.s_contrast;
        tone.s_contrast = 2.;
        check(&tone, "is above upper bound");
        tone.s_contrast = temp;
        assert!(tone.validate().is_ok());
    }

    // GradingBSplineCurve_tests.cpp

    #[test]
    fn bspline_curve_basic() {
        let mut curve = GradingBSplineCurve::with_size(3, BSplineType::BSpline);
        assert_eq!(3, curve.num_control_points());
        assert_eq!(0.0, curve.control_point(0).unwrap().x);
        assert_eq!(0.0, curve.control_point(0).unwrap().y);
        *curve.control_point_mut(1).unwrap() = GradingControlPoint::new(0.5, 0.4);
        *curve.control_point_mut(2).unwrap() = GradingControlPoint::new(1.0, 0.9);
        assert_eq!(0.5, curve.control_point(1).unwrap().x);
        assert_eq!(0.4, curve.control_point(1).unwrap().y);
        assert_eq!(1.0, curve.control_point(2).unwrap().x);
        assert_eq!(0.9, curve.control_point(2).unwrap().y);

        assert!(curve.slopes_are_default());
        curve.set_slope(2, 0.9);
        assert_eq!(0.9, curve.try_slope(2).unwrap());
        assert!(!curve.slopes_are_default());

        curve.set_num_control_points(4);
        assert_eq!(4, curve.num_control_points());
        assert_eq!(
            GradingControlPoint::new(0.0, 0.0),
            *curve.control_point(3).unwrap()
        );

        let mut curve = GradingBSplineCurve::new(
            &[(0.0, 0.0), (0.2, 0.3), (0.5, 0.7), (1.0, 1.0)],
            BSplineType::BSpline,
        );
        assert_eq!(4, curve.num_control_points());
        assert_eq!(0.2, curve.control_point(1).unwrap().x);
        assert_eq!(0.3, curve.control_point(1).unwrap().y);
        assert_eq!(
            curve.control_point(42).unwrap_err().message(),
            "There are '4' control points. '42' is out of bounds."
        );
        assert_eq!(
            curve.try_set_slope(42, 0.2).unwrap_err().message(),
            "There are '4' control points. '42' is out of bounds."
        );
        assert_eq!(
            format!("{curve}"),
            "<control_points=[<x=0, y=0><x=0.2, y=0.3><x=0.5, y=0.7><x=1, y=1>]>"
        );
        curve.set_slope(1, 2.0);
        assert_eq!(
            format!("{curve}"),
            "<control_points=[<x=0, y=0, slp=0><x=0.2, y=0.3, slp=2><x=0.5, y=0.7, slp=0><x=1, y=1, slp=0>]>"
        );
    }

    #[test]
    fn bspline_curve_validate() {
        let curve = GradingBSplineCurve::with_size(1, BSplineType::BSpline);
        assert_eq!(
            curve.validate().unwrap_err().message(),
            "There must be at least 2 control points."
        );
        let mut curve = GradingBSplineCurve::new(
            &[(0.0, 0.0), (0.7, 0.3), (0.5, 0.7), (1.0, 1.0)],
            BSplineType::BSpline,
        );
        assert!(curve.validate().unwrap_err().message().contains(
            "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
        ));
        curve.control_points[1].x = 0.3;
        assert!(curve.validate().is_ok());

        curve.slopes.pop();
        assert_eq!(
            curve.validate().unwrap_err().message(),
            "The slopes array must be the same length as the control points."
        );

        // Diagonal types must be non-decreasing in y.
        let curve = GradingBSplineCurve::new(
            &[(0.0, 0.5), (0.5, 0.2), (1.0, 1.0)],
            BSplineType::DiagonalBSpline,
        );
        assert!(curve.validate().unwrap_err().message().contains(
            "has a y coordinate '0.2' that is less than previous control point y coordinate '0.5'."
        ));
        // But not horizontal ones.
        let curve = GradingBSplineCurve::new(
            &[(0.0, 0.5), (0.5, 0.2), (1.0, 1.0)],
            BSplineType::Horizontal1BSpline,
        );
        assert!(curve.validate().is_ok());

        let curve =
            GradingBSplineCurve::new(&[(-0.1, 0.0), (0.5, 0.5)], BSplineType::HueHueBSpline);
        assert_eq!(
            curve.validate().unwrap_err().message(),
            "The HUE-HUE spline may not have negative x coordinates."
        );
        let curve = GradingBSplineCurve::new(&[(0.1, 0.1), (1.5, 1.0)], BSplineType::HueHueBSpline);
        assert_eq!(
            curve.validate().unwrap_err().message(),
            "The HUE-HUE spline may not have x coordinates greater than one."
        );
        let curve =
            GradingBSplineCurve::new(&[(0.0, 1.0), (1.0, 1.0)], BSplineType::Periodic1BSpline);
        assert_eq!(
            curve.validate().unwrap_err().message(),
            "The periodic spline x coordinates may not wrap to the same value."
        );
    }

    #[test]
    fn bspline_curve_equals() {
        let pts = [(0.0, 0.0), (0.2, 0.3), (0.5, 0.7), (1.0, 1.0)];
        let curve1 = GradingBSplineCurve::new(&pts, BSplineType::BSpline);
        let mut curve2 = GradingBSplineCurve::new(&pts, BSplineType::BSpline);
        assert_eq!(curve1, curve2);
        let curve3 = GradingBSplineCurve::new(&pts, BSplineType::DiagonalBSpline);
        assert_ne!(curve1, curve3);
        curve2.set_slope(3, 0.9);
        assert!(curve2.validate().is_ok());
        assert_ne!(curve1, curve2);
        let mut curve4 = GradingBSplineCurve::new(&pts, BSplineType::BSpline);
        assert_eq!(curve1, curve4);
        curve4.control_points[2].y = 0.9;
        assert_ne!(curve1, curve4);
    }

    // GradingRGBCurve_tests.cpp

    #[test]
    fn rgb_curve_basic() {
        let mut curve = GradingBSplineCurve::new(
            &[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)],
            BSplineType::BSpline,
        );
        curve.control_points[1].y = 0.3;
        let rgb = GradingRgbCurve::from_curves(
            curve.clone(),
            GradingBSplineCurve::with_size(4, BSplineType::BSpline),
            GradingBSplineCurve::with_size(3, BSplineType::BSpline),
            GradingBSplineCurve::with_size(2, BSplineType::BSpline),
        );
        // The curves are copies.
        curve.control_points[1].y = 0.4;
        assert_eq!(0.3, rgb.curve(RgbCurveType::Red).control_points[1].y);

        let lin = GradingRgbCurve::new(GradingStyle::Lin);
        let log = GradingRgbCurve::new(GradingStyle::Log);
        let vid = GradingRgbCurve::new(GradingStyle::Video);
        assert_eq!(log, vid);
        assert_ne!(log, lin);
        for c in RgbCurveType::ALL {
            assert_eq!(log.curve(RgbCurveType::Red), log.curve(c));
            assert_eq!(lin.curve(RgbCurveType::Red), lin.curve(c));
        }
        let r = log.curve(RgbCurveType::Red);
        assert_eq!(3, r.num_control_points());
        assert_eq!(
            r.control_points,
            vec![
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(0.5, 0.5),
                GradingControlPoint::new(1.0, 1.0)
            ]
        );
        let r = lin.curve(RgbCurveType::Red);
        assert_eq!(
            r.control_points,
            vec![
                GradingControlPoint::new(-7.0, -7.0),
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(7.0, 7.0)
            ]
        );
        let copy = lin.clone();
        assert_eq!(lin, copy);

        assert_eq!(
            format!("{lin}"),
            "<red=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
             green=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
             blue=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
             master=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>>"
        );
    }

    #[test]
    fn rgb_curve_curves() {
        let mut curves = GradingRgbCurve::new(GradingStyle::Video);
        assert!(curves.is_identity());
        let spline = curves.curve_mut(RgbCurveType::Green);
        spline.set_num_control_points(4);
        spline.control_points[3] = GradingControlPoint::new(1.1, 2.0);
        assert!(!curves.is_identity());
        curves.curve_mut(RgbCurveType::Green).control_points[3].x = 2.0;
        assert!(curves.is_identity());
        assert_eq!(curves.curve(RgbCurveType::Green).num_control_points(), 4);
    }

    #[test]
    fn rgb_curve_validate() {
        let mut curves = GradingRgbCurve::new(GradingStyle::Log);
        assert!(curves.validate().is_ok());
        curves.curve_mut(RgbCurveType::Blue).control_points[1].x = 1.5;
        assert_eq!(
            curves.validate().unwrap_err().message(),
            "GradingRGBCurve validation failed for 'blue' curve with: Control point at index 2 has a x \
             coordinate '1' that is less than previous control point x coordinate '1.5'."
        );
        let mut curves = GradingRgbCurve::new(GradingStyle::Log);
        curves.curve_mut(RgbCurveType::Master).spline_type = BSplineType::DiagonalBSpline;
        assert_eq!(
            curves.validate().unwrap_err().message(),
            "GradingRGBCurve validation failed: 'master' curve is of the wrong BSplineType."
        );
    }

    // GradingHueCurve_tests.cpp

    #[test]
    fn hue_curve_basic() {
        use HueCurveType as H;
        let mut curve = GradingBSplineCurve::new_for_hue_curve(
            &[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)],
            H::HueHue,
        );
        curve.control_points[1].y = 0.3;
        let hh = curve.clone();
        let hs = GradingBSplineCurve::with_size_for_hue_curve(4, H::HueSat);
        let hl = GradingBSplineCurve::with_size_for_hue_curve(3, H::HueLum);
        let ls = GradingBSplineCurve::with_size_for_hue_curve(2, H::LumSat);
        let ss = GradingBSplineCurve::with_size_for_hue_curve(2, H::SatSat);
        let ll = GradingBSplineCurve::with_size_for_hue_curve(2, H::LumLum);
        let sl = GradingBSplineCurve::with_size_for_hue_curve(2, H::SatLum);
        let hfx = GradingBSplineCurve::with_size_for_hue_curve(2, H::HueFx);

        let bad = GradingHueCurve::from_curves(
            hh.clone(),
            hs.clone(),
            hl.clone(),
            ls.clone(),
            hh.clone(),
            ll.clone(),
            sl.clone(),
            hfx.clone(),
        );
        assert_eq!(
            bad.unwrap_err().message(),
            "GradingHueCurve validation failed: 'sat_sat' curve is of the wrong BSplineType."
        );

        let mut hue_curve = GradingHueCurve::from_curves(hh, hs, hl, ls, ss, ll, sl, hfx).unwrap();
        assert!(hue_curve.validate().is_ok());
        curve.control_points[1].y = 0.4;
        assert_eq!(0.3, hue_curve.curve(H::HueHue).control_points[1].y);

        hue_curve.curve_mut(H::HueHue).spline_type = BSplineType::DiagonalBSpline;
        assert_eq!(
            hue_curve.validate().unwrap_err().message(),
            "GradingHueCurve validation failed: 'hue_hue' curve is of the wrong BSplineType."
        );
        hue_curve.draw_curve_only = true;
        assert!(hue_curve.validate().is_ok());

        let lin = GradingHueCurve::new(GradingStyle::Lin);
        let log = GradingHueCurve::new(GradingStyle::Log);
        let vid = GradingHueCurve::new(GradingStyle::Video);
        assert_eq!(log, vid);
        assert_ne!(log, lin);
        assert_eq!(log.curve(H::HueLum), log.curve(H::HueSat));
        assert_eq!(log.curve(H::SatSat), log.curve(H::LumLum));
        assert_eq!(log.curve(H::LumSat), log.curve(H::SatLum));
        assert_ne!(log.curve(H::HueHue), log.curve(H::HueSat));
        assert_eq!(
            log.curve(H::LumLum).control_points,
            vec![
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(0.5, 0.5),
                GradingControlPoint::new(1.0, 1.0)
            ]
        );
        assert_eq!(lin.curve(H::HueLum), lin.curve(H::HueSat));
        assert_ne!(lin.curve(H::SatSat), lin.curve(H::LumLum));
        assert_ne!(lin.curve(H::LumSat), lin.curve(H::SatLum));
        assert_eq!(
            lin.curve(H::LumLum).control_points,
            vec![
                GradingControlPoint::new(-7.0, -7.0),
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(7.0, 7.0)
            ]
        );

        let mut lin = lin;
        assert!(!lin.draw_curve_only);
        lin.draw_curve_only = true;
        let copy = lin.clone();
        assert_eq!(lin, copy);
        assert!(copy.draw_curve_only);

        assert_eq!(
            format!("{lin}"),
            "<hue_hue=<control_points=[<x=0, y=0><x=0.166667, y=0.166667>\
             <x=0.333333, y=0.333333><x=0.5, y=0.5><x=0.666667, y=0.666667><x=0.833333, y=0.833333>]>, \
             hue_sat=<control_points=[<x=0, y=1><x=0.166667, y=1><x=0.333333, y=1><x=0.5, y=1>\
             <x=0.666667, y=1><x=0.833333, y=1>]>, \
             hue_lum=<control_points=[<x=0, y=1><x=0.166667, y=1><x=0.333333, y=1><x=0.5, y=1>\
             <x=0.666667, y=1><x=0.833333, y=1>]>, \
             lum_sat=<control_points=[<x=-7, y=1><x=0, y=1><x=7, y=1>]>, \
             sat_sat=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
             lum_lum=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
             sat_lum=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
             hue_fx=<control_points=[<x=0, y=0><x=0.166667, y=0><x=0.333333, y=0><x=0.5, y=0>\
             <x=0.666667, y=0><x=0.833333, y=0>]>>"
        );
    }

    #[test]
    fn hue_curve_curves() {
        let mut curves = GradingHueCurve::new(GradingStyle::Video);
        assert!(curves.is_identity());
        assert_eq!(curves.curve(HueCurveType::HueSat).num_control_points(), 6);
        let spline = curves.curve_mut(HueCurveType::HueSat);
        spline.control_points[3] = GradingControlPoint::new(0.9, 1.1);
        assert!(!curves.is_identity());
        curves.curve_mut(HueCurveType::HueSat).control_points[3].y = 1.0;
        assert!(curves.is_identity());
        curves
            .curve_mut(HueCurveType::HueSat)
            .set_num_control_points(4);
        assert_eq!(4, curves.curve(HueCurveType::HueSat).num_control_points());
    }
}
