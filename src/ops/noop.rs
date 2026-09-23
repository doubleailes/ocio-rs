//! Marker no-ops used to track the files and looks involved in a processor
//! (port of `NoOps.cpp`). They are removed by the optimizer.

use super::{Op, OpVec, Pixel};
use std::any::Any;
use std::sync::Arc;

/// What a marker records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerKind {
    /// A file used by a `FileTransform`.
    File,
    /// A look applied by a `LookTransform`.
    Look,
}

/// A no-op carrying metadata for `ProcessorMetadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerNoOp {
    pub kind: MarkerKind,
    pub value: String,
}

impl Op for MarkerNoOp {
    fn name(&self) -> &'static str {
        match self.kind {
            MarkerKind::File => "FileNoOp",
            MarkerKind::Look => "LookNoOp",
        }
    }
    fn apply(&self, _pixels: &mut [Pixel]) {}
    fn is_no_op(&self) -> bool {
        true
    }
    fn has_channel_crosstalk(&self) -> bool {
        false
    }
    fn cache_id(&self) -> String {
        String::new()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

/// Append a file marker.
pub fn create_file_no_op(ops: &mut OpVec, fname: &str) {
    ops.push(Arc::new(MarkerNoOp { kind: MarkerKind::File, value: fname.to_string() }));
}

/// Append a look marker.
pub fn create_look_no_op(ops: &mut OpVec, look: &str) {
    ops.push(Arc::new(MarkerNoOp { kind: MarkerKind::Look, value: look.to_string() }));
}
