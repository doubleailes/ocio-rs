//! The builtin transform registry (port of `BuiltinTransformRegistry.cpp`).

use super::op_helpers::{add_identity_matrix, TransformVec};
use crate::error::{Error, Result};
use std::fmt;
use std::sync::{Arc, OnceLock};

/// Function appending the transforms implementing a builtin (in the forward
/// direction) to a list.
pub type BuiltinCreator = Arc<dyn Fn(&mut TransformVec) -> Result<()> + Send + Sync>;

#[derive(Clone)]
struct BuiltinData {
    style: String,
    description: String,
    creator: BuiltinCreator,
}

/// A registry of builtin transforms: each builtin has a unique style name
/// (case insensitive), a description and a creator building its transforms.
#[derive(Clone, Default)]
pub struct BuiltinTransformRegistry {
    builtins: Vec<BuiltinData>,
}

impl fmt::Debug for BuiltinTransformRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.builtins.iter().map(|b| &b.style))
            .finish()
    }
}

fn invalid_index() -> Error {
    Error::msg("Invalid index.")
}

impl BuiltinTransformRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The global registry holding all the builtins.
    pub fn get() -> &'static BuiltinTransformRegistry {
        static REGISTRY: OnceLock<BuiltinTransformRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let mut r = BuiltinTransformRegistry::new();
            r.register_all();
            r
        })
    }

    /// Add a builtin; an existing builtin with the same (case insensitive)
    /// style is replaced.
    pub fn add_builtin<F>(&mut self, style: &str, description: &str, creator: F)
    where
        F: Fn(&mut TransformVec) -> Result<()> + Send + Sync + 'static,
    {
        let data = BuiltinData {
            style: style.to_string(),
            description: description.to_string(),
            creator: Arc::new(creator),
        };

        if let Some(b) = self
            .builtins
            .iter_mut()
            .find(|b| b.style.eq_ignore_ascii_case(style))
        {
            *b = data;
        } else {
            self.builtins.push(data);
        }
    }

    /// Number of builtins.
    pub fn num_builtins(&self) -> usize {
        self.builtins.len()
    }

    /// Style of the builtin at `index`.
    pub fn builtin_style(&self, index: usize) -> Result<&str> {
        self.builtins
            .get(index)
            .map(|b| b.style.as_str())
            .ok_or_else(invalid_index)
    }

    /// Description of the builtin at `index`.
    pub fn builtin_description(&self, index: usize) -> Result<&str> {
        self.builtins
            .get(index)
            .map(|b| b.description.as_str())
            .ok_or_else(invalid_index)
    }

    /// Iterator over all the styles.
    pub fn styles(&self) -> impl Iterator<Item = &str> {
        self.builtins.iter().map(|b| b.style.as_str())
    }

    /// Index of the builtin with the given style (case insensitive).
    pub fn index_of(&self, style: &str) -> Option<usize> {
        self.builtins
            .iter()
            .position(|b| b.style.eq_ignore_ascii_case(style))
    }

    /// Append the (forward) transforms of the builtin at `index`.
    pub fn create_transforms(&self, index: usize, transforms: &mut TransformVec) -> Result<()> {
        let b = self.builtins.get(index).ok_or_else(invalid_index)?;
        (b.creator)(transforms)
    }

    /// Clear the registry and register all the builtins of OCIO.
    pub fn register_all(&mut self) {
        self.builtins.clear();

        self.add_builtin("IDENTITY", "", |ops| {
            add_identity_matrix(ops);
            Ok(())
        });

        // ACES support.
        super::aces::register_all(self);

        // Camera support.
        super::cameras::apple::register_all(self);
        super::cameras::arri::register_all(self);
        super::cameras::canon::register_all(self);
        super::cameras::panasonic::register_all(self);
        super::cameras::red::register_all(self);
        super::cameras::sony::register_all(self);

        // Display support.
        super::displays::register_all(self);
    }
}

/// All the builtin transform styles, in registry order.
pub fn builtin_transform_styles() -> Vec<&'static str> {
    BuiltinTransformRegistry::get().styles().collect()
}

/// Number of builtin transforms.
pub fn num_builtin_transforms() -> usize {
    BuiltinTransformRegistry::get().num_builtins()
}

/// Style of the builtin transform at `index`.
pub fn builtin_transform_style(index: usize) -> Result<&'static str> {
    BuiltinTransformRegistry::get().builtin_style(index)
}

/// Description of the builtin transform with the given style (case
/// insensitive), or `None` if the style is unknown.
pub fn builtin_transform_description(style: &str) -> Option<&'static str> {
    let reg = BuiltinTransformRegistry::get();
    reg.index_of(style)
        .and_then(|i| reg.builtin_description(i).ok())
}

/// Description of the builtin transform at `index`.
pub fn builtin_transform_description_by_index(index: usize) -> Result<&'static str> {
    BuiltinTransformRegistry::get().builtin_description(index)
}

/// The transforms (forward direction) implementing the builtin with the given
/// style (case insensitive).
pub fn builtin_transforms(style: &str) -> Result<TransformVec> {
    let reg = BuiltinTransformRegistry::get();
    let index = reg
        .index_of(style)
        .ok_or_else(|| Error::msg("Invalid built-in transform name."))?;
    let mut v = TransformVec::new();
    reg.create_transforms(index, &mut v)?;
    Ok(v)
}
