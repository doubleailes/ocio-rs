//! Color mixing (color picker) helpers (port of
//! `apphelpers/MixingHelpers.cpp`): [`MixingColorSpaceManager`] and
//! [`MixingSlider`].
//!
//! A user may want to mix colors in different color spaces, typically a
//! scene-linear working space or the display space. Since scene-linear color
//! spaces are not perceptually uniform, UI sliders are compensated using a
//! mapping from linear into an approximately perceptually uniform space.
//! Mixing values may extend outside the typical [0, 1] domain.

use super::color_space_helpers::ColorSpaceInfo;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::{
    DisplayViewTransform, FixedFunctionTransform, GroupTransform, MatrixTransform, Transform,
};
use crate::types::{FixedFunctionStyle, TransformDirection, ROLE_COLOR_PICKING};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const GAMMA: f32 = 2.0;
const LOGSLOPE: f32 = 0.55;
const BREAKPNT: f32 = 0.18;
const NEGSLOPE: f32 = 0.5;
const INVGAMMA: f32 = 0.5; // 1/GAMMA
const INVBREAKPNT: f32 = 0.42426406871192851; // BREAKPNT^(1/GAMMA)
const LOGOFFSET: f32 = 0.83386419090511021; // BREAKPNT^(1/GAMMA) - log10(BREAKPNT)*LOGSLOPE

fn linear_to_perceptual(linear: f32) -> f32 {
    if linear <= 0.0 {
        linear * NEGSLOPE
    } else if linear > BREAKPNT {
        LOGOFFSET + linear.log10() * LOGSLOPE
    } else {
        linear.powf(INVGAMMA)
    }
}

fn perceptual_to_linear(percept: f32) -> f32 {
    if percept <= 0.0 {
        percept / NEGSLOPE
    } else if percept > INVBREAKPNT {
        10.0f32.powf((percept - LOGOFFSET) / LOGSLOPE)
    } else {
        percept.powf(GAMMA)
    }
}

#[derive(Debug)]
struct SliderShared {
    // (min edge, max edge).
    edges: Mutex<(f32, f32)>,
    // Mirror of `MixingColorSpaceManager::is_perceptually_uniform`.
    perceptually_uniform: AtomicBool,
}

/// A UI slider for color mixing. It is a handle on the slider owned by a
/// [`MixingColorSpaceManager`] (clones share the same state, as the
/// reference returned by OCIO's `getSlider`), and it follows the selected
/// mixing space of the manager.
#[derive(Debug, Clone)]
pub struct MixingSlider {
    shared: Arc<SliderShared>,
}

impl MixingSlider {
    fn new() -> Self {
        Self {
            shared: Arc::new(SliderShared {
                edges: Mutex::new((0.0, 1.0)),
                perceptually_uniform: AtomicBool::new(false),
            }),
        }
    }

    fn edges(&self) -> (f32, f32) {
        self.shared
            .edges
            .lock()
            .map(|e| *e)
            .unwrap_or_else(|e| *e.into_inner())
    }

    fn is_perceptually_uniform(&self) -> bool {
        self.shared.perceptually_uniform.load(Ordering::Relaxed)
    }

    /// Set the minimum edge of the slider (in mixing space units).
    pub fn set_slider_min_edge(&self, v: f32) {
        if let Ok(mut e) = self.shared.edges.lock() {
            e.0 = v;
        }
    }

    /// Minimum edge of the slider for the conversion to mixing space.
    pub fn slider_min_edge(&self) -> f32 {
        let (min, max) = self.edges();
        if !self.is_perceptually_uniform() {
            linear_to_perceptual(min.min(max - 0.01))
        } else {
            min
        }
    }

    /// Set the maximum edge of the slider (in mixing space units).
    pub fn set_slider_max_edge(&self, v: f32) {
        if let Ok(mut e) = self.shared.edges.lock() {
            e.1 = v;
        }
    }

    /// Maximum edge of the slider for the conversion to mixing space.
    pub fn slider_max_edge(&self) -> f32 {
        let (min, max) = self.edges();
        if !self.is_perceptually_uniform() {
            linear_to_perceptual(max.max(min + 0.01))
        } else {
            max
        }
    }

    /// Convert from units in distance along the slider to mixing space units.
    pub fn slider_to_mixing(&self, slider_units: f32) -> f32 {
        let min = self.slider_min_edge();
        let max = self.slider_max_edge();
        let percept = min + slider_units * (max - min);
        if !self.is_perceptually_uniform() {
            perceptual_to_linear(percept)
        } else {
            percept
        }
    }

    /// Convert from mixing space units to distance along the slider.
    pub fn mixing_to_slider(&self, mixing_units: f32) -> f32 {
        let percept = if !self.is_perceptually_uniform() {
            linear_to_perceptual(mixing_units)
        } else {
            mixing_units
        };
        // The slider is a window onto the perceptual units. Apply affine based on current
        // left/right or min/max edges of the UI.
        let min = self.slider_min_edge();
        let max = self.slider_max_edge();
        (percept - min) / (max - min)
    }
}

impl fmt::Display for MixingSlider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "minEdge: {}, maxEdge: {}",
            crate::config::utils::fmt_f32(self.slider_min_edge()),
            crate::config::utils::fmt_f32(self.slider_max_edge())
        )
    }
}

const MIXING_ENCODINGS: [&str; 2] = ["RGB", "HSV"];

/// Used to mix (or pick / choose) colors.
#[derive(Debug)]
pub struct MixingColorSpaceManager {
    config: Arc<Config>,
    slider: MixingSlider,
    mixing_spaces: Vec<String>,
    selected_mixing_space_idx: usize,
    selected_mixing_encoding_idx: usize,
    color_picker: Option<ColorSpaceInfo>,
}

impl MixingColorSpaceManager {
    /// A manager for the config (`MixingColorSpaceManager::Create`).
    pub fn new(config: Arc<Config>) -> Self {
        let mut m = Self {
            config,
            slider: MixingSlider::new(),
            mixing_spaces: Vec::new(),
            selected_mixing_space_idx: 0,
            selected_mixing_encoding_idx: 0,
            color_picker: None,
        };
        m.refresh_impl();
        m
    }

    fn update_slider(&self) {
        self.slider
            .shared
            .perceptually_uniform
            .store(self.is_perceptually_uniform(), Ordering::Relaxed);
    }

    fn refresh_impl(&mut self) {
        // Add the mixing spaces.
        self.selected_mixing_space_idx = 0;
        self.mixing_spaces.clear();
        self.color_picker = None;

        if self.config.has_role(ROLE_COLOR_PICKING) {
            self.color_picker = ColorSpaceInfo::from_single_role(&self.config, ROLE_COLOR_PICKING);
            if let Some(cp) = &self.color_picker {
                self.mixing_spaces.push(cp.ui_name().to_string());
            }
        } else {
            // Upstream OCIO note: Replace the 'Display Space' entry (i.e. the color space of the
            // monitor) by the list of all the display color spaces from the configuration.
            self.mixing_spaces.push("Rendering Space".to_string());
            self.mixing_spaces.push("Display Space".to_string());
        }

        // The mixing encodings.
        self.selected_mixing_encoding_idx = 0;
        self.update_slider();
    }

    /// Refresh the instance (e.g. following a config change).
    pub fn refresh(&mut self, config: Arc<Config>) {
        self.config = config;
        self.refresh_impl();
    }

    /// Number of mixing spaces.
    pub fn num_mixing_spaces(&self) -> usize {
        self.mixing_spaces.len()
    }

    /// UI name of the mixing space at `idx`.
    pub fn mixing_space_ui_name(&self, idx: usize) -> Result<&str> {
        self.mixing_spaces
            .get(idx)
            .map(|s| s.as_str())
            .ok_or_else(|| {
                Error::msg(format!(
                    "Invalid mixing space index {} where size is {}.",
                    idx,
                    self.mixing_spaces.len()
                ))
            })
    }

    /// Index of the selected mixing space.
    pub fn selected_mixing_space_idx(&self) -> usize {
        self.selected_mixing_space_idx
    }

    /// Select the mixing space by index.
    pub fn set_selected_mixing_space_idx(&mut self, idx: usize) -> Result<()> {
        if idx >= self.mixing_spaces.len() {
            return Err(Error::msg(format!(
                "Invalid idx for the mixing space index {} where size is {}.",
                idx,
                self.mixing_spaces.len()
            )));
        }
        self.selected_mixing_space_idx = idx;
        self.update_slider();
        Ok(())
    }

    /// Select the mixing space by UI name.
    pub fn set_selected_mixing_space(&mut self, mixing_space: &str) -> Result<()> {
        match self.mixing_spaces.iter().position(|s| s == mixing_space) {
            Some(idx) => {
                self.selected_mixing_space_idx = idx;
                self.update_slider();
                Ok(())
            }
            None => Err(Error::msg(format!(
                "Invalid mixing space name: '{mixing_space}'."
            ))),
        }
    }

    /// True if the selected mixing space is perceptually uniform.
    pub fn is_perceptually_uniform(&self) -> bool {
        // Upstream OCIO note: This response should vary as a function of the mixing space.
        // Only display color spaces are perceptually linear.
        match self.color_picker {
            None => self.selected_mixing_space_idx != 0,
            Some(_) => true,
        }
    }

    /// Number of mixing encodings (RGB and HSV).
    pub fn num_mixing_encodings(&self) -> usize {
        MIXING_ENCODINGS.len()
    }

    /// Name of the mixing encoding at `idx`.
    pub fn mixing_encoding_name(&self, idx: usize) -> Result<&str> {
        MIXING_ENCODINGS.get(idx).copied().ok_or_else(|| {
            Error::msg(format!(
                "Invalid mixing encoding index {} where size is {}.",
                idx,
                MIXING_ENCODINGS.len()
            ))
        })
    }

    /// Index of the selected mixing encoding.
    pub fn selected_mixing_encoding_idx(&self) -> usize {
        self.selected_mixing_encoding_idx
    }

    /// Select the mixing encoding by index.
    pub fn set_selected_mixing_encoding_idx(&mut self, idx: usize) -> Result<()> {
        if idx >= MIXING_ENCODINGS.len() {
            return Err(Error::msg(format!(
                "Invalid idx for the mixing encoding index {} where size is {}.",
                idx,
                MIXING_ENCODINGS.len()
            )));
        }
        self.selected_mixing_encoding_idx = idx;
        Ok(())
    }

    /// Select the mixing encoding by name.
    pub fn set_selected_mixing_encoding(&mut self, mixing_encoding: &str) -> Result<()> {
        match MIXING_ENCODINGS.iter().position(|s| *s == mixing_encoding) {
            Some(idx) => {
                self.selected_mixing_encoding_idx = idx;
                Ok(())
            }
            None => Err(Error::msg(format!(
                "Invalid mixing encoding: '{mixing_encoding}'."
            ))),
        }
    }

    // Processor converting from the working / rendering space to the mixing space (using
    // the RGB encoding rather than HSV).
    fn processor_without_encoding(
        &self,
        working_name: &str,
        display_name: &str,
        view_name: &str,
    ) -> Result<Processor> {
        if let Some(cp) = &self.color_picker {
            // Mix colors in the color_picker role color space.
            self.config.get_processor(working_name, cp.name())
        } else if self.selected_mixing_space_idx > 0 {
            // Mix colors in the selected (display, view) space.
            let dt = DisplayViewTransform::new(working_name, display_name, view_name);
            self.config.get_processor_for_transform(
                &Transform::DisplayView(dt),
                TransformDirection::Forward,
            )
        } else {
            // Mix colors directly in the working / rendering space.
            self.config.get_processor_for_transform(
                &Transform::Matrix(MatrixTransform::default()),
                TransformDirection::Forward,
            )
        }
    }

    /// Processor from the working space to the mixing space (forward) or
    /// back (inverse).
    pub fn get_processor(
        &self,
        working_name: &str,
        display_name: &str,
        view_name: &str,
        direction: TransformDirection,
    ) -> Result<Processor> {
        let mut group = GroupTransform::new();
        let processor = self.processor_without_encoding(working_name, display_name, view_name)?;
        group.append(processor.create_group_transform());

        if self.selected_mixing_encoding_idx == 1 {
            // i.e. HSV
            group.append(FixedFunctionTransform::new(
                FixedFunctionStyle::RgbToHsv,
                &[],
            ));
        }

        self.config
            .get_processor_for_transform(&Transform::Group(group), direction)
    }

    /// The slider (a handle sharing its state with the manager).
    pub fn slider(&self) -> MixingSlider {
        self.slider.clone()
    }

    /// The slider, after setting its edges.
    pub fn slider_with_edges(&self, min_edge: f32, max_edge: f32) -> MixingSlider {
        self.slider.set_slider_min_edge(min_edge);
        self.slider.set_slider_max_edge(max_edge);
        self.slider.clone()
    }
}

impl fmt::Display for MixingColorSpaceManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config: {}", self.config.cache_id())?;
        write!(f, ", slider: [{}]", self.slider)?;
        if !self.mixing_spaces.is_empty() {
            write!(f, ", mixingSpaces: [{}]", self.mixing_spaces.join(", "))?;
        }
        write!(
            f,
            ", selectedMixingSpaceIdx: {}",
            self.selected_mixing_space_idx
        )?;
        write!(
            f,
            ", selectedMixingEncodingIdx: {}",
            self.selected_mixing_encoding_idx
        )?;
        if self.color_picker.is_some() {
            write!(f, ", colorPicking")?;
        }
        Ok(())
    }
}
