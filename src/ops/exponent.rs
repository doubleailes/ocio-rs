//! Legacy exponent op (port of `ExponentOp.cpp`) and the op building of
//! `ExponentTransform`.
//!
//! In OCIO v2 configs an `ExponentTransform` builds a gamma op (see
//! `crate::ops::gamma`) honoring the negative style. OCIO v1 configs (and the
//! v1 CDL implementation) use this simpler op: `out = pow(max(0, in), exp)`
//! on the four channels.

use crate::config::Config;
use crate::context::Context;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::math_utils::{is_scalar_equal_to_zero, is_vec_equal_to_one};
use crate::ops::gamma::{create_gamma_op, GammaOpData};
use crate::ops::matrix::float_format::format_g;
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{BuildOps, ExponentTransform, Transform, Validate};
use crate::types::{NegativeStyle, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::sync::Arc;

/// Parameters of an [`ExponentOp`] (port of OCIO's `ExponentOpData`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExponentOpData {
    /// RGBA exponents.
    pub exp4: [f64; 4],
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl Default for ExponentOpData {
    fn default() -> Self {
        Self {
            exp4: [1.0; 4],
            metadata: FormatMetadata::default(),
        }
    }
}

impl ExponentOpData {
    /// Build from the RGBA exponents.
    pub fn new(exp4: [f64; 4]) -> Self {
        Self {
            exp4,
            metadata: FormatMetadata::default(),
        }
    }

    /// True if all the exponents are 1.
    pub fn is_identity(&self) -> bool {
        is_vec_equal_to_one(&self.exp4)
    }

    /// Same as [`ExponentOpData::is_identity`] (the clamp is ignored).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        for v in &self.exp4 {
            s.push_str(&format_g(*v, 7));
            s.push(' ');
        }
        s
    }
}

/// Legacy exponent op (port of OCIO's `ExponentOp`).
#[derive(Debug, Clone)]
pub struct ExponentOp {
    data: ExponentOpData,
    exp: [f32; 4],
}

impl ExponentOp {
    /// Create the op.
    pub fn new(data: ExponentOpData) -> Self {
        let e = &data.exp4;
        let exp = [e[0] as f32, e[1] as f32, e[2] as f32, e[3] as f32];
        Self { data, exp }
    }

    /// The parameters.
    pub fn data(&self) -> &ExponentOpData {
        &self.data
    }
}

impl Op for ExponentOp {
    fn name(&self) -> &'static str {
        "Exponent"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for p in pixels.iter_mut() {
            for c in 0..4 {
                // std::max(0.0f, v): NaN becomes 0.
                let v = if 0.0 < p[c] { p[c] } else { 0.0 };
                p[c] = v.powf(self.exp[c]);
            }
        }
    }

    fn is_no_op(&self) -> bool {
        self.data.is_no_op()
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<ExponentOp {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::COMP_EXPONENT) {
            return None;
        }
        let other = next.downcast_ref::<ExponentOp>()?;
        let a = &self.data.exp4;
        let b = &other.data.exp4;
        let combined = [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]];
        if is_vec_equal_to_one(&combined) {
            return Some(vec![]);
        }
        let mut data = ExponentOpData::new(combined);
        data.metadata = self.data.metadata.clone();
        data.metadata.combine(&other.data.metadata);
        Some(vec![Arc::new(ExponentOp::new(data))])
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::Exponent(ExponentTransform {
            direction: TransformDirection::Forward,
            value: self.data.exp4,
            negative_style: NegativeStyle::Clamp,
            metadata: self.data.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

/// Append a legacy exponent op built from `data` (the exponents are inverted
/// in the inverse direction; a 0 exponent can't be inverted).
pub fn create_exponent_op_from_data(
    ops: &mut OpVec,
    data: &ExponentOpData,
    dir: TransformDirection,
) -> Result<()> {
    match dir {
        TransformDirection::Forward => ops.push(Arc::new(ExponentOp::new(data.clone()))),
        TransformDirection::Inverse => {
            let mut values = [0.0; 4];
            for i in 0..4 {
                if is_scalar_equal_to_zero(data.exp4[i]) {
                    crate::bail!(
                        "Cannot apply ExponentOp op, Cannot apply 0.0 exponent in the inverse."
                    );
                }
                values[i] = 1.0 / data.exp4[i];
            }
            // Note: as in OCIO, the metadata is not kept by the inverse.
            ops.push(Arc::new(ExponentOp::new(ExponentOpData::new(values))));
        }
    }
    Ok(())
}

/// Append a legacy exponent op (port of `CreateExponentOp`).
pub fn create_exponent_op(ops: &mut OpVec, exp4: &[f64; 4], dir: TransformDirection) -> Result<()> {
    create_exponent_op_from_data(ops, &ExponentOpData::new(*exp4), dir)
}

/// Major version of the config used to build the ops (v1 configs build
/// `ExponentTransform` and `CdlTransform` with the legacy ops).
pub(crate) fn config_major_version(config: &Config) -> u32 {
    config.major_version()
}

/// Build the ops of an `ExponentTransform` for a config of the given major
/// version (port of `BuildExponentOp`).
pub fn build_exponent_ops(
    ops: &mut OpVec,
    transform: &ExponentTransform,
    dir: TransformDirection,
    config_major_version: u32,
) -> Result<()> {
    if config_major_version == 1 {
        // Ignore the style, use a simple exponent.
        let combined = dir.combine(transform.direction);
        let mut data = ExponentOpData::new(transform.value);
        data.metadata = transform.metadata.clone();
        create_exponent_op_from_data(ops, &data, combined)
    } else {
        transform.validate()?;
        let data = GammaOpData::from_exponent_transform(transform)?;
        create_gamma_op(ops, &data, dir)
    }
}

impl ExponentTransform {
    /// Equality as defined by OCIO (metadata ignored).
    pub fn equals(&self, other: &ExponentTransform) -> bool {
        match (
            GammaOpData::from_exponent_transform(self),
            GammaOpData::from_exponent_transform(other),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => self == other,
        }
    }
}

impl Validate for ExponentTransform {
    fn validate(&self) -> Result<()> {
        GammaOpData::from_exponent_transform(self)
            .and_then(|d| d.validate())
            .map_err(|e| e.prefixed("ExponentTransform validation failed: "))
    }
}

impl BuildOps for ExponentTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        build_exponent_ops(ops, self, dir, config_major_version(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::gamma::{GammaOp, GammaStyle};
    use crate::ops::matrix::test_utils::*;
    use crate::ops::noop::create_file_no_op;
    use crate::processor::optimize_ops;
    use crate::types::{METADATA_DESCRIPTION, METADATA_ID, METADATA_NAME};

    fn check(op: &dyn Op, src: &[f32], expected: &[f32], err: f64) {
        let out = apply_op(op, src);
        for i in 0..4 {
            assert_close_f(out[i], expected[i] as f64, err);
        }
    }

    // ExponentOp_tests.cpp

    #[test]
    fn op_value() {
        let exp1 = [1.2, 1.3, 1.4, 1.5];
        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, &exp1, TransformDirection::Forward).unwrap();
        create_exponent_op(&mut ops, &exp1, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 2);
        let source = [0.1f32, 0.3, 0.9, 0.5];
        let result1 = [0.0630957261f32, 0.209053621, 0.862858355, 0.353553385];
        let tmp = apply_op(ops[0].as_ref(), &source);
        for i in 0..4 {
            assert_close_f(tmp[i], result1[i] as f64, 1e-6);
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..4 {
            assert_close_f(tmp[i], source[i] as f64, 1e-6);
        }
    }

    #[test]
    fn op_value_limits() {
        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, &[0., 2., -2., 1.5], TransformDirection::Forward).unwrap();
        let op = ops[0].as_ref();
        check(op, &[1.0; 4], &[1.0; 4], 1e-6);
        check(op, &[2.0; 4], &[1.0, 4.0, 0.25, 2.82842708], 1e-6);
        check(op, &[-2.0, -2.0, 1.0, -2.0], &[1.0, 0.0, 1.0, 0.0], 1e-6);
        check(op, &[0.0, 0.0, 1.0, 0.0], &[1.0, 0.0, 1.0, 0.0], 1e-6);
    }

    #[test]
    fn op_combining() {
        let flags = OptimizationFlags::DEFAULT;
        {
            let mut d1 = ExponentOpData::new([2.0, 2.0, 2.0, 1.0]);
            let mut d2 = ExponentOpData::new([1.2, 1.2, 1.2, 1.0]);
            d1.metadata.set_name("Exp1");
            d1.metadata.set_id("ID1");
            d1.metadata
                .add_child_element(METADATA_DESCRIPTION, "First exponent");
            d2.metadata.set_name("Exp2");
            d2.metadata.set_id("ID2");
            d2.metadata
                .add_child_element(METADATA_DESCRIPTION, "Second exponent");
            d2.metadata.add_attribute("Attrib", "value");
            let mut ops = OpVec::new();
            create_exponent_op_from_data(&mut ops, &d1, TransformDirection::Forward).unwrap();
            create_exponent_op_from_data(&mut ops, &d2, TransformDirection::Forward).unwrap();
            let source = [0.9f32, 0.4, 0.1, 0.5];
            let result = [0.776572466f32, 0.110903174, 0.00398107106, 0.5];
            let tmp = apply_ops(&ops, &source);
            for i in 0..4 {
                assert_close_f(tmp[i], result[i] as f64, 1e-6);
            }
            let combined = ops[0].combine_with(ops[1].as_ref(), flags).unwrap();
            assert_eq!(combined.len(), 1);
            let cd = combined[0].downcast_ref::<ExponentOp>().unwrap().data();
            assert_eq!(cd.metadata.attribute_value(METADATA_NAME), "Exp1 + Exp2");
            assert_eq!(cd.metadata.attribute_value(METADATA_ID), "ID1 + ID2");
            assert_eq!(cd.metadata.children.len(), 2);
            assert_eq!(cd.metadata.children[0].element_name, METADATA_DESCRIPTION);
            assert_eq!(cd.metadata.children[0].element_value, "First exponent");
            assert_eq!(cd.metadata.children[1].element_value, "Second exponent");
            assert_eq!(cd.metadata.attributes.len(), 3);
            assert_eq!(
                cd.metadata.attributes[2],
                ("Attrib".to_string(), "value".to_string())
            );
            let tmp2 = apply_ops(&combined, &source);
            for i in 0..4 {
                assert_close_f(tmp2[i], result[i] as f64, 1e-6);
            }
        }
        {
            let exp1 = [1.037289, 1.019015, 0.966082, 1.0];
            let mut ops = OpVec::new();
            create_exponent_op(&mut ops, &exp1, TransformDirection::Forward).unwrap();
            create_exponent_op(&mut ops, &exp1, TransformDirection::Inverse).unwrap();
            let combined = ops[0].combine_with(ops[1].as_ref(), flags).unwrap();
            assert!(combined.is_empty());
        }
        {
            let exp1 = [1.037289, 1.019015, 0.966082, 1.0];
            let mut ops = OpVec::new();
            for _ in 0..3 {
                create_exponent_op(&mut ops, &exp1, TransformDirection::Forward).unwrap();
            }
            let source = [0.1f32, 0.5, 0.9, 0.5];
            let result = [0.0765437484f32, 0.480251998, 0.909373641, 0.5];
            let tmp = apply_ops(&ops, &source);
            for i in 0..4 {
                assert_close_f(tmp[i], result[i] as f64, 1e-6);
            }
            let opt = optimize_ops(&ops, flags);
            assert_eq!(opt.len(), 1);
            let tmp = apply_ops(&opt, &source);
            for i in 0..4 {
                assert_close_f(tmp[i], result[i] as f64, 1e-6);
            }
            // Not combined without the flag.
            assert_eq!(
                optimize_ops(&ops, flags & !OptimizationFlags::COMP_EXPONENT).len(),
                3
            );
        }
    }

    #[test]
    fn op_throw_create() {
        let mut ops = OpVec::new();
        let e = create_exponent_op(&mut ops, &[0.0, 1.3, 1.4, 1.5], TransformDirection::Inverse)
            .unwrap_err();
        assert!(e
            .message()
            .contains("Cannot apply 0.0 exponent in the inverse"));
    }

    #[test]
    fn op_can_combine_with() {
        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, &[0.0, 1.3, 1.4, 1.5], TransformDirection::Forward).unwrap();
        create_file_no_op(&mut ops, "NoOp");
        assert!(ops[0]
            .combine_with(ops[1].as_ref(), OptimizationFlags::ALL)
            .is_none());
    }

    #[test]
    fn op_noop() {
        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, &[1.0; 4], TransformDirection::Forward).unwrap();
        create_exponent_op(&mut ops, &[1.0; 4], TransformDirection::Inverse).unwrap();
        assert!(ops[0].is_no_op());
        assert!(ops[1].is_no_op());
        assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    }

    #[test]
    fn op_cache_id() {
        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, &[2.0, 2.1, 3.0, 3.1], TransformDirection::Forward).unwrap();
        create_exponent_op(&mut ops, &[4.0, 4.1, 5.0, 5.1], TransformDirection::Forward).unwrap();
        create_exponent_op(&mut ops, &[2.0, 2.1, 3.0, 3.1], TransformDirection::Forward).unwrap();
        assert_eq!(ops[0].cache_id(), ops[2].cache_id());
        assert_ne!(ops[0].cache_id(), ops[1].cache_id());
        assert_eq!(ops[0].cache_id(), "<ExponentOp 2 2.1 3 3.1 >");
    }

    #[test]
    fn op_create_transform() {
        let exp = [2.0, 2.1, 3.0, 3.1];
        let op = ExponentOp::new(ExponentOpData::new(exp));
        match op.to_transform().unwrap() {
            Transform::Exponent(t) => {
                assert_eq!(t.direction, TransformDirection::Forward);
                assert_eq!(t.value, exp);
            }
            _ => panic!("expected an exponent transform"),
        }
    }

    // ExponentTransform_tests.cpp

    #[test]
    fn transform_basic() {
        let mut exp = ExponentTransform::default();
        assert_eq!(exp.direction, TransformDirection::Forward);
        assert_eq!(exp.value, [1.0; 4]);
        assert_eq!(exp.negative_style, NegativeStyle::Clamp);
        exp.direction = TransformDirection::Inverse;
        exp.value[1] = 2.1234567;
        assert!(exp.validate().is_ok());
        let exp2 = exp.clone();
        assert!(exp.equals(&exp2));
        exp.negative_style = NegativeStyle::Linear;
        assert_eq!(
            exp.validate().unwrap_err().message(),
            "ExponentTransform validation failed: Linear negative extrapolation is not valid for basic exponent style."
        );
        exp.negative_style = NegativeStyle::Clamp;
        exp.value[0] = 0.0;
        assert_eq!(
            exp.validate().unwrap_err().message(),
            "ExponentTransform validation failed: Parameter 0 is less than lower bound 0.01"
        );
    }

    #[test]
    fn transform_build_ops() {
        let mut exp = ExponentTransform::default();
        let id = "sample exponent";
        exp.metadata.add_attribute(METADATA_ID, id);

        // With v1 config, exponent transform is converted to ExponentOp that does not handle
        // negative styles.
        for neg in [NegativeStyle::Clamp, NegativeStyle::Mirror] {
            exp.negative_style = neg;
            let mut ops = OpVec::new();
            build_exponent_ops(&mut ops, &exp, TransformDirection::Forward, 1).unwrap();
            assert_eq!(ops.len(), 1);
            let d = ops[0].downcast_ref::<ExponentOp>().unwrap().data();
            // In v1 identity exponent is considered a no-op (losing the clamp).
            assert!(d.is_no_op());
            assert_eq!(d.id(), id);
        }

        // With v2 config, exponent transform is converted to GammaOp that handles negative styles.
        let cases = [
            (NegativeStyle::Clamp, GammaStyle::BasicFwd, false),
            (NegativeStyle::Mirror, GammaStyle::BasicMirrorFwd, true),
            (NegativeStyle::PassThru, GammaStyle::BasicPassThruFwd, true),
        ];
        for (neg, style, no_op) in cases {
            exp.negative_style = neg;
            let mut ops = OpVec::new();
            build_exponent_ops(&mut ops, &exp, TransformDirection::Forward, 2).unwrap();
            assert_eq!(ops.len(), 1);
            let d = ops[0].downcast_ref::<GammaOp>().unwrap().data();
            assert_eq!(d.style, style);
            assert!(d.is_identity());
            assert_eq!(d.is_no_op(), no_op);
            assert_eq!(d.id(), id);
        }

        exp.negative_style = NegativeStyle::Linear;
        let mut ops = OpVec::new();
        let e = build_exponent_ops(&mut ops, &exp, TransformDirection::Forward, 2).unwrap_err();
        assert!(e
            .message()
            .contains("Linear negative extrapolation is not valid for basic exponent style"));

        // The transform build uses the config (v2 behavior).
        exp.negative_style = NegativeStyle::Mirror;
        exp.value = [2.2, 2.2, 2.2, 1.0];
        let mut ops = OpVec::new();
        exp.build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            TransformDirection::Inverse,
        )
        .unwrap();
        let d = ops[0].downcast_ref::<GammaOp>().unwrap().data();
        assert_eq!(d.style, GammaStyle::BasicMirrorRev);
        let out = apply_ops(&ops, &[-0.25, 0.25, 1.0, 0.5]);
        assert_close_f(out[0], -(0.25f32.powf((1.0 / 2.2) as f32)) as f64, 1e-7);
        assert_close_f(out[3], 0.5, 1e-7);
    }
}
