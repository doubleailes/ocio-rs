//! Log op (port of `LogOpData`, `LogOp`, `LogOpCPU`, `LogUtils` and the op
//! building part of `LogTransform.cpp`, `LogAffineTransform.cpp` and
//! `LogCameraTransform.cpp`).
//!
//! The op computes, per RGB channel (alpha is untouched):
//!
//! * forward (lin to log): `logSlope * log(linSlope * x + linOffset, base) + logOffset`
//! * inverse (log to lin): `(base^((x - logOffset) / logSlope) - linOffset) / linSlope`
//!
//! Camera logs add a linear segment below `linSideBreak`.

pub mod log_utils;

use crate::config::Config;
use crate::context::Context;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::math_utils::is_scalar_equal_to_zero;
use crate::ops::matrix::float_format::format_g;
use crate::ops::range::{RangeOp, RangeOpData};
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{
    BuildOps, LogAffineTransform, LogCameraTransform, LogTransform, Transform, Validate,
};
use crate::types::{OptimizationFlags, TransformDirection};
use std::any::Any;
use std::sync::Arc;

/// Index of the log side slope in the log parameters.
pub const LOG_SIDE_SLOPE: usize = 0;
/// Index of the log side offset in the log parameters.
pub const LOG_SIDE_OFFSET: usize = 1;
/// Index of the lin side slope in the log parameters.
pub const LIN_SIDE_SLOPE: usize = 2;
/// Index of the lin side offset in the log parameters.
pub const LIN_SIDE_OFFSET: usize = 3;
/// Index of the lin side break in the log parameters (camera logs).
pub const LIN_SIDE_BREAK: usize = 4;
/// Index of the linear slope in the log parameters (camera logs, optional).
pub const LINEAR_SLOPE: usize = 5;

const FLOAT_DECIMALS: usize = 7;

/// Parameters of a [`LogOp`] (port of OCIO's `LogOpData`).
///
/// Each channel holds 4 (log affine), 5 (camera log) or 6 (camera log with
/// linear slope) parameters, indexed by [`LOG_SIDE_SLOPE`], ...
#[derive(Debug, Clone)]
pub struct LogOpData {
    /// Base of the logarithm.
    pub base: f64,
    /// Red parameters.
    pub red: Vec<f64>,
    /// Green parameters.
    pub green: Vec<f64>,
    /// Blue parameters.
    pub blue: Vec<f64>,
    /// Direction (forward: lin to log).
    pub direction: TransformDirection,
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl PartialEq for LogOpData {
    /// Equality as defined by OCIO (metadata ignored).
    fn eq(&self, other: &Self) -> bool {
        self.direction == other.direction
            && self.base == other.base
            && self.red == other.red
            && self.green == other.green
            && self.blue == other.blue
    }
}

fn validate_params(params: &[f64]) -> Result<()> {
    if params.len() < 4 {
        crate::bail!("Log: expecting at least 4 parameters.");
    }
    if params.len() > 6 {
        crate::bail!("Log: expecting at most 6 parameters.");
    }
    if is_scalar_equal_to_zero(params[LIN_SIDE_SLOPE]) {
        crate::bail!(
            "Log: Invalid linear side slope value '{}', linear side slope cannot be 0.",
            format_g(params[LIN_SIDE_SLOPE], 6)
        );
    }
    if is_scalar_equal_to_zero(params[LOG_SIDE_SLOPE]) {
        crate::bail!(
            "Log: Invalid log side slope value '{}', log side slope cannot be 0.",
            format_g(params[LOG_SIDE_SLOPE], 6)
        );
    }
    Ok(())
}

impl LogOpData {
    /// A simple log of the given base (default parameters).
    pub fn from_base(base: f64, direction: TransformDirection) -> Self {
        let p = vec![1.0, 0.0, 1.0, 0.0];
        Self {
            base,
            red: p.clone(),
            green: p.clone(),
            blue: p,
            direction,
            metadata: FormatMetadata::default(),
        }
    }

    /// A log affine from the per channel slopes and offsets.
    pub fn from_affine(
        base: f64,
        log_slope: &[f64; 3],
        log_offset: &[f64; 3],
        lin_slope: &[f64; 3],
        lin_offset: &[f64; 3],
        direction: TransformDirection,
    ) -> Self {
        let p = |i: usize| vec![log_slope[i], log_offset[i], lin_slope[i], lin_offset[i]];
        Self {
            base,
            red: p(0),
            green: p(1),
            blue: p(2),
            direction,
            metadata: FormatMetadata::default(),
        }
    }

    /// A log from explicit per channel parameters. All channels must have the
    /// same style (log affine or camera).
    pub fn new(
        base: f64,
        red: Vec<f64>,
        green: Vec<f64>,
        blue: Vec<f64>,
        direction: TransformDirection,
    ) -> Result<Self> {
        let (sr, sg, sb) = (red.len(), green.len(), blue.len());
        if (sr >= 4 || sg >= 4 || sb >= 4) && (sr < 4 || sg < 4 || sb < 4) {
            crate::bail!("Cannot create Log op, all channels need to have the same style.");
        }
        Ok(Self {
            base,
            red,
            green,
            blue,
            direction,
            metadata: FormatMetadata::default(),
        })
    }

    /// Build from a [`LogTransform`].
    pub fn from_log_transform(t: &LogTransform) -> Self {
        let mut d = Self::from_base(t.base, t.direction);
        d.metadata = t.metadata.clone();
        d
    }

    /// Build from a [`LogAffineTransform`].
    pub fn from_log_affine_transform(t: &LogAffineTransform) -> Self {
        let mut d = Self::from_affine(
            t.base,
            &t.log_side_slope,
            &t.log_side_offset,
            &t.lin_side_slope,
            &t.lin_side_offset,
            t.direction,
        );
        d.metadata = t.metadata.clone();
        d
    }

    /// Build from a [`LogCameraTransform`].
    pub fn from_log_camera_transform(t: &LogCameraTransform) -> Self {
        let mut d = Self::from_affine(
            t.base,
            &t.log_side_slope,
            &t.log_side_offset,
            &t.lin_side_slope,
            &t.lin_side_offset,
            t.direction,
        );
        d.set_value(LIN_SIDE_BREAK, &t.lin_side_break);
        if let Some(ls) = &t.linear_slope {
            d.set_value(LINEAR_SLOPE, ls);
        }
        d.metadata = t.metadata.clone();
        d
    }

    /// The parameters of a channel (0: red, 1: green, 2: blue).
    pub fn params(&self, channel: usize) -> &[f64] {
        match channel {
            0 => &self.red,
            1 => &self.green,
            _ => &self.blue,
        }
    }

    /// Set a parameter for the three channels (resizing the parameters when
    /// setting the lin side break or the linear slope).
    pub fn set_value(&mut self, index: usize, values: &[f64; 3]) {
        if index == LIN_SIDE_BREAK && self.red.len() < 5 {
            self.red.resize(5, 0.0);
            self.green.resize(5, 0.0);
            self.blue.resize(5, 0.0);
        } else if index == LINEAR_SLOPE && self.red.len() == 5 {
            self.red.resize(6, 0.0);
            self.green.resize(6, 0.0);
            self.blue.resize(6, 0.0);
        }
        if index < self.red.len() {
            self.red[index] = values[0];
        }
        if index < self.green.len() {
            self.green[index] = values[1];
        }
        if index < self.blue.len() {
            self.blue[index] = values[2];
        }
    }

    /// Get a parameter for the three channels, if defined.
    pub fn get_value(&self, index: usize) -> Option<[f64; 3]> {
        if index >= self.red.len() || index >= self.green.len() || index >= self.blue.len() {
            return None;
        }
        Some([self.red[index], self.green[index], self.blue[index]])
    }

    /// Remove the linear slope.
    pub fn unset_linear_slope(&mut self) {
        if self.red.len() == 6 {
            self.red.truncate(5);
            self.green.truncate(5);
            self.blue.truncate(5);
        }
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Validate the parameters and the base.
    pub fn validate(&self) -> Result<()> {
        validate_params(&self.red)?;
        validate_params(&self.green)?;
        validate_params(&self.blue)?;
        if self.red.len() != self.green.len() || self.red.len() != self.blue.len() {
            crate::bail!("Log: Red, green & blue parameters must have the same size.");
        }
        if self.base == 1.0 {
            crate::bail!(
                "Log: Invalid base value '{}', base cannot be 1.",
                format_g(self.base, 6)
            );
        } else if self.base <= 0.0 {
            crate::bail!(
                "Log: Invalid base value '{}', base must be greater than 0.",
                format_g(self.base, 6)
            );
        }
        Ok(())
    }

    /// A log is never an identity.
    pub fn is_identity(&self) -> bool {
        false
    }

    /// A log is never a no-op.
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// Logs have no channel crosstalk.
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// True if the three channels use the same parameters.
    pub fn all_components_equal(&self) -> bool {
        self.red == self.green && self.red == self.blue
    }

    /// True for a plain logarithm (default parameters, same for all channels).
    pub fn is_simple_log(&self) -> bool {
        self.all_components_equal()
            && self.red.len() == 4
            && self.red[LOG_SIDE_SLOPE] == 1.0
            && self.red[LIN_SIDE_SLOPE] == 1.0
            && self.red[LIN_SIDE_OFFSET] == 0.0
            && self.red[LOG_SIDE_OFFSET] == 0.0
    }

    fn is_log_base(&self, base: f64) -> bool {
        self.is_simple_log() && self.base == base
    }

    /// True for a plain base-2 logarithm.
    pub fn is_log2(&self) -> bool {
        self.is_log_base(2.0)
    }

    /// True for a plain base-10 logarithm.
    pub fn is_log10(&self) -> bool {
        self.is_log_base(10.0)
    }

    /// True for a camera log (with a linear segment).
    pub fn is_camera(&self) -> bool {
        self.red.len() > 4
    }

    /// Copy with the direction flipped.
    pub fn inverse(&self) -> Result<LogOpData> {
        let mut inv = self.clone();
        inv.direction = self.direction.inverse();
        inv.validate()?;
        Ok(inv)
    }

    /// True if `other` is the inverse of `self` (only when all the channels
    /// are equal).
    pub fn is_inverse(&self, other: &LogOpData) -> bool {
        self.direction.inverse() == other.direction
            && self.all_components_equal()
            && other.all_components_equal()
            && self.red == other.red
            && self.base == other.base
    }

    /// The op replacing a pair of inverse logs (emulating the clamping done
    /// by the pair): a range, or `None` for an identity (the pair is removed).
    pub fn identity_replacement(&self) -> Option<RangeOpData> {
        if self.is_log2() || self.is_log10() {
            match self.direction {
                // The first op logarithm is not defined for negative values.
                TransformDirection::Forward => {
                    RangeOpData::new(0.0, RangeOpData::EMPTY, 0.0, RangeOpData::EMPTY).ok()
                }
                // In practice the input of the following logarithm is clamped
                // to a very small positive number: consider it an exact inverse.
                TransformDirection::Inverse => None,
            }
        } else if !self.is_camera() {
            match self.direction {
                TransformDirection::Forward => {
                    // Minimum value allowed is -linOffset/linSlope so that
                    // linSlope * x + linOffset > 0.
                    let min_value = -self.red[LIN_SIDE_OFFSET] / self.red[LIN_SIDE_SLOPE];
                    RangeOpData::new(min_value, RangeOpData::EMPTY, min_value, RangeOpData::EMPTY)
                        .ok()
                }
                TransformDirection::Inverse => None,
            }
        } else {
            None
        }
    }

    fn parameter_string(&self, index: usize) -> String {
        if index >= self.red.len() {
            return String::new();
        }
        if self.all_components_equal() {
            format_g(self.red[index], FLOAT_DECIMALS)
        } else {
            format!(
                "{}, {}, {}",
                format_g(self.red[index], FLOAT_DECIMALS),
                format_g(self.green[index], FLOAT_DECIMALS),
                format_g(self.blue[index], FLOAT_DECIMALS)
            )
        }
    }

    /// Log side slope as a string (one value if all channels are equal).
    pub fn log_slope_string(&self) -> String {
        self.parameter_string(LOG_SIDE_SLOPE)
    }
    /// Log side offset as a string.
    pub fn log_offset_string(&self) -> String {
        self.parameter_string(LOG_SIDE_OFFSET)
    }
    /// Lin side slope as a string.
    pub fn lin_slope_string(&self) -> String {
        self.parameter_string(LIN_SIDE_SLOPE)
    }
    /// Lin side offset as a string.
    pub fn lin_offset_string(&self) -> String {
        self.parameter_string(LIN_SIDE_OFFSET)
    }
    /// Lin side break as a string.
    pub fn lin_break_string(&self) -> String {
        self.parameter_string(LIN_SIDE_BREAK)
    }
    /// Linear slope as a string.
    pub fn linear_slope_string(&self) -> String {
        self.parameter_string(LINEAR_SLOPE)
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        s.push_str(self.direction.as_str());
        s.push(' ');
        s.push_str(&format!("Base {} ", format_g(self.base, FLOAT_DECIMALS)));
        s.push_str(&format!("LogSideSlope {} ", self.log_slope_string()));
        s.push_str(&format!("LogSideOffset {} ", self.log_offset_string()));
        s.push_str(&format!("LinSideSlope {} ", self.lin_slope_string()));
        s.push_str(&format!("LinSideOffset {}", self.lin_offset_string()));
        if self.red.len() > 4 {
            s.push_str(&format!(" LinSideBreak {}", self.lin_break_string()));
            if self.red.len() > 5 {
                s.push_str(&format!(" LinearSlope {}", self.linear_slope_string()));
            }
        }
        s
    }
}

// ---------------------------------------------------------------------------
// CPU renderers.

const LOG2_10: f32 = std::f64::consts::LOG2_10 as f32;
const LOG10_2: f32 = std::f64::consts::LOG10_2 as f32;

/// CPU renderer chosen from the log parameters (as `GetLogRenderer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogRenderer {
    /// `log2(max(x, FLT_MIN)) * scale`.
    Log { scale: f32 },
    /// `exp2(x * log2(base))`.
    AntiLog { log2_base: f32 },
    /// Lin to log.
    Lin2Log {
        m: [f32; 3],
        b: [f32; 3],
        klog: [f32; 3],
        kb: [f32; 3],
    },
    /// Log to lin.
    Log2Lin {
        kinv: [f32; 3],
        minuskb: [f32; 3],
        minusb: [f32; 3],
        minv: [f32; 3],
    },
    /// Camera lin to log.
    CameraLin2Log {
        m: [f32; 3],
        b: [f32; 3],
        klog: [f32; 3],
        kb: [f32; 3],
        linb: [f32; 3],
        linear_slope: [f32; 3],
        linear_offset: [f32; 3],
    },
    /// Camera log to lin.
    CameraLog2Lin {
        kinv: [f32; 3],
        minuskb: [f32; 3],
        minusb: [f32; 3],
        minv: [f32; 3],
        log_side_break: [f32; 3],
        linsinv: [f32; 3],
        minuslino: [f32; 3],
    },
}

/// `std::max(a, b)` (returns `a` if `b` is NaN).
#[inline]
fn cmax(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

impl LogRenderer {
    /// Select and initialize the renderer.
    pub fn new(log: &LogOpData) -> Self {
        let dir = log.direction;
        if log.is_log2() {
            return match dir {
                TransformDirection::Forward => LogRenderer::Log { scale: 1.0 },
                TransformDirection::Inverse => LogRenderer::AntiLog { log2_base: 1.0 },
            };
        }
        if log.is_log10() {
            return match dir {
                TransformDirection::Forward => LogRenderer::Log { scale: LOG10_2 },
                TransformDirection::Inverse => LogRenderer::AntiLog { log2_base: LOG2_10 },
            };
        }
        let base = log.base as f32;
        let p = [log.params(0), log.params(1), log.params(2)];
        let arr = |f: &dyn Fn(&[f64]) -> f32| [f(p[0]), f(p[1]), f(p[2])];
        if log.is_camera() {
            let linear_slope = arr(&|q| log_utils::get_linear_slope(q, base as f64));
            let log_side_break = arr(&|q| log_utils::get_log_side_break(q, base as f64));
            let mut linear_offset = [0.0f32; 3];
            for i in 0..3 {
                linear_offset[i] =
                    log_utils::get_linear_offset(p[i], linear_slope[i], log_side_break[i]);
            }
            let log2_base = base.log2();
            match dir {
                TransformDirection::Forward => LogRenderer::CameraLin2Log {
                    m: arr(&|q| q[LIN_SIDE_SLOPE] as f32),
                    b: arr(&|q| q[LIN_SIDE_OFFSET] as f32),
                    klog: arr(&|q| (q[LOG_SIDE_SLOPE] / log2_base as f64) as f32),
                    kb: arr(&|q| q[LOG_SIDE_OFFSET] as f32),
                    linb: arr(&|q| q[LIN_SIDE_BREAK] as f32),
                    linear_slope,
                    linear_offset,
                },
                TransformDirection::Inverse => LogRenderer::CameraLog2Lin {
                    kinv: arr(&|q| log2_base / q[LOG_SIDE_SLOPE] as f32),
                    minuskb: arr(&|q| -(q[LOG_SIDE_OFFSET] as f32)),
                    minusb: arr(&|q| -(q[LIN_SIDE_OFFSET] as f32)),
                    minv: arr(&|q| 1.0f32 / q[LIN_SIDE_SLOPE] as f32),
                    log_side_break,
                    linsinv: [
                        1.0 / linear_slope[0],
                        1.0 / linear_slope[1],
                        1.0 / linear_slope[2],
                    ],
                    minuslino: [-linear_offset[0], -linear_offset[1], -linear_offset[2]],
                },
            }
        } else {
            match dir {
                TransformDirection::Forward => {
                    // log2(float) in C++ returns a float.
                    let log2_base = base.log2() as f64;
                    LogRenderer::Lin2Log {
                        m: arr(&|q| q[LIN_SIDE_SLOPE] as f32),
                        b: arr(&|q| q[LIN_SIDE_OFFSET] as f32),
                        klog: arr(&|q| (q[LOG_SIDE_SLOPE] / log2_base) as f32),
                        kb: arr(&|q| q[LOG_SIDE_OFFSET] as f32),
                    }
                }
                TransformDirection::Inverse => LogRenderer::Log2Lin {
                    kinv: arr(&|q| base.log2() / q[LOG_SIDE_SLOPE] as f32),
                    minuskb: arr(&|q| -(q[LOG_SIDE_OFFSET] as f32)),
                    minusb: arr(&|q| -(q[LIN_SIDE_OFFSET] as f32)),
                    minv: arr(&|q| 1.0f32 / q[LIN_SIDE_SLOPE] as f32),
                },
            }
        }
    }

    /// Process pixels in place (alpha is not modified).
    pub fn apply(&self, pixels: &mut [Pixel]) {
        const MIN_VALUE: f32 = f32::MIN_POSITIVE;
        match self {
            LogRenderer::Log { scale } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        *v = cmax(MIN_VALUE, *v).log2() * scale;
                    }
                }
            }
            LogRenderer::AntiLog { log2_base } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        *v = (*v * log2_base).exp2();
                    }
                }
            }
            LogRenderer::Lin2Log { m, b, klog, kb } => {
                for p in pixels.iter_mut() {
                    for i in 0..3 {
                        let v = p[i] * m[i] + b[i];
                        let v = cmax(MIN_VALUE, v).log2();
                        p[i] = v * klog[i] + kb[i];
                    }
                }
            }
            LogRenderer::Log2Lin {
                kinv,
                minuskb,
                minusb,
                minv,
            } => {
                for p in pixels.iter_mut() {
                    for i in 0..3 {
                        let v = ((p[i] + minuskb[i]) * kinv[i]).exp2();
                        p[i] = (v + minusb[i]) * minv[i];
                    }
                }
            }
            LogRenderer::CameraLin2Log {
                m,
                b,
                klog,
                kb,
                linb,
                linear_slope,
                linear_offset,
            } => {
                for p in pixels.iter_mut() {
                    for i in 0..3 {
                        let x = p[i];
                        p[i] = if x < linb[i] {
                            linear_slope[i] * x + linear_offset[i]
                        } else {
                            let v = x * m[i] + b[i];
                            let v = cmax(MIN_VALUE, v).log2();
                            v * klog[i] + kb[i]
                        };
                    }
                }
            }
            LogRenderer::CameraLog2Lin {
                kinv,
                minuskb,
                minusb,
                minv,
                log_side_break,
                linsinv,
                minuslino,
            } => {
                for p in pixels.iter_mut() {
                    for i in 0..3 {
                        let x = p[i];
                        p[i] = if x < log_side_break[i] {
                            linsinv[i] * (x + minuslino[i])
                        } else {
                            let v = ((x + minuskb[i]) * kinv[i]).exp2();
                            (v + minusb[i]) * minv[i]
                        };
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// Log op (port of OCIO's `LogOp`).
#[derive(Debug, Clone)]
pub struct LogOp {
    data: LogOpData,
    renderer: LogRenderer,
}

impl LogOp {
    /// Create the op (the parameters are validated).
    pub fn new(data: LogOpData) -> Result<Self> {
        data.validate()?;
        let renderer = LogRenderer::new(&data);
        Ok(Self { data, renderer })
    }

    /// The parameters.
    pub fn data(&self) -> &LogOpData {
        &self.data
    }

    /// The CPU renderer.
    pub fn renderer(&self) -> &LogRenderer {
        &self.renderer
    }
}

impl Op for LogOp {
    fn name(&self) -> &'static str {
        "Log"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        false
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<LogOp {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_LOG) {
            return None;
        }
        let other = next.downcast_ref::<LogOp>()?;
        if !self.data.is_inverse(&other.data) {
            return None;
        }
        match self.data.identity_replacement() {
            Some(range) => Some(vec![Arc::new(RangeOp::new(range).ok()?)]),
            None => Some(vec![]),
        }
    }

    fn to_transform(&self) -> Option<Transform> {
        let d = &self.data;
        let get = |i: usize| d.get_value(i).unwrap_or([0.0; 3]);
        if d.is_camera() {
            Some(Transform::LogCamera(LogCameraTransform {
                direction: d.direction,
                base: d.base,
                log_side_slope: get(LOG_SIDE_SLOPE),
                log_side_offset: get(LOG_SIDE_OFFSET),
                lin_side_slope: get(LIN_SIDE_SLOPE),
                lin_side_offset: get(LIN_SIDE_OFFSET),
                lin_side_break: get(LIN_SIDE_BREAK),
                linear_slope: d.get_value(LINEAR_SLOPE),
                metadata: d.metadata.clone(),
            }))
        } else if d.is_simple_log() {
            Some(Transform::Log(LogTransform {
                direction: d.direction,
                base: d.base,
                metadata: d.metadata.clone(),
            }))
        } else {
            Some(Transform::LogAffine(LogAffineTransform {
                direction: d.direction,
                base: d.base,
                log_side_slope: get(LOG_SIDE_SLOPE),
                log_side_offset: get(LOG_SIDE_OFFSET),
                lin_side_slope: get(LIN_SIDE_SLOPE),
                lin_side_offset: get(LIN_SIDE_OFFSET),
                metadata: d.metadata.clone(),
            }))
        }
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Op builders.

/// Append a log op built from `data` in direction `dir` (combined with the
/// data direction).
pub fn create_log_op_from_data(
    ops: &mut OpVec,
    data: &LogOpData,
    dir: TransformDirection,
) -> Result<()> {
    let log = match dir {
        TransformDirection::Forward => data.clone(),
        TransformDirection::Inverse => data.inverse()?,
    };
    ops.push(Arc::new(LogOp::new(log)?));
    Ok(())
}

/// Append a log affine op:
/// `logSlope * log(linSlope * x + linOffset, base) + logOffset`.
pub fn create_log_op(
    ops: &mut OpVec,
    base: f64,
    log_slope: &[f64; 3],
    log_offset: &[f64; 3],
    lin_slope: &[f64; 3],
    lin_offset: &[f64; 3],
    dir: TransformDirection,
) -> Result<()> {
    let data = LogOpData::from_affine(base, log_slope, log_offset, lin_slope, lin_offset, dir);
    ops.push(Arc::new(LogOp::new(data)?));
    Ok(())
}

/// Append a simple log op of the given base.
pub fn create_log_op_base(ops: &mut OpVec, base: f64, dir: TransformDirection) -> Result<()> {
    ops.push(Arc::new(LogOp::new(LogOpData::from_base(base, dir))?));
    Ok(())
}

// ---------------------------------------------------------------------------
// Transforms.

fn with_prefix<T>(r: Result<T>, prefix: &str) -> Result<T> {
    r.map_err(|e| e.prefixed(prefix))
}

impl LogTransform {
    /// Equality as defined by OCIO (metadata ignored).
    pub fn equals(&self, other: &LogTransform) -> bool {
        LogOpData::from_log_transform(self) == LogOpData::from_log_transform(other)
    }
}

impl Validate for LogTransform {
    fn validate(&self) -> Result<()> {
        with_prefix(
            LogOpData::from_log_transform(self).validate(),
            "LogTransform validation failed: ",
        )
    }
}

impl BuildOps for LogTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        create_log_op_from_data(ops, &LogOpData::from_log_transform(self), dir)
    }
}

impl LogAffineTransform {
    /// Equality as defined by OCIO (metadata ignored).
    pub fn equals(&self, other: &LogAffineTransform) -> bool {
        LogOpData::from_log_affine_transform(self) == LogOpData::from_log_affine_transform(other)
    }
}

impl Validate for LogAffineTransform {
    fn validate(&self) -> Result<()> {
        with_prefix(
            LogOpData::from_log_affine_transform(self).validate(),
            "LogAffineTransform validation failed: ",
        )
    }
}

impl BuildOps for LogAffineTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        create_log_op_from_data(ops, &LogOpData::from_log_affine_transform(self), dir)
    }
}

impl LogCameraTransform {
    /// Equality as defined by OCIO (metadata ignored).
    pub fn equals(&self, other: &LogCameraTransform) -> bool {
        LogOpData::from_log_camera_transform(self) == LogOpData::from_log_camera_transform(other)
    }
}

impl Validate for LogCameraTransform {
    fn validate(&self) -> Result<()> {
        let data = LogOpData::from_log_camera_transform(self);
        let check = || -> Result<()> {
            data.validate()?;
            if data.red.len() < 5 {
                crate::bail!("LinSideBreak has to be defined.");
            }
            Ok(())
        };
        with_prefix(check(), "LogCameraTransform validation failed: ")
    }
}

impl BuildOps for LogCameraTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        create_log_op_from_data(ops, &LogOpData::from_log_camera_transform(self), dir)
    }
}

#[cfg(test)]
mod tests;
