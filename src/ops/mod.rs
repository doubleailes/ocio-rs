//! Ops: the internal, CPU-evaluable building blocks a processor is made of
//! (port of `Op.h` / `OpData` / the `*OpCPU` renderers).
//!
//! Each op module owns:
//! * a struct implementing [`Op`] (data + CPU evaluation),
//! * the `BuildOps` implementation of the transform(s) it serves, which
//!   converts a transform into ops (see `crate::transforms::build`).
//!
//! Pixels are always processed as interleaved RGBA `f32`.

pub mod allocation;
pub mod cdl;
pub mod exponent;
pub mod exposure_contrast;
pub mod fixed_function;
pub mod gamma;
pub mod grading_hue_curve;
pub mod grading_primary;
pub mod grading_rgb_curve;
pub mod grading_tone;
pub mod log;
pub mod lut1d;
pub mod lut3d;
pub mod matrix;
pub mod noop;
pub mod range;

use crate::dynamic_property::DynamicProperty;
use crate::transforms::Transform;
use crate::types::{DynamicPropertyType, OptimizationFlags};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

/// Shared pointer to an op.
pub type OpRc = Arc<dyn Op>;
/// Ordered list of ops.
pub type OpVec = Vec<OpRc>;

/// An RGBA pixel.
pub type Pixel = [f32; 4];

/// A color operation.
///
/// Implementations must be cheap to share (`Arc`) and thread safe: `apply`
/// may be called concurrently from several threads.
pub trait Op: Debug + Send + Sync + Any {
    /// Short type name, e.g. `"Matrix"`, `"Lut1D"`. Used in debugging and
    /// cache ids.
    fn name(&self) -> &'static str;

    /// Process pixels in place.
    fn apply(&self, pixels: &mut [Pixel]);

    /// True if the op does nothing at all (e.g. file/look markers, or an
    /// op whose parameters make it an exact no-op). No-ops are always
    /// removed by the optimizer.
    fn is_no_op(&self) -> bool {
        false
    }

    /// True if the op is an identity (may still clamp is **not** an
    /// identity). Removed by the optimizer under `OptimizationFlags::IDENTITY`.
    fn is_identity(&self) -> bool {
        self.is_no_op()
    }

    /// True if an output channel depends on other input channels.
    fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// Unique id describing the op parameters (used for processor caching
    /// and equality checks).
    fn cache_id(&self) -> String;

    /// `Any` access, used to downcast in `combine_with` / `is_inverse_of`.
    fn as_any(&self) -> &dyn Any;

    /// If `self` followed by `next` can be replaced by fewer ops, return the
    /// replacement:
    /// * `Some(vec![])` when the pair cancels out (an op followed by its
    ///   exact inverse, gated by the matching `PAIR_IDENTITY_*` flag),
    /// * `Some(vec![combined])` when both compose into one op (e.g. two
    ///   matrices, gated by `COMP_*` flags),
    /// * `None` otherwise (default).
    ///
    /// Implementations must check `flags` for the relevant bits and use
    /// `next.downcast_ref::<Self>()` to identify the other op.
    fn combine_with(&self, _next: &dyn Op, _flags: OptimizationFlags) -> Option<OpVec> {
        None
    }

    /// True if the op holds dynamic properties.
    fn is_dynamic(&self) -> bool {
        false
    }

    /// Return the dynamic property of the given type, if held.
    fn dynamic_property(&self, _ty: DynamicPropertyType) -> Option<DynamicProperty> {
        None
    }

    /// Replace the dynamic property of the op by `prop` (so several ops can
    /// share one property). Default: no-op.
    fn replace_dynamic_property(&mut self, _prop: &DynamicProperty) {}

    /// Copy of the op with dynamic properties converted to static values
    /// (used by `OptimizationFlags::NO_DYNAMIC_PROPERTIES`). Default: none.
    fn make_non_dynamic(&self) -> Option<OpRc> {
        None
    }

    /// Convert back to an equivalent transform (used by
    /// `Processor::create_group_transform` and CLF export). `None` if not
    /// representable (e.g. no-op markers).
    fn to_transform(&self) -> Option<Transform> {
        None
    }

    /// Clone into a new boxed op (so it can be mutated before sharing).
    fn clone_box(&self) -> Box<dyn Op>;
}

impl dyn Op {
    /// Downcast helper.
    pub fn downcast_ref<T: Op>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
}

/// Combined cache id of an op list.
pub fn ops_cache_id(ops: &[OpRc]) -> String {
    let mut s = String::new();
    for op in ops {
        s.push_str(&op.cache_id());
        s.push(' ');
    }
    s
}

/// True if every op is a no-op.
pub fn ops_are_no_op(ops: &[OpRc]) -> bool {
    ops.iter().all(|o| o.is_no_op())
}

/// Evaluate an op list on pixels.
pub fn apply_ops(ops: &[OpRc], pixels: &mut [Pixel]) {
    for op in ops {
        op.apply(pixels);
    }
}

/// Helper for cache ids: hash of float data.
pub fn hash_f32(values: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for v in values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    format!("{:x}", md5::compute(&bytes))
}

/// Helper for cache ids: hash of double data.
pub fn hash_f64(values: &[f64]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    for v in values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    format!("{:x}", md5::compute(&bytes))
}
