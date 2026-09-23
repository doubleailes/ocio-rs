//! ASC CDL op (port of `CDLOpData`, `CDLOp`, `CDLOpCPU` and the op building
//! part of `CDLTransform.cpp`).
//!
//! `out = sat(clamp(slope * in + offset) ^ power)` (forward, the clamps only
//! apply to the ASC v1.2 style). A CDL without power is replaced by matrices
//! (and clamping ranges) under `OptimizationFlags::SIMPLIFY_OPS`.

use crate::config::Config;
use crate::context::Context;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::math_utils::{clamp_f32, equal_with_abs_error};
use crate::ops::exponent::{config_major_version, create_exponent_op};
use crate::ops::matrix::float_format::format_g;
use crate::ops::matrix::{
    create_matrix_op_from_data, create_saturation_op, create_scale_offset_op, MatrixOpData,
};
use crate::ops::range::{create_range_op_from_data, RangeOpData};
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{
    BuildOps, CdlTransform, MatrixTransform, Transform, Validate, METADATA_SOP_DESCRIPTION,
};
use crate::types::{CdlStyle, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::sync::Arc;

const FLOAT_DECIMALS: usize = 7;

/// Luma weights used by the CDL saturation.
pub const CDL_LUMA_COEFS: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// Styles of the CDL op (port of `CDLOpData::Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CdlOpStyle {
    /// ASC v1.2 forward (clamps to `[0, 1]`).
    AscFwd,
    /// ASC v1.2 reverse (clamps to `[0, 1]`).
    AscRev,
    /// Forward without clamping.
    #[default]
    NoClampFwd,
    /// Reverse without clamping.
    NoClampRev,
}

impl CdlOpStyle {
    /// Parse a CTF / CLF style name (case insensitive; `v1.2_Fwd`, `Fwd`,
    /// `noClampFwd`, `FwdNoClamp`, ...).
    pub fn parse(name: &str) -> Result<Self> {
        let pairs = [
            ("v1.2_Fwd", CdlOpStyle::AscFwd),
            ("Fwd", CdlOpStyle::AscFwd),
            ("v1.2_Rev", CdlOpStyle::AscRev),
            ("Rev", CdlOpStyle::AscRev),
            ("noClampFwd", CdlOpStyle::NoClampFwd),
            ("FwdNoClamp", CdlOpStyle::NoClampFwd),
            ("noClampRev", CdlOpStyle::NoClampRev),
            ("RevNoClamp", CdlOpStyle::NoClampRev),
        ];
        for (n, s) in pairs {
            if !name.is_empty() && n.eq_ignore_ascii_case(name) {
                return Ok(s);
            }
        }
        crate::bail!("Unknown style for CDL.")
    }

    /// CLF name of the style.
    pub fn as_str(&self) -> &'static str {
        match self {
            CdlOpStyle::AscFwd => "Fwd",
            CdlOpStyle::AscRev => "Rev",
            CdlOpStyle::NoClampFwd => "FwdNoClamp",
            CdlOpStyle::NoClampRev => "RevNoClamp",
        }
    }

    /// Style from the transform style and a direction.
    pub fn from_transform_style(style: CdlStyle, dir: TransformDirection) -> Self {
        let fwd = dir == TransformDirection::Forward;
        match (style, fwd) {
            (CdlStyle::Asc, true) => CdlOpStyle::AscFwd,
            (CdlStyle::Asc, false) => CdlOpStyle::AscRev,
            (CdlStyle::NoClamp, true) => CdlOpStyle::NoClampFwd,
            (CdlStyle::NoClamp, false) => CdlOpStyle::NoClampRev,
        }
    }

    /// The transform style.
    pub fn transform_style(&self) -> CdlStyle {
        match self {
            CdlOpStyle::AscFwd | CdlOpStyle::AscRev => CdlStyle::Asc,
            CdlOpStyle::NoClampFwd | CdlOpStyle::NoClampRev => CdlStyle::NoClamp,
        }
    }

    /// Direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        match self {
            CdlOpStyle::AscFwd | CdlOpStyle::NoClampFwd => TransformDirection::Forward,
            CdlOpStyle::AscRev | CdlOpStyle::NoClampRev => TransformDirection::Inverse,
        }
    }

    /// The inverse style.
    pub fn inverse(&self) -> CdlOpStyle {
        match self {
            CdlOpStyle::AscFwd => CdlOpStyle::AscRev,
            CdlOpStyle::AscRev => CdlOpStyle::AscFwd,
            CdlOpStyle::NoClampFwd => CdlOpStyle::NoClampRev,
            CdlOpStyle::NoClampRev => CdlOpStyle::NoClampFwd,
        }
    }

    /// True for the reverse styles.
    pub fn is_reverse(&self) -> bool {
        self.direction() == TransformDirection::Inverse
    }

    /// True for the clamping (ASC) styles.
    pub fn is_clamping(&self) -> bool {
        matches!(self, CdlOpStyle::AscFwd | CdlOpStyle::AscRev)
    }
}

/// Channel parameters comparison (1e-9 absolute tolerance, as OCIO's
/// `ChannelParams::operator==`).
pub fn channel_params_equal(a: &[f64; 3], b: &[f64; 3]) -> bool {
    (0..3).all(|i| equal_with_abs_error(a[i], b[i], 1e-9))
}

/// Parameters of a [`CdlOp`] (port of OCIO's `CDLOpData`).
#[derive(Debug, Clone)]
pub struct CdlOpData {
    /// Style (encodes the direction).
    pub style: CdlOpStyle,
    /// RGB slopes.
    pub slope: [f64; 3],
    /// RGB offsets.
    pub offset: [f64; 3],
    /// RGB powers.
    pub power: [f64; 3],
    /// Saturation.
    pub saturation: f64,
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl Default for CdlOpData {
    /// Identity CDL of the default style (no clamp, forward).
    fn default() -> Self {
        Self {
            style: CdlOpStyle::NoClampFwd,
            slope: [1.0; 3],
            offset: [0.0; 3],
            power: [1.0; 3],
            saturation: 1.0,
            metadata: FormatMetadata::default(),
        }
    }
}

impl PartialEq for CdlOpData {
    /// Equality as defined by OCIO (tolerance on the SOP values, metadata
    /// ignored).
    fn eq(&self, other: &Self) -> bool {
        self.style == other.style
            && channel_params_equal(&self.slope, &other.slope)
            && channel_params_equal(&self.offset, &other.offset)
            && channel_params_equal(&self.power, &other.power)
            && self.saturation == other.saturation
    }
}

fn validate_greater_equal(name: &str, value: f64, threshold: f64) -> Result<()> {
    if !(value >= threshold) {
        crate::bail!(
            "CDL: Invalid '{}' {} should be greater than {}.",
            name,
            format_g(value, 6),
            format_g(threshold, 6)
        );
    }
    Ok(())
}

fn validate_greater_than(name: &str, value: f64, threshold: f64) -> Result<()> {
    if !(value > threshold) {
        crate::bail!(
            "CDLOpData: Invalid '{}' {} should be greater than {}.",
            name,
            format_g(value, 6),
            format_g(threshold, 6)
        );
    }
    Ok(())
}

fn params_string(p: &[f64; 3]) -> String {
    format!(
        "{}, {}, {}",
        format_g(p[0], FLOAT_DECIMALS),
        format_g(p[1], FLOAT_DECIMALS),
        format_g(p[2], FLOAT_DECIMALS)
    )
}

impl CdlOpData {
    /// Build from all the parameters and validate them.
    pub fn new(
        style: CdlOpStyle,
        slope: [f64; 3],
        offset: [f64; 3],
        power: [f64; 3],
        saturation: f64,
    ) -> Result<Self> {
        let d = Self {
            style,
            slope,
            offset,
            power,
            saturation,
            metadata: FormatMetadata::default(),
        };
        d.validate()?;
        Ok(d)
    }

    /// Build from a [`CdlTransform`] (not validated).
    pub fn from_transform(t: &CdlTransform) -> Self {
        Self {
            style: CdlOpStyle::from_transform_style(t.style, t.direction),
            slope: t.slope,
            offset: t.offset,
            power: t.power,
            saturation: t.sat,
            metadata: t.metadata.clone(),
        }
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Validate: slope >= 0, power > 0, saturation >= 0 (ASC v1.2 spec).
    pub fn validate(&self) -> Result<()> {
        for v in self.slope {
            validate_greater_equal("slope", v, 0.0)?;
        }
        for v in self.power {
            validate_greater_than("power", v, 0.0)?;
        }
        validate_greater_equal("saturation", self.saturation, 0.0)
    }

    /// Direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        self.style.direction()
    }

    /// Set the direction (inverts the style if needed).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.direction() != dir {
            self.style = self.style.inverse();
        }
    }

    /// True for the reverse styles.
    pub fn is_reverse(&self) -> bool {
        self.style.is_reverse()
    }

    /// True for the clamping (ASC) styles.
    pub fn is_clamping(&self) -> bool {
        self.style.is_clamping()
    }

    /// True if the SOP and saturation are the identity values (the op may
    /// still clamp).
    pub fn is_identity(&self) -> bool {
        channel_params_equal(&self.slope, &[1.0; 3])
            && channel_params_equal(&self.offset, &[0.0; 3])
            && channel_params_equal(&self.power, &[1.0; 3])
            && self.saturation == 1.0
    }

    /// An identity that does not clamp.
    pub fn is_no_op(&self) -> bool {
        self.is_identity() && !self.is_clamping()
    }

    /// The saturation mixes the channels.
    pub fn has_channel_crosstalk(&self) -> bool {
        self.saturation != 1.0
    }

    /// Copy with the inverse style.
    pub fn inverse(&self) -> CdlOpData {
        let mut c = self.clone();
        c.style = c.style.inverse();
        c
    }

    /// True if `r` is the inverse of `self`.
    pub fn is_inverse(&self, r: &CdlOpData) -> bool {
        *r == self.inverse()
    }

    /// The op replacing an identity (or a pair of inverse CDLs): a `[0, 1]`
    /// clamp for the ASC styles, `None` (an identity matrix) otherwise.
    pub fn identity_replacement(&self) -> Option<RangeOpData> {
        if self.is_clamping() {
            RangeOpData::new(0.0, 1.0, 0.0, 1.0).ok()
        } else {
            None
        }
    }

    /// Simpler ops replacing a CDL whose power is 1 (port of
    /// `getSimplerReplacement`): matrices for the slope / offset and the
    /// saturation, and ranges for the clamps. `None` if the power is used or
    /// for identities (see [`CdlOpData::identity_replacement`]).
    pub fn simpler_replacement(&self) -> Option<OpVec> {
        if !channel_params_equal(&self.power, &[1.0; 3]) || self.is_identity() {
            return None;
        }
        let dir = self.direction();
        let clamp = || RangeOpData::new(0.0, 1.0, 0.0, 1.0).ok();
        let mut ops = OpVec::new();

        // Slope + offset.
        let mut m44 = [0.0; 16];
        m44[0] = self.slope[0];
        m44[5] = self.slope[1];
        m44[10] = self.slope[2];
        m44[15] = 1.0;
        let offset4 = [self.offset[0], self.offset[1], self.offset[2], 0.0];
        create_matrix_op_from_data(
            &mut ops,
            &MatrixOpData::from_values(&m44, &offset4, dir),
            TransformDirection::Forward,
        )
        .ok()?;

        // Saturation.
        if self.saturation != 1.0 {
            if self.is_clamping() {
                // Same in both directions.
                create_range_op_from_data(&mut ops, &clamp()?, TransformDirection::Forward).ok()?;
            }
            let (m, o) = MatrixTransform::sat(self.saturation, &CDL_LUMA_COEFS);
            create_matrix_op_from_data(
                &mut ops,
                &MatrixOpData::from_values(&m, &o, dir),
                TransformDirection::Forward,
            )
            .ok()?;
        }

        // Clamping.
        if self.is_clamping() {
            create_range_op_from_data(&mut ops, &clamp()?, TransformDirection::Forward).ok()?;
        }

        if dir == TransformDirection::Inverse {
            ops.reverse();
        }
        Some(ops)
    }

    /// Slope as a string.
    pub fn slope_string(&self) -> String {
        params_string(&self.slope)
    }
    /// Offset as a string.
    pub fn offset_string(&self) -> String {
        params_string(&self.offset)
    }
    /// Power as a string.
    pub fn power_string(&self) -> String {
        params_string(&self.power)
    }
    /// Saturation as a string.
    pub fn saturation_string(&self) -> String {
        format_g(self.saturation, FLOAT_DECIMALS)
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        s.push_str(self.style.as_str());
        s.push(' ');
        s.push_str(&self.slope_string());
        s.push(' ');
        s.push_str(&self.offset_string());
        s.push(' ');
        s.push_str(&self.power_string());
        s.push(' ');
        s.push_str(&self.saturation_string());
        s.push(' ');
        s
    }
}

// ---------------------------------------------------------------------------
// CPU renderer.

const RCP_MIN_VALUE: f32 = 1e-2;

fn reciprocal(x: f32) -> f32 {
    1.0 / if x < RCP_MIN_VALUE { RCP_MIN_VALUE } else { x }
}

/// Render parameters of the CDL (port of `RenderParams`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdlRenderParams {
    /// Slopes (inverted for the reverse styles).
    pub slope: [f32; 3],
    /// Offsets (negated for the reverse styles).
    pub offset: [f32; 3],
    /// Powers (inverted for the reverse styles).
    pub power: [f32; 3],
    /// Saturation (inverted for the reverse styles).
    pub saturation: f32,
    /// Reverse style.
    pub is_reverse: bool,
    /// Non clamping style.
    pub is_no_clamp: bool,
}

impl CdlRenderParams {
    /// Compute the render parameters.
    pub fn new(cdl: &CdlOpData) -> Self {
        let sat = cdl.saturation as f32;
        let is_reverse = cdl.is_reverse();
        let is_no_clamp = !cdl.is_clamping();
        if is_reverse {
            Self {
                slope: cdl.slope.map(|v| reciprocal(v as f32)),
                offset: cdl.offset.map(|v| (-v) as f32),
                power: cdl.power.map(|v| reciprocal(v as f32)),
                saturation: reciprocal(sat),
                is_reverse,
                is_no_clamp,
            }
        } else {
            Self {
                slope: cdl.slope.map(|v| v as f32),
                offset: cdl.offset.map(|v| v as f32),
                power: cdl.power.map(|v| v as f32),
                saturation: sat,
                is_reverse,
                is_no_clamp,
            }
        }
    }
}

#[inline]
fn apply_slope(p: &mut Pixel, s: &[f32; 3]) {
    p[0] *= s[0];
    p[1] *= s[1];
    p[2] *= s[2];
}

#[inline]
fn apply_offset(p: &mut Pixel, o: &[f32; 3]) {
    p[0] += o[0];
    p[1] += o[1];
    p[2] += o[2];
}

#[inline]
fn apply_saturation(p: &mut Pixel, sat: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let src = [p[0], p[1], p[2]];
    let luma = src[0] * LUMA[0] + src[1] * LUMA[1] + src[2] * LUMA[2];
    for i in 0..3 {
        p[i] = luma + sat * (src[i] - luma);
    }
}

#[inline]
fn apply_clamp(p: &mut Pixel) {
    // NaNs become 0.
    for v in p.iter_mut().take(3) {
        *v = clamp_f32(*v, 0.0, 1.0);
    }
}

#[inline]
fn apply_power_clamp(p: &mut Pixel, power: &[f32; 3]) {
    apply_clamp(p);
    for i in 0..3 {
        p[i] = p[i].powf(power[i]);
    }
}

#[inline]
fn apply_power_no_clamp(p: &mut Pixel, power: &[f32; 3]) {
    // NaNs are set to 0, negative values are passed through.
    for i in 0..3 {
        let v = p[i];
        p[i] = if v.is_nan() {
            0.0
        } else if v < 0.0 {
            v
        } else {
            v.powf(power[i])
        };
    }
}

/// CPU renderer of the CDL op.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdlRenderer {
    params: CdlRenderParams,
}

impl CdlRenderer {
    /// Initialize the renderer.
    pub fn new(cdl: &CdlOpData) -> Self {
        Self {
            params: CdlRenderParams::new(cdl),
        }
    }

    /// The render parameters.
    pub fn params(&self) -> &CdlRenderParams {
        &self.params
    }

    /// Process pixels in place (alpha is not modified).
    pub fn apply(&self, pixels: &mut [Pixel]) {
        let p = &self.params;
        let clamp = !p.is_no_clamp;
        if !p.is_reverse {
            for px in pixels.iter_mut() {
                apply_slope(px, &p.slope);
                apply_offset(px, &p.offset);
                if clamp {
                    apply_power_clamp(px, &p.power);
                } else {
                    apply_power_no_clamp(px, &p.power);
                }
                apply_saturation(px, p.saturation);
                if clamp {
                    apply_clamp(px);
                }
            }
        } else {
            for px in pixels.iter_mut() {
                if clamp {
                    apply_clamp(px);
                }
                apply_saturation(px, p.saturation);
                if clamp {
                    apply_power_clamp(px, &p.power);
                } else {
                    apply_power_no_clamp(px, &p.power);
                }
                apply_offset(px, &p.offset);
                apply_slope(px, &p.slope);
                if clamp {
                    apply_clamp(px);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// CDL op (port of OCIO's `CDLOp`).
#[derive(Debug, Clone)]
pub struct CdlOp {
    data: CdlOpData,
    renderer: CdlRenderer,
}

impl CdlOp {
    /// Create the op (the parameters are validated).
    pub fn new(data: CdlOpData) -> Result<Self> {
        data.validate()?;
        let renderer = CdlRenderer::new(&data);
        Ok(Self { data, renderer })
    }

    /// The parameters.
    pub fn data(&self) -> &CdlOpData {
        &self.data
    }
}

impl Op for CdlOp {
    fn name(&self) -> &'static str {
        "CDL"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        self.data.is_no_op()
    }

    fn has_channel_crosstalk(&self) -> bool {
        self.data.has_channel_crosstalk()
    }

    fn cache_id(&self) -> String {
        format!("<CDLOp {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_CDL) {
            return None;
        }
        let other = next.downcast_ref::<CdlOp>()?;
        if !self.data.is_inverse(&other.data) {
            return None;
        }
        let mut ops = OpVec::new();
        if let Some(range) = self.data.identity_replacement() {
            create_range_op_from_data(&mut ops, &range, TransformDirection::Forward).ok()?;
        }
        Some(ops)
    }

    fn simplify(&self, flags: OptimizationFlags) -> Option<OpVec> {
        if flags.contains(OptimizationFlags::IDENTITY) && self.data.is_identity() {
            let mut ops = OpVec::new();
            if let Some(range) = self.data.identity_replacement() {
                create_range_op_from_data(&mut ops, &range, TransformDirection::Forward).ok()?;
            }
            return Some(ops);
        }
        if flags.contains(OptimizationFlags::SIMPLIFY_OPS) {
            return self.data.simpler_replacement();
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        let d = &self.data;
        Some(Transform::Cdl(CdlTransform {
            direction: d.direction(),
            style: d.style.transform_style(),
            slope: d.slope,
            offset: d.offset,
            power: d.power,
            sat: d.saturation,
            metadata: d.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Op builders.

/// Append a CDL op built from `data` in direction `dir` (combined with the
/// direction of the data style).
pub fn create_cdl_op_from_data(
    ops: &mut OpVec,
    data: &CdlOpData,
    dir: TransformDirection,
) -> Result<()> {
    let cdl = match dir {
        TransformDirection::Forward => data.clone(),
        TransformDirection::Inverse => data.inverse(),
    };
    ops.push(Arc::new(CdlOp::new(cdl)?));
    Ok(())
}

/// Append a CDL op (port of `CreateCDLOp`).
pub fn create_cdl_op(
    ops: &mut OpVec,
    style: CdlOpStyle,
    slope3: &[f64; 3],
    offset3: &[f64; 3],
    power3: &[f64; 3],
    saturation: f64,
    dir: TransformDirection,
) -> Result<()> {
    let data = CdlOpData::new(style, *slope3, *offset3, *power3, saturation)?;
    create_cdl_op_from_data(ops, &data, dir)
}

/// Build the ops of a `CdlTransform` for a config of the given major version
/// (port of `BuildCDLOp`): v1 configs use a scale / offset matrix, a
/// (clamping) exponent and a saturation matrix.
pub fn build_cdl_ops(
    ops: &mut OpVec,
    transform: &CdlTransform,
    dir: TransformDirection,
    config_major_version: u32,
) -> Result<()> {
    if config_major_version == 1 {
        let combined = dir.combine(transform.direction);
        let s = &transform.slope;
        let o = &transform.offset;
        let p = &transform.power;
        let slope4 = [s[0], s[1], s[2], 1.0];
        let offset4 = [o[0], o[1], o[2], 0.0];
        let power4 = [p[0], p[1], p[2], 1.0];
        let luma = transform.sat_luma_coefs();
        let sat = transform.sat;
        match combined {
            TransformDirection::Forward => {
                // 1) Scale + Offset.
                create_scale_offset_op(ops, &slope4, &offset4, TransformDirection::Forward)?;
                // 2) Power + Clamp at 0 (NB: This is not in accord with the
                //    ASC v1.2 spec since it also requires clamping at 1).
                create_exponent_op(ops, &power4, TransformDirection::Forward)?;
                // 3) Saturation (NB: Does not clamp at 0 and 1 as per ASC v1.2 spec).
                create_saturation_op(ops, sat, &luma, TransformDirection::Forward)
            }
            TransformDirection::Inverse => {
                create_saturation_op(ops, sat, &luma, TransformDirection::Inverse)?;
                create_exponent_op(ops, &power4, TransformDirection::Inverse)?;
                create_scale_offset_op(ops, &slope4, &offset4, TransformDirection::Inverse)
            }
        }
    } else {
        // Starting with the version 2, OCIO uses a CDL op complying with the
        // Common LUT Format (i.e. CLF) specification.
        transform.validate()?;
        create_cdl_op_from_data(ops, &CdlOpData::from_transform(transform), dir)
    }
}

// ---------------------------------------------------------------------------
// CdlTransform.

impl CdlTransform {
    /// Equality as defined by OCIO (1e-9 tolerance on the SOP values,
    /// metadata ignored).
    pub fn equals(&self, other: &CdlTransform) -> bool {
        CdlOpData::from_transform(self) == CdlOpData::from_transform(other)
    }

    /// Remove the first `SOPDescription` child element (OCIO's
    /// `setFirstSOPDescription(nullptr)`).
    pub fn clear_first_sop_description(&mut self) {
        if let Some(i) = self.metadata.first_child_index(METADATA_SOP_DESCRIPTION) {
            self.metadata.children.remove(i);
        }
    }
}

impl Validate for CdlTransform {
    fn validate(&self) -> Result<()> {
        CdlOpData::from_transform(self)
            .validate()
            .map_err(|e| e.prefixed("CDLTransform validation failed: "))
    }
}

impl BuildOps for CdlTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        build_cdl_ops(ops, self, dir, config_major_version(config))
    }
}

#[cfg(test)]
mod tests;
