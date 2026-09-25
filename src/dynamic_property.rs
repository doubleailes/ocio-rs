//! Dynamic properties: values of a processor that can be changed after the
//! processor is created (exposure, contrast, gamma, grading values).
//!
//! A dynamic property is a shared handle (`Arc<RwLock<..>>`). The op holding it
//! reads the current value on each `apply`; the client mutates it through the
//! handle returned by `Processor::get_dynamic_property`.

use crate::transforms::grading::{GradingHueCurve, GradingPrimary, GradingRgbCurve, GradingTone};
use crate::types::DynamicPropertyType;
use std::sync::{Arc, RwLock};

/// A shared, mutable value.
#[derive(Debug, Default)]
pub struct SharedValue<T>(Arc<RwLock<T>>);

impl<T> Clone for SharedValue<T> {
    fn clone(&self) -> Self {
        SharedValue(self.0.clone())
    }
}

impl<T: Clone> SharedValue<T> {
    pub fn new(v: T) -> Self {
        SharedValue(Arc::new(RwLock::new(v)))
    }
    pub fn get(&self) -> T {
        self.0.read().unwrap().clone()
    }
    pub fn set(&self, v: T) {
        *self.0.write().unwrap() = v;
    }
    /// True if both handles point to the same storage.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Access the value by reference under a read lock.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.0.read().unwrap())
    }
}

/// Handle to a dynamic property.
#[derive(Debug, Clone)]
pub enum DynamicProperty {
    Exposure(SharedValue<f64>),
    Contrast(SharedValue<f64>),
    Gamma(SharedValue<f64>),
    GradingPrimary(SharedValue<GradingPrimary>),
    GradingRgbCurve(SharedValue<GradingRgbCurve>),
    GradingTone(SharedValue<GradingTone>),
    GradingHueCurve(SharedValue<GradingHueCurve>),
}

impl DynamicProperty {
    pub fn property_type(&self) -> DynamicPropertyType {
        match self {
            DynamicProperty::Exposure(_) => DynamicPropertyType::Exposure,
            DynamicProperty::Contrast(_) => DynamicPropertyType::Contrast,
            DynamicProperty::Gamma(_) => DynamicPropertyType::Gamma,
            DynamicProperty::GradingPrimary(_) => DynamicPropertyType::GradingPrimary,
            DynamicProperty::GradingRgbCurve(_) => DynamicPropertyType::GradingRgbCurve,
            DynamicProperty::GradingTone(_) => DynamicPropertyType::GradingTone,
            DynamicProperty::GradingHueCurve(_) => DynamicPropertyType::GradingHueCurve,
        }
    }

    /// Double value for exposure / contrast / gamma properties.
    pub fn as_double(&self) -> Option<&SharedValue<f64>> {
        match self {
            DynamicProperty::Exposure(v)
            | DynamicProperty::Contrast(v)
            | DynamicProperty::Gamma(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_grading_primary(&self) -> Option<&SharedValue<GradingPrimary>> {
        match self {
            DynamicProperty::GradingPrimary(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_grading_rgb_curve(&self) -> Option<&SharedValue<GradingRgbCurve>> {
        match self {
            DynamicProperty::GradingRgbCurve(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_grading_tone(&self) -> Option<&SharedValue<GradingTone>> {
        match self {
            DynamicProperty::GradingTone(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_grading_hue_curve(&self) -> Option<&SharedValue<GradingHueCurve>> {
        match self {
            DynamicProperty::GradingHueCurve(v) => Some(v),
            _ => None,
        }
    }
}
