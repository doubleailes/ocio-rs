//! Allocation ops (port of `AllocationOp.cpp` and the op building part of
//! `AllocationTransform.cpp`).
//!
//! * uniform allocation: a fit of `[min, max]` (default `[0, 1]`) to `[0, 1]`,
//! * lg2 allocation: a base-2 log (with an optional linear offset) followed by
//!   a fit of `[min, max]` (default `[-10, 6]`) to `[0, 1]`.

use crate::config::Config;
use crate::context::Context;
use crate::error::Result;
use crate::ops::log::create_log_op;
use crate::ops::matrix::create_fit_op;
use crate::ops::matrix::float_format::format_g;
use crate::ops::OpVec;
use crate::transforms::{AllocationTransform, BuildOps, Validate};
use crate::types::{Allocation, TransformDirection};

/// Allocation parameters (port of OCIO's `AllocationData`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllocationData {
    /// The allocation.
    pub allocation: Allocation,
    /// The variables: min, max and (lg2 only) the linear offset.
    pub vars: Vec<f32>,
}

impl AllocationData {
    /// Build from an allocation transform (the variables are stored in single
    /// precision, as in OCIO).
    pub fn from_transform(t: &AllocationTransform) -> Self {
        Self {
            allocation: t.allocation,
            vars: t.vars.iter().map(|&v| v as f32).collect(),
        }
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = format!("{} ", self.allocation.as_str());
        for v in &self.vars {
            s.push_str(&format_g(*v as f64, 7));
            s.push(' ');
        }
        s
    }
}

/// Append the ops implementing an allocation (port of `CreateAllocationOps`).
pub fn create_allocation_ops(
    ops: &mut OpVec,
    data: &AllocationData,
    dir: TransformDirection,
) -> Result<()> {
    match data.allocation {
        Allocation::Uniform => {
            let mut oldmin = [0.0; 4];
            let mut oldmax = [1.0; 4];
            let newmin = [0.0; 4];
            let newmax = [1.0; 4];
            if data.vars.len() >= 2 {
                for i in 0..3 {
                    oldmin[i] = data.vars[0] as f64;
                    oldmax[i] = data.vars[1] as f64;
                }
            }
            create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)
        }
        Allocation::Lg2 => {
            let mut oldmin = [-10.0, -10.0, -10.0, 0.0];
            let mut oldmax = [6.0, 6.0, 6.0, 1.0];
            let newmin = [0.0; 4];
            let newmax = [1.0; 4];
            if data.vars.len() >= 2 {
                for i in 0..3 {
                    oldmin[i] = data.vars[0] as f64;
                    oldmax[i] = data.vars[1] as f64;
                }
            }

            // output = logSlope * log(linSlope * input + linOffset, base) + logOffset
            let base = 2.0;
            let log_slope = [1.0; 3];
            let lin_slope = [1.0; 3];
            let mut lin_offset = [0.0; 3];
            let log_offset = [0.0; 3];
            if data.vars.len() >= 3 {
                lin_offset = [data.vars[2] as f64; 3];
            }

            match dir {
                TransformDirection::Forward => {
                    create_log_op(
                        ops,
                        base,
                        &log_slope,
                        &log_offset,
                        &lin_slope,
                        &lin_offset,
                        dir,
                    )?;
                    create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)
                }
                TransformDirection::Inverse => {
                    create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)?;
                    create_log_op(
                        ops,
                        base,
                        &log_slope,
                        &log_offset,
                        &lin_slope,
                        &lin_offset,
                        dir,
                    )
                }
            }
        }
        Allocation::Unknown => crate::bail!("Unsupported Allocation Type."),
    }
}

impl Validate for AllocationTransform {
    fn validate(&self) -> Result<()> {
        let n = self.vars.len();
        match self.allocation {
            Allocation::Uniform => {
                if n != 2 && n != 0 {
                    crate::bail!(
                        "AllocationTransform: wrong number of values for the uniform allocation"
                    );
                }
            }
            Allocation::Lg2 => {
                if n != 3 && n != 2 && n != 0 {
                    crate::bail!("AllocationTransform: wrong number of values for the logarithmic allocation");
                }
            }
            Allocation::Unknown => crate::bail!("AllocationTransform: invalid allocation type"),
        }
        Ok(())
    }
}

impl BuildOps for AllocationTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        let combined = dir.combine(self.direction);
        create_allocation_ops(ops, &AllocationData::from_transform(self), combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::log::LogOp;
    use crate::ops::matrix::test_utils::*;
    use crate::ops::matrix::MatrixOp;
    use crate::processor::optimize_ops;
    use crate::types::OptimizationFlags;

    fn is_log(op: &crate::ops::OpRc) -> bool {
        op.downcast_ref::<LogOp>().is_some()
    }

    // AllocationOp_tests.cpp

    #[test]
    fn allocation_ops_create() {
        let mut ops = OpVec::new();
        let mut data = AllocationData {
            allocation: Allocation::Unknown,
            vars: vec![],
        };
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let e = create_allocation_ops(&mut ops, &data, dir).unwrap_err();
            assert!(e.message().contains("Unsupported Allocation Type"));
            assert!(ops.is_empty());
        }

        data.allocation = Allocation::Uniform;
        // No allocation data leads to identity, an identity op is created.
        create_allocation_ops(&mut ops, &data, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].is_no_op());
        ops.clear();
        create_allocation_ops(&mut ops, &data, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 1);
        ops.clear();

        // Adding data to avoid identity: a fit is created.
        data.vars = vec![0.0, 10.0];
        create_allocation_ops(&mut ops, &data, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].downcast_ref::<MatrixOp>().is_some());
        ops.clear();
        create_allocation_ops(&mut ops, &data, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 1);
        ops.clear();

        data.allocation = Allocation::Lg2;
        // Default is not identity.
        data.vars.clear();
        create_allocation_ops(&mut ops, &data, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops[1].downcast_ref::<MatrixOp>().is_some());
        let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
        assert_eq!(ops.len(), 2);
        let default_log = ops[0].clone();
        let default_fit = ops[1].clone();

        let src = [
            0.16f32, 0.2, 0.3, 0.4, -0.16, -0.2, 32.0, 123.4, 1.0, 1.0, 1.0, 1.0,
        ];
        let dst_log = [
            -2.64385629f32,
            -2.32192802,
            -1.73696554,
            0.4,
            -126.0,
            -126.0,
            5.0,
            123.4,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let dst_fit = [
            0.635f32, 0.6375, 0.64375, 0.4, 0.615, 0.6125, 2.625, 123.4, 0.6875, 0.6875, 0.6875,
            1.0,
        ];
        let tmp = apply_op(default_log.as_ref(), &src);
        for i in 0..12 {
            assert_close_f(tmp[i], dst_log[i] as f64, 1e-6);
        }
        let tmp = apply_op(default_fit.as_ref(), &src);
        for i in 0..12 {
            assert_close_f(tmp[i], dst_fit[i] as f64, 1e-6);
        }

        let mut ops = OpVec::new();
        create_allocation_ops(&mut ops, &data, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 2);
        let l0 = default_log.downcast_ref::<LogOp>().unwrap().data();
        let l1 = ops[1].downcast_ref::<LogOp>().unwrap().data();
        assert!(l0.is_inverse(l1));

        // Adding data to target identity: log op and identity are created.
        data.vars = vec![0.0, 1.0];
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let mut ops = OpVec::new();
            create_allocation_ops(&mut ops, &data, dir).unwrap();
            assert_eq!(ops.len(), 2);
            // Identity is removed.
            let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
            assert_eq!(ops.len(), 1);
            assert!(is_log(&ops[0]));
        }

        // Change log intercept.
        data.vars.push(10.0);
        let mut ops = OpVec::new();
        create_allocation_ops(&mut ops, &data, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 2);
        let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
        assert_eq!(ops.len(), 1);
        let dst_log_shift = [
            3.34482837f32,
            3.35049725,
            3.36457253,
            0.4,
            3.29865813,
            3.29278183,
            5.39231730,
            123.4,
            3.45943165,
            3.45943165,
            3.45943165,
            1.0,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert_close_f(tmp[i], dst_log_shift[i] as f64, 1e-6);
        }
        assert_eq!(data.cache_id(), "lg2 0 1 10 ");
    }

    // AllocationTransform_tests.cpp

    #[test]
    fn allocation_transform_validate() {
        let mut al = AllocationTransform {
            allocation: Allocation::Uniform,
            ..Default::default()
        };
        assert!(al.validate().is_ok());
        al.vars = vec![0.0, 0.0];
        assert!(al.validate().is_ok());
        al.vars.push(0.01);
        assert!(al.validate().is_err());
        al.allocation = Allocation::Lg2;
        assert!(al.validate().is_ok());
        al.vars.push(0.1);
        assert!(al.validate().is_err());
        al.vars.clear();
        assert!(al.validate().is_ok());
        al.allocation = Allocation::Unknown;
        assert_eq!(
            al.validate().unwrap_err().message(),
            "AllocationTransform: invalid allocation type"
        );
    }

    #[test]
    fn allocation_transform_build_ops() {
        let config = Config::create_raw();
        let ctx = Context::new();
        let al = AllocationTransform {
            allocation: Allocation::Lg2,
            vars: vec![-8.0, 5.0, 0.00390625],
            ..Default::default()
        };
        let mut ops = OpVec::new();
        al.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        al.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
            .unwrap();
        assert_eq!(ops.len(), 4);
        assert!(is_log(&ops[0]));
        assert!(is_log(&ops[3]));
        // Round trip.
        let src = [0.18f32, 1.0, 4.0, 0.5];
        let out = apply_ops(&ops, &src);
        for i in 0..4 {
            assert_close_f(out[i], src[i] as f64, 1e-5);
        }
        // The optimizer removes the fit pair and the log pair (replaced by a clamp range).
        let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
        assert_eq!(opt.len(), 1);
        assert_eq!(opt[0].name(), "Range");

        let mut inv = al.clone();
        inv.direction = TransformDirection::Inverse;
        let mut ops = OpVec::new();
        inv.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        assert!(ops[0].downcast_ref::<MatrixOp>().is_some());
        assert!(is_log(&ops[1]));
    }
}
