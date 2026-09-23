//! The mergers of each config section (port of
//! `apphelpers/mergeconfigs/SectionMerger.cpp`).
//!
//! All the section mergers assume that the merged config is initialized
//! from the base config.

use super::merge_utils::{
    find_equivalent_colorspace, initialize_color_space_fingerprints,
    initialize_ref_space_converters, update_reference_colorspace, update_reference_view,
    ColorSpaceFingerprints,
};
use super::{ConfigMergingParameters, MergeStrategies};
use crate::config::logging::log_warning;
use crate::config::tokens::{CustomKeys, TokensManager};
use crate::config::transform_display::format_transform;
use crate::config::utils::{compare, contain, join, remove, split, trim};
use crate::config::{ColorSpace, Config, FileRules, ViewingRules, FILE_PATH_SEARCH_RULE_NAME};
use crate::config::{NamedTransform, DEFAULT_RULE_NAME};
use crate::error::{Error, Result};
use crate::transforms::Transform;
use crate::types::{
    ColorSpaceVisibility, NamedTransformVisibility, ReferenceSpaceType, SearchReferenceSpaceType,
    ViewTransformDirection, ViewType,
};

/// The configs and parameters of a merge.
pub(crate) struct MergeHandlerOptions<'a> {
    pub base_config: &'a Config,
    pub input_config: &'a Config,
    pub params: &'a ConfigMergingParameters,
    pub merged_config: &'a mut Config,
}

/// Log a warning, or fail if `must_throw` (`SectionMerger::notify`).
fn notify(s: &str, must_throw: bool) -> Result<()> {
    if !must_throw {
        // By logging, we can see all errors.
        log_warning(s);
        Ok(())
    } else {
        // By failing, we only get the first error on conflict, but also stop the merge.
        Err(Error::msg(s))
    }
}

fn unsupported(name: &str, strategy: &str) -> Result<()> {
    log_warning(&format!(
        "{name} section does not support strategy '{strategy}'"
    ));
    Ok(())
}

/// A section merger: dispatches to the handler of the strategy.
pub(crate) trait SectionMerger {
    /// Name of the section (used in messages).
    fn name(&self) -> &'static str;
    /// The strategy used by the merger.
    fn strategy(&self) -> MergeStrategies;

    fn handle_prefer_input(&mut self) -> Result<()> {
        unsupported(self.name(), "PreferInput")
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        unsupported(self.name(), "PreferBase")
    }
    fn handle_input_only(&mut self) -> Result<()> {
        unsupported(self.name(), "InputOnly")
    }
    fn handle_base_only(&mut self) -> Result<()> {
        unsupported(self.name(), "BaseOnly")
    }
    fn handle_remove(&mut self) -> Result<()> {
        unsupported(self.name(), "Remove")
    }

    /// Merge the section.
    fn merge(&mut self) -> Result<()> {
        match self.strategy() {
            MergeStrategies::PreferInput => self.handle_prefer_input(),
            MergeStrategies::PreferBase => self.handle_prefer_base(),
            MergeStrategies::InputOnly => self.handle_input_only(),
            MergeStrategies::BaseOnly => self.handle_base_only(),
            MergeStrategies::Remove => self.handle_remove(),
            // Nothing to do.
            MergeStrategies::Unspecified => Ok(()),
        }
    }
}

fn section_strategy(strat: MergeStrategies, params: &ConfigMergingParameters) -> MergeStrategies {
    if strat != MergeStrategies::Unspecified {
        strat
    } else {
        params.default_strategy()
    }
}

fn split_active_list(list: &str) -> Vec<String> {
    if list.is_empty() {
        // Upstream OCIO note: Need to handle quoted substrings.
        Vec::new()
    } else {
        split(list, ',')
    }
}

fn merge_strings_without_duplicates(input: &[String], merged: &mut Vec<String>) {
    // Note that contain requires a full match, hence the items of merged are trimmed as well.
    for m in merged.iter_mut() {
        *m = trim(m).to_string();
    }
    for item in input {
        let t = trim(item);
        if !t.is_empty() && !contain(merged, t) {
            merged.push(t.to_string());
        }
    }
}

macro_rules! declare_merger {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub(crate) struct $name<'o, 'a> {
            o: &'o mut MergeHandlerOptions<'a>,
            strategy: MergeStrategies,
        }
    };
}

// ------------------------------------------------------------------------------------------
// GeneralMerger

declare_merger!(
    /// Name, description, version and luma coefficients (always uses the default strategy).
    GeneralMerger
);

impl<'o, 'a> GeneralMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        // This merger always uses the default strategy as this is for properties that are not
        // linked to a specific section.
        let strategy = o.params.default_strategy();
        Self { o, strategy }
    }

    fn set_merged_config_version(&mut self) -> Result<()> {
        let (input_major, input_minor) = (
            self.o.input_config.major_version(),
            self.o.input_config.minor_version(),
        );
        let (base_major, base_minor) = (
            self.o.base_config.major_version(),
            self.o.base_config.minor_version(),
        );
        let (mut major, mut minor) = (base_major, base_minor);
        // Use the higher of the input or base version.
        if input_major * 100 + input_minor > base_major * 100 + base_minor {
            major = input_major;
            minor = input_minor;
        }
        // A merge always produces at least a v2 config.
        if 2 * 100 > major * 100 + minor {
            major = 2;
            minor = 0;
        }
        self.o.merged_config.set_version(major, minor)
    }

    fn apply(&mut self, from_input: bool) -> Result<()> {
        let src = if from_input {
            self.o.input_config
        } else {
            self.o.base_config
        };
        let params = self.o.params;

        // Set the config name.
        if !params.name().is_empty() {
            // Use name from override.
            self.o.merged_config.set_name(params.name());
        } else {
            self.o.merged_config.set_name(src.name());
        }

        // Set the config description.
        if !params.description().is_empty() {
            self.o.merged_config.set_description(params.description());
        } else {
            self.o.merged_config.set_description(src.description());
        }

        // Use the higher value for ocio_profile_version. Note that the default strategy is used
        // for the general merger but other strategies may be used for other sections of the
        // config and so always use the higher of the two config versions.
        self.set_merged_config_version()?;

        let rgb = src.default_luma_coefs();
        self.o.merged_config.set_default_luma_coefs(&rgb);
        Ok(())
    }
}

impl SectionMerger for GeneralMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "General"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.apply(true)
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.apply(false)
    }
    fn handle_input_only(&mut self) -> Result<()> {
        self.apply(true)
    }
    fn handle_base_only(&mut self) -> Result<()> {
        self.apply(false)
    }
}

// ------------------------------------------------------------------------------------------
// RolesMerger

declare_merger!(
    /// The roles section.
    RolesMerger
);

impl<'o, 'a> RolesMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        let strategy = section_strategy(o.params.roles(), o.params);
        Self { o, strategy }
    }

    fn merge_input_roles(&mut self) -> Result<()> {
        let input = self.o.input_config;
        let params = self.o.params;
        let merged = &mut *self.o.merged_config;

        // Insert roles from input config.
        for i in 0..input.num_roles() {
            let name = input.role_name(i);
            let role_cs_name = input.role_color_space(name);

            if merged.has_role(name) {
                // The base config already has this role.
                let base_role_cs_name = merged.role_color_space(name).to_string();
                let strategy = params.roles();
                if !compare(role_cs_name, &base_role_cs_name) {
                    // The color spaces are different. Replace based on the strategy.
                    if strategy == MergeStrategies::PreferInput
                        || strategy == MergeStrategies::InputOnly
                    {
                        merged.set_role(name, Some(role_cs_name))?;
                    }
                    notify(
                        &format!(
                            "The Input config contains a role that would override Base config role '{name}'."
                        ),
                        params.is_error_on_conflict(),
                    )?;
                }
                continue;
            }

            // Check for any conflicts. Not allowing input roles to override color spaces or
            // named transforms in the base config. The merge strategy only applies to
            // overriding base config roles.

            if let Some(existing) = merged.get_color_space(name) {
                // There is a conflict, figure out what it is.
                let msg = if compare(existing.name(), name) {
                    format!(
                        "The Input config contains a role '{}' that would override Base config color space '{}'.",
                        name,
                        existing.name()
                    )
                } else if existing.has_alias(name) {
                    format!(
                        "The Input config contains a role '{}' that would override an alias of Base config color space '{}'.",
                        name,
                        existing.name()
                    )
                } else {
                    // (Should never happen.)
                    return Err(Error::msg(format!(
                        "Problem merging role: '{name}' due to color space conflict."
                    )));
                };
                notify(&msg, params.is_error_on_conflict())?;
                continue;
            }

            if let Some(existing) = merged.get_named_transform(name) {
                // There is a conflict, figure out what it is.
                let msg = if compare(existing.name(), name) {
                    format!(
                        "The Input config contains a role '{}' that would override Base config named transform: '{}'.",
                        name,
                        existing.name()
                    )
                } else if existing.has_alias(name) {
                    format!(
                        "The Input config contains a role '{}' that would override an alias of Base config named transform: '{}'.",
                        name,
                        existing.name()
                    )
                } else {
                    // (Should never happen.)
                    return Err(Error::msg(format!("Problem merging role: '{name}'.")));
                };
                notify(&msg, params.is_error_on_conflict())?;
                continue;
            }

            // No conflicts, go ahead and merge it.
            merged.set_role(name, Some(role_cs_name))?;
        }
        Ok(())
    }
}

impl SectionMerger for RolesMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "Roles"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.merge_input_roles()
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.merge_input_roles()
    }
    fn handle_input_only(&mut self) -> Result<()> {
        // Remove the roles from base and take the roles from input.
        let base = self.o.base_config;
        for i in 0..base.num_roles() {
            self.o.merged_config.set_role(base.role_name(i), None)?;
        }
        self.merge_input_roles()
    }
    fn handle_base_only(&mut self) -> Result<()> {
        // Nothing to do, since the merged config is initialized from the base config.
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let input = self.o.input_config;
        for i in 0..input.num_roles() {
            let name = input.role_name(i);
            if self.o.merged_config.has_role(name) {
                self.o.merged_config.set_role(name, None)?;
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// FileRulesMerger

fn custom_keys_are_equal(
    n1: usize,
    key1: &dyn Fn(usize) -> (String, String),
    n2: usize,
    key2: &dyn Fn(usize) -> (String, String),
) -> bool {
    // Compare the custom keys, handling the case where they may be in a different order.
    if n1 != n2 {
        return false;
    }
    let mut keys = CustomKeys::default();
    for m in 0..n1 {
        let (k, v) = key1(m);
        let _ = keys.set(&k, &v);
    }
    for m in 0..n2 {
        let (k, v) = key2(m);
        if !keys.has_key(&k) {
            return false;
        }
        if !compare(keys.value_for_key(&k).unwrap_or(""), &v) {
            return false;
        }
    }
    true
}

fn file_rules_are_equal(f1: &FileRules, f1_idx: usize, f2: &FileRules, f2_idx: usize) -> bool {
    // NB: No need to compare the name of the rules, that should be done in the caller.

    // Compare color space name, pattern, extension, and regex strings.
    let s = |r: Result<&str>| r.unwrap_or("").to_string();
    if !compare(&s(f1.color_space(f1_idx)), &s(f2.color_space(f2_idx)))
        || !compare(&s(f1.pattern(f1_idx)), &s(f2.pattern(f2_idx)))
        || !compare(&s(f1.regex(f1_idx)), &s(f2.regex(f2_idx)))
        || !compare(&s(f1.extension(f1_idx)), &s(f2.extension(f2_idx)))
    {
        return false;
    }

    let key = |f: &FileRules, idx: usize, m: usize| {
        (
            f.custom_key_name(idx, m).unwrap_or("").to_string(),
            f.custom_key_value(idx, m).unwrap_or("").to_string(),
        )
    };
    custom_keys_are_equal(
        f1.num_custom_keys(f1_idx).unwrap_or(0),
        &|m| key(f1, f1_idx, m),
        f2.num_custom_keys(f2_idx).unwrap_or(0),
        &|m| key(f2, f2_idx, m),
    )
}

fn copy_rule(
    input: &FileRules,
    input_idx: usize,
    merged: &mut FileRules,
    merged_idx: usize,
) -> Result<()> {
    // Handle the case where the rule is ColorSpaceNamePathSearch.
    let name = input.name(input_idx)?.to_string();
    if compare(&name, FILE_PATH_SEARCH_RULE_NAME) {
        return merged.insert_path_search_rule(merged_idx);
    }

    // Normal rule case.
    let regex = input.regex(input_idx)?;
    let cs = input.color_space(input_idx)?;
    if regex.is_empty() {
        // The regex is empty --> handle it as a pattern & extension type rule.
        let pattern = input.pattern(input_idx)?;
        let extension = input.extension(input_idx)?;
        merged.insert_rule(
            merged_idx,
            &name,
            cs,
            if pattern.is_empty() { "*" } else { pattern },
            if extension.is_empty() { "*" } else { extension },
        )?;
    } else {
        // Handle it as a regex type rule.
        merged.insert_rule_regex(merged_idx, &name, cs, regex)?;
    }

    // Copy over any custom keys.
    for k in 0..input.num_custom_keys(input_idx)? {
        merged.set_custom_key(
            merged_idx,
            input.custom_key_name(input_idx, k)?,
            input.custom_key_value(input_idx, k)?,
        )?;
    }
    Ok(())
}

declare_merger!(
    /// The file_rules section (and strictparsing).
    FileRulesMerger
);

impl<'o, 'a> FileRulesMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        let strategy = section_strategy(o.params.file_rules(), o.params);
        Self { o, strategy }
    }

    fn conflict_message(name: &str) -> String {
        format!(
            "The Input config contains a value that would override the Base config: file_rules: {name}"
        )
    }

    fn add_rules_if_not_present(&self, input: &FileRules, merged: &mut FileRules) -> Result<()> {
        for idx in 0..input.num_entries() {
            let name = input.name(idx)?.to_string();
            match merged.index_for_rule(&name) {
                Ok(merged_idx) => {
                    // Based on the name, this file rule exists in the merged config. If the
                    // rules are not identical, need to report the conflict.
                    if !file_rules_are_equal(merged, merged_idx, input, idx) {
                        notify(
                            &Self::conflict_message(&name),
                            self.o.params.is_error_on_conflict(),
                        )?;
                    }
                }
                Err(_) => {
                    // The rule does not exist, add it in the penultimate position, before the
                    // default rule (always present).
                    let pos = merged.num_entries().saturating_sub(1);
                    copy_rule(input, idx, merged, pos)?;
                }
            }
        }
        Ok(())
    }

    fn add_rules_and_overwrite(&self, input: &FileRules, merged: &mut FileRules) -> Result<()> {
        for idx in 0..input.num_entries() {
            let name = input.name(idx)?.to_string();
            match merged.index_for_rule(&name) {
                Ok(merged_idx) => {
                    if !file_rules_are_equal(merged, merged_idx, input, idx) {
                        // Overwrite the existing rule.
                        let overwrite = |merged: &mut FileRules| -> Result<()> {
                            if !compare(&name, DEFAULT_RULE_NAME) {
                                merged.remove_rule(merged_idx)?;
                                copy_rule(input, idx, merged, merged_idx)
                            } else {
                                merged.set_default_rule_color_space(input.color_space(idx)?)
                            }
                        };
                        let msg = match overwrite(merged) {
                            Ok(()) => Self::conflict_message(&name),
                            Err(e) => e.message().to_string(),
                        };
                        notify(&msg, self.o.params.is_error_on_conflict())?;
                    }
                }
                Err(_) => {
                    // The rule does not exist, add it in the penultimate position, before the
                    // default rule (always present).
                    let pos = merged.num_entries().saturating_sub(1);
                    copy_rule(input, idx, merged, pos)?;
                }
            }
        }
        Ok(())
    }
}

impl SectionMerger for FileRulesMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "FileRules"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        let base_fr = self.o.base_config.file_rules();
        let input_fr = self.o.input_config.file_rules();

        // Handle strictparsing.
        let strict = self.o.input_config.is_strict_parsing_enabled();
        self.o.merged_config.set_strict_parsing_enabled(strict);

        // The technique depends on whether the input rules should go first or not.
        let merged = if self.o.params.is_input_first() {
            // Copy the file rules from the input config and insert the base rules if not
            // present (right before the default rule).
            let mut merged = input_fr.clone();
            self.add_rules_if_not_present(base_fr, &mut merged)?;
            merged
        } else {
            // Copy the file rules from the base config and insert the input rules, overwriting
            // the existing ones.
            let mut merged = base_fr.clone();
            self.add_rules_and_overwrite(input_fr, &mut merged)?;
            merged
        };
        self.o.merged_config.set_file_rules(&merged);
        Ok(())
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        let base_fr = self.o.base_config.file_rules();
        let input_fr = self.o.input_config.file_rules();

        // Handle strictparsing: nothing to do, keep the base config value.

        let merged = if self.o.params.is_input_first() {
            // Copy the file rules from the input config and insert the base rules, overwriting
            // the existing ones.
            let mut merged = input_fr.clone();
            self.add_rules_and_overwrite(base_fr, &mut merged)?;
            merged
        } else {
            // Copy the file rules from the base config and insert the input rules if not
            // present.
            let mut merged = base_fr.clone();
            self.add_rules_if_not_present(input_fr, &mut merged)?;
            merged
        };
        self.o.merged_config.set_file_rules(&merged);
        Ok(())
    }
    fn handle_input_only(&mut self) -> Result<()> {
        // Handle strictparsing.
        let strict = self.o.input_config.is_strict_parsing_enabled();
        self.o.merged_config.set_strict_parsing_enabled(strict);

        // Simply take the rules from the input config.
        let rules = self.o.input_config.file_rules().clone();
        self.o.merged_config.set_file_rules(&rules);
        Ok(())
    }
    fn handle_base_only(&mut self) -> Result<()> {
        // Supported, but nothing to do.
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let input_fr = self.o.input_config.file_rules();
        let mut merged = self.o.base_config.file_rules().clone();

        for f in 0..input_fr.num_entries() {
            let name = input_fr.name(f)?;
            // Never remove the Default rule.
            if compare(name, DEFAULT_RULE_NAME) {
                continue;
            }
            // Remove the rule if present (regardless of whether the content matches the base
            // config).
            if let Ok(idx) = merged.index_for_rule(name) {
                let _ = merged.remove_rule(idx);
            }
        }
        self.o.merged_config.set_file_rules(&merged);
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// DisplayViewMerger

fn viewing_rules_are_equal(
    r1: &ViewingRules,
    r1_idx: usize,
    r2: &ViewingRules,
    r2_idx: usize,
) -> bool {
    // NB: No need to compare the name of the rules, that should be done in the caller.

    // Compare color space tokens, handling the case where they may be in a different order.
    let n1 = r1.num_color_spaces(r1_idx).unwrap_or(0);
    let n2 = r2.num_color_spaces(r2_idx).unwrap_or(0);
    if n1 != n2 {
        return false;
    }
    let mut tokens = TokensManager::new();
    for m in 0..n1 {
        tokens.add_token(r1.color_space(r1_idx, m).unwrap_or(""));
    }
    for m in 0..n2 {
        if !tokens.has_token(r2.color_space(r2_idx, m).unwrap_or("")) {
            return false;
        }
    }

    // Compare encoding tokens, handling the case where they may be in a different order.
    let n1 = r1.num_encodings(r1_idx).unwrap_or(0);
    let n2 = r2.num_encodings(r2_idx).unwrap_or(0);
    if n1 != n2 {
        return false;
    }
    let mut tokens = TokensManager::new();
    for m in 0..n1 {
        tokens.add_token(r1.encoding(r1_idx, m).unwrap_or(""));
    }
    for m in 0..n2 {
        if !tokens.has_token(r2.encoding(r2_idx, m).unwrap_or("")) {
            return false;
        }
    }

    let key = |r: &ViewingRules, idx: usize, m: usize| {
        (
            r.custom_key_name(idx, m).unwrap_or("").to_string(),
            r.custom_key_value(idx, m).unwrap_or("").to_string(),
        )
    };
    custom_keys_are_equal(
        r1.num_custom_keys(r1_idx).unwrap_or(0),
        &|m| key(r1, r1_idx, m),
        r2.num_custom_keys(r2_idx).unwrap_or(0),
        &|m| key(r2, r2_idx, m),
    )
}

fn copy_viewing_rule(src: &ViewingRules, src_idx: usize, dst_idx: usize, rules: &mut ViewingRules) {
    let copy = |rules: &mut ViewingRules| -> Result<()> {
        rules.insert_rule(dst_idx, src.name(src_idx)?)?;
        for j in 0..src.num_color_spaces(src_idx)? {
            rules.add_color_space(dst_idx, src.color_space(src_idx, j)?)?;
        }
        for k in 0..src.num_encodings(src_idx)? {
            rules.add_encoding(dst_idx, src.encoding(src_idx, k)?)?;
        }
        for l in 0..src.num_custom_keys(src_idx)? {
            rules.set_custom_key(
                dst_idx,
                src.custom_key_name(src_idx, l)?,
                src.custom_key_value(src_idx, l)?,
            )?;
        }
        Ok(())
    };
    // Don't add it if any errors, and continue.
    let _ = copy(rules);
}

fn add_unique_viewing_rules(rules: &ViewingRules, merged: &mut ViewingRules) {
    for i in 0..rules.num_entries() {
        let name = rules.name(i).unwrap_or("");
        // Take the rule from the first config if it does not exist.
        if merged.index_for_rule(name).is_err() {
            let n = merged.num_entries();
            copy_viewing_rule(rules, i, n, merged);
        }
    }
}

declare_merger!(
    /// The displays / views section: shared_views, displays, viewing_rules,
    /// virtual_display, active_displays and active_views.
    DisplayViewMerger
);

impl<'o, 'a> DisplayViewMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        let strategy = section_strategy(o.params.display_views(), o.params);
        Self { o, strategy }
    }

    fn conflict(&self, what: &str) -> Result<()> {
        notify(
            &format!(
                "The Input config contains a value that would override the Base config: {what}"
            ),
            self.o.params.is_error_on_conflict(),
        )
    }

    fn add_display_view_from(&mut self, cfg: &Config, disp: &str, view: &str) -> Result<()> {
        // (Note this works for either the new or old style of view.)
        self.o.merged_config.add_display_view_full(
            disp,
            view,
            cfg.display_view_transform_name(disp, view),
            cfg.display_view_color_space_name(disp, view),
            cfg.display_view_looks(disp, view),
            cfg.display_view_rule(disp, view),
            cfg.display_view_description(disp, view),
        )
    }

    fn add_virtual_view_from(&mut self, cfg: &Config, view: &str) -> Result<()> {
        self.o.merged_config.add_virtual_display_view(
            view,
            cfg.virtual_display_view_transform_name(view),
            cfg.virtual_display_view_color_space_name(view),
            cfg.virtual_display_view_looks(view),
            cfg.virtual_display_view_rule(view),
            cfg.virtual_display_view_description(view),
        )
    }

    fn add_shared_view_from(&mut self, cfg: &Config, view: &str) -> Result<()> {
        self.o.merged_config.add_shared_view(
            view,
            cfg.display_view_transform_name("", view),
            cfg.display_view_color_space_name("", view),
            cfg.display_view_looks("", view),
            cfg.display_view_rule("", view),
            cfg.display_view_description("", view),
        )
    }

    fn add_unique_displays(&mut self, cfg: &Config) -> Result<()> {
        // For each display, add any views from cfg that are not already in the merged config.
        for i in 0..cfg.num_displays_all() {
            let disp = cfg.display_all(i).to_string();

            // Display-defined views.
            for v in 0..cfg.num_views_by_type(ViewType::DisplayDefined, &disp) {
                let view = cfg
                    .view_by_type(ViewType::DisplayDefined, &disp, v)
                    .to_string();
                // True if the display contains either a display-defined or shared view with this
                // name.
                let exists = self.o.merged_config.has_view(&disp, &view);
                if !view.is_empty() && !exists {
                    self.add_display_view_from(cfg, &disp, &view)?;
                }
            }

            // Shared views.
            for v in 0..cfg.num_views_by_type(ViewType::Shared, &disp) {
                let view = cfg.view_by_type(ViewType::Shared, &disp, v).to_string();
                let exists = self.o.merged_config.has_view(&disp, &view);
                if !view.is_empty() && !exists {
                    self.o.merged_config.add_display_shared_view(&disp, &view)?;
                }
            }
        }
        Ok(())
    }

    fn add_unique_virtual_views(&mut self, cfg: &Config) -> Result<()> {
        // Display-defined views.
        for v in 0..cfg.virtual_display_num_views(ViewType::DisplayDefined) {
            let view = cfg
                .virtual_display_view(ViewType::DisplayDefined, v)
                .to_string();
            let exists = self.o.merged_config.has_virtual_view(&view);
            if !view.is_empty() && !exists {
                self.add_virtual_view_from(cfg, &view)?;
            }
        }

        // Shared views.
        for v in 0..cfg.virtual_display_num_views(ViewType::Shared) {
            let view = cfg.virtual_display_view(ViewType::Shared, v).to_string();
            let exists = self.o.merged_config.has_virtual_view(&view);
            if !view.is_empty() && !exists {
                self.o
                    .merged_config
                    .add_virtual_display_shared_view(&view)?;
            }
        }
        Ok(())
    }

    fn process_displays(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        // Iterate over the first config's displays.
        for i in 0..first.num_displays_all() {
            let disp = first.display_all(i).to_string();

            // Iterate over this display's display-defined views.
            for v in 0..first.num_views_by_type(ViewType::DisplayDefined, &disp) {
                let view = first
                    .view_by_type(ViewType::DisplayDefined, &disp, v)
                    .to_string();
                if view.is_empty() {
                    continue;
                }

                // Both configs may have the same display with the same view name, but it's a
                // display-defined view in one and a shared view in the other. This check returns
                // true if it exists in either form.
                let exists_in_second = second.has_view(&disp, &view);

                if exists_in_second && !Config::are_views_equal(first, second, &disp, &view) {
                    self.conflict(&format!("display: {disp}, view: {view}"))?;
                }

                if exists_in_second && prefer_second {
                    // Take the view from the second config, as the same type of view.
                    if second.is_view_shared(&disp, &view) {
                        // Note that this may change the order in a way that does not follow the
                        // preference of input-first or base-first.
                        self.o.merged_config.add_display_shared_view(&disp, &view)?;
                    } else {
                        self.add_display_view_from(second, &disp, &view)?;
                    }
                } else {
                    // Take the view from the first config (where it is display-defined).
                    self.add_display_view_from(first, &disp, &view)?;
                }
            }

            // Iterate over this display's shared views.
            for v in 0..first.num_views_by_type(ViewType::Shared, &disp) {
                let view = first.view_by_type(ViewType::Shared, &disp, v).to_string();
                if view.is_empty() {
                    continue;
                }
                let exists_in_second = second.has_view(&disp, &view);

                if exists_in_second && prefer_second {
                    // This was a shared view in the first config but it may not be in the second
                    // config. Add it as the same type of view.
                    if second.is_view_shared(&disp, &view) {
                        self.o.merged_config.add_display_shared_view(&disp, &view)?;
                    } else {
                        if !Config::are_views_equal(first, second, &disp, &view) {
                            self.conflict(&format!("display: {disp}, view: {view}"))?;
                        }
                        self.add_display_view_from(second, &disp, &view)?;
                    }
                } else {
                    // Note: The error-on-conflict check happens in process_shared_views, this is
                    // just adding the reference, so it's not checked again here.
                    self.o.merged_config.add_display_shared_view(&disp, &view)?;
                }
            }
        }

        // Add the remaining views of all displays of the second config (only the views that
        // are not already present).
        self.add_unique_displays(second)
    }

    fn process_virtual_display(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        for v in 0..first.virtual_display_num_views(ViewType::DisplayDefined) {
            let view = first
                .virtual_display_view(ViewType::DisplayDefined, v)
                .to_string();
            if view.is_empty() {
                continue;
            }

            let exists_in_second = second.has_virtual_view(&view);

            if exists_in_second && !Config::are_virtual_views_equal(first, second, &view) {
                self.conflict(&format!("virtual_display: {view}"))?;
            }

            if exists_in_second && prefer_second {
                // Take the view from the second config, as the same type of view.
                if second.is_virtual_view_shared(&view) {
                    self.o
                        .merged_config
                        .add_virtual_display_shared_view(&view)?;
                } else {
                    self.add_virtual_view_from(second, &view)?;
                }
            } else {
                // Take the view from the first config (where it is display-defined).
                self.add_virtual_view_from(first, &view)?;
            }
        }

        // Iterate over the shared views.
        for v in 0..first.virtual_display_num_views(ViewType::Shared) {
            let view = first.virtual_display_view(ViewType::Shared, v).to_string();
            if view.is_empty() {
                continue;
            }
            let exists_in_second = second.has_virtual_view(&view);

            if exists_in_second && prefer_second {
                if second.is_virtual_view_shared(&view) {
                    self.o
                        .merged_config
                        .add_virtual_display_shared_view(&view)?;
                } else {
                    if !Config::are_virtual_views_equal(first, second, &view) {
                        self.conflict(&format!("virtual_display: {view}"))?;
                    }
                    self.add_virtual_view_from(second, &view)?;
                }
            } else {
                // Note: The error-on-conflict check happens in process_shared_views.
                self.o
                    .merged_config
                    .add_virtual_display_shared_view(&view)?;
            }
        }

        // Add the remaining views from the second config.
        self.add_unique_virtual_views(second)
    }

    fn add_unique_shared_views(&mut self, cfg: &Config) -> Result<()> {
        // Add any shared views that are not already in the merged config.
        for v in 0..cfg.num_views_by_type(ViewType::Shared, "") {
            let view = cfg.view_by_type(ViewType::Shared, "", v).to_string();
            let exists = self.o.merged_config.has_view("", &view);
            if !view.is_empty() && !exists {
                self.add_shared_view_from(cfg, &view)?;
            }
        }
        Ok(())
    }

    fn process_shared_views(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        // Iterate over all shared views of the first config.
        for v in 0..first.num_views_by_type(ViewType::Shared, "") {
            let view = first.view_by_type(ViewType::Shared, "", v).to_string();
            if view.is_empty() {
                continue;
            }
            let exists_in_second = second.has_view("", &view);

            if exists_in_second && !Config::are_views_equal(first, second, "", &view) {
                self.conflict(&format!("shared_views: {view}"))?;
            }

            if exists_in_second && prefer_second {
                // Take the shared view from the second config.
                self.add_shared_view_from(second, &view)?;
            } else {
                // Take the shared view from the first config.
                self.add_shared_view_from(first, &view)?;
            }
        }

        // Add the remaining shared views that are only in the second config.
        self.add_unique_shared_views(second)
    }

    fn process_active_lists(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        let params = self.o.params;

        // Merge active_displays.
        let active_displays = params.active_displays();
        if !active_displays.is_empty() {
            // Take active_displays from overrides.
            self.o.merged_config.set_active_displays(&active_displays)?;
        } else {
            let merged = if params.is_input_first() {
                let base_list = split_active_list(&base.active_displays());
                let mut merged = split_active_list(&input.active_displays());
                merge_strings_without_duplicates(&base_list, &mut merged);
                merged
            } else {
                let input_list = split_active_list(&input.active_displays());
                let mut merged = split_active_list(&base.active_displays());
                merge_strings_without_duplicates(&input_list, &mut merged);
                merged
            };
            // NB: join adds a space after the comma.
            self.o
                .merged_config
                .set_active_displays(&join(&merged, ','))?;
        }

        // Merge active_views.
        let active_views = params.active_views();
        if !active_views.is_empty() {
            // Take active_views from overrides.
            self.o.merged_config.set_active_views(&active_views)?;
        } else {
            let merged = if params.is_input_first() {
                let base_list = split_active_list(&base.active_views());
                let mut merged = split_active_list(&input.active_views());
                merge_strings_without_duplicates(&base_list, &mut merged);
                merged
            } else {
                let input_list = split_active_list(&input.active_views());
                let mut merged = split_active_list(&base.active_views());
                merge_strings_without_duplicates(&input_list, &mut merged);
                merged
            };
            self.o.merged_config.set_active_views(&join(&merged, ','))?;
        }
        Ok(())
    }

    fn process_viewing_rules(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
    ) -> Result<()> {
        let mut merged = ViewingRules::new();
        let first_rules = first.viewing_rules();
        let second_rules = second.viewing_rules();

        for i in 0..first_rules.num_entries() {
            let name = first_rules.name(i).unwrap_or("").to_string();
            match second_rules.index_for_rule(&name) {
                Ok(idx) => {
                    if !viewing_rules_are_equal(first_rules, i, second_rules, idx) {
                        let n = merged.num_entries();
                        if prefer_second {
                            // Take the rule from the second config.
                            copy_viewing_rule(second_rules, idx, n, &mut merged);
                        } else {
                            // Found, but not overriding. Take the rule from the first config.
                            copy_viewing_rule(first_rules, i, n, &mut merged);
                        }
                        self.conflict(&format!("viewing_rules: {name}"))?;
                    }
                }
                Err(_) => {
                    // Not found in the second rules. Take the rule from the first config.
                    let n = merged.num_entries();
                    copy_viewing_rule(first_rules, i, n, &mut merged);
                }
            }
        }

        // Add the remaining rules.
        add_unique_viewing_rules(second_rules, &mut merged);

        self.o.merged_config.set_viewing_rules(&merged);
        Ok(())
    }

    fn prefer(&mut self, prefer_input: bool) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        let input_first = self.o.params.is_input_first();

        // The error_on_conflict option applies to shared_views, displays/views,
        // virtual_display, view_transforms, default_view_transform, and viewing_rules.

        // Clear displays and shared_views from the merged config.
        self.o.merged_config.clear_displays();
        self.o.merged_config.clear_shared_views();

        // Merge displays and views. The order is important: shared_views, and then displays.
        let (first, second, prefer_second) = if input_first {
            (input, base, !prefer_input)
        } else {
            (base, input, prefer_input)
        };
        self.process_shared_views(first, second, prefer_second)?;
        self.process_displays(first, second, prefer_second)?;

        // Merge virtual_display.
        self.o.merged_config.clear_virtual_display();
        self.process_virtual_display(first, second, prefer_second)?;

        // Merge active_displays and active_views.
        self.process_active_lists()?;

        // Merge viewing_rules.
        self.process_viewing_rules(first, second, prefer_second)
    }
}

impl SectionMerger for DisplayViewMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "Display/Views"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.prefer(true)
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.prefer(false)
    }
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.o.input_config;
        let params = self.o.params;

        // Clear displays and shared_views from the merged config.
        self.o.merged_config.clear_displays();
        self.o.merged_config.clear_shared_views();

        // Merge displays and views.
        self.add_unique_shared_views(input)?;
        self.add_unique_displays(input)?;

        // Merge virtual_display.
        self.o.merged_config.clear_virtual_display();
        self.add_unique_virtual_views(input)?;

        // Merge active_displays.
        let active_displays = params.active_displays();
        if !active_displays.is_empty() {
            self.o.merged_config.set_active_displays(&active_displays)?;
        } else {
            self.o
                .merged_config
                .set_active_displays(&input.active_displays())?;
        }

        // Merge active_views.
        let active_views = params.active_views();
        if !active_views.is_empty() {
            self.o.merged_config.set_active_views(&active_views)?;
        } else {
            self.o
                .merged_config
                .set_active_views(&input.active_views())?;
        }

        // Merge viewing_rules.
        self.o
            .merged_config
            .set_viewing_rules(input.viewing_rules());
        Ok(())
    }
    fn handle_base_only(&mut self) -> Result<()> {
        // Process the overrides only since the merged config is initialized to the base config.
        let params = self.o.params;
        let active_displays = params.active_displays();
        if !active_displays.is_empty() {
            self.o.merged_config.set_active_displays(&active_displays)?;
        }
        let active_views = params.active_views();
        if !active_views.is_empty() {
            self.o.merged_config.set_active_views(&active_views)?;
        }
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Remove shared_views: add the shared views of the base config NOT present in the
        // input config.
        self.o.merged_config.clear_shared_views();
        for v in 0..base.num_views_by_type(ViewType::Shared, "") {
            let view = base.view_by_type(ViewType::Shared, "", v).to_string();
            if !view.is_empty() && !input.has_view("", &view) {
                self.add_shared_view_from(base, &view)?;
            }
        }

        // Remove views from displays.
        self.o.merged_config.clear_displays();
        for i in 0..base.num_displays_all() {
            let disp = base.display_all(i).to_string();

            // Display-defined views.
            for v in 0..base.num_views_by_type(ViewType::DisplayDefined, &disp) {
                let view = base
                    .view_by_type(ViewType::DisplayDefined, &disp, v)
                    .to_string();
                if !view.is_empty() && !input.has_view(&disp, &view) {
                    self.add_display_view_from(base, &disp, &view)?;
                }
            }

            // Shared views.
            for v in 0..base.num_views_by_type(ViewType::Shared, &disp) {
                let view = base.view_by_type(ViewType::Shared, &disp, v).to_string();
                if !view.is_empty() && !input.has_view(&disp, &view) {
                    self.o.merged_config.add_display_shared_view(&disp, &view)?;
                }
            }
        }

        // Remove views from virtual_display.
        self.o.merged_config.clear_virtual_display();
        for v in 0..base.virtual_display_num_views(ViewType::DisplayDefined) {
            let view = base
                .virtual_display_view(ViewType::DisplayDefined, v)
                .to_string();
            if !view.is_empty() && !input.has_virtual_view(&view) {
                self.add_virtual_view_from(base, &view)?;
            }
        }
        for v in 0..base.virtual_display_num_views(ViewType::Shared) {
            let view = base.virtual_display_view(ViewType::Shared, v).to_string();
            if !view.is_empty() && !input.has_virtual_view(&view) {
                self.o
                    .merged_config
                    .add_virtual_display_shared_view(&view)?;
            }
        }

        // Remove from active_displays.
        let input_displays = split_active_list(&input.active_displays());
        let mut merged_displays = split_active_list(&base.active_displays());
        for d in &input_displays {
            remove(&mut merged_displays, d);
        }
        self.o
            .merged_config
            .set_active_displays(&join(&merged_displays, ','))?;

        // Remove from active_views.
        let input_views = split_active_list(&input.active_views());
        let mut merged_views = split_active_list(&base.active_views());
        for v in &input_views {
            remove(&mut merged_views, v);
        }
        self.o
            .merged_config
            .set_active_views(&join(&merged_views, ','))?;

        // Handle viewing_rules: keep the base rules that aren't in the input.
        let mut merged_rules = ViewingRules::new();
        let input_rules = input.viewing_rules();
        let base_rules = base.viewing_rules();
        for i in 0..base_rules.num_entries() {
            let name = base_rules.name(i).unwrap_or("");
            if input_rules.index_for_rule(name).is_err() {
                let n = merged_rules.num_entries();
                copy_viewing_rule(base_rules, i, n, &mut merged_rules);
            }
        }
        self.o.merged_config.set_viewing_rules(&merged_rules);
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// ViewTransformsMerger

fn view_transforms_are_equal(first: &Config, second: &Config, name: &str) -> bool {
    let (vt1, vt2) = match (first.view_transform(name), second.view_transform(name)) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    // Both configs have a view transform by this name, now check the parts. Note: Not
    // checking family or description since it is not a functional difference.

    // Upstream OCIO note: Check categories.

    if vt1.reference_space_type() != vt2.reference_space_type() {
        return false;
    }

    for dir in [
        ViewTransformDirection::ToReference,
        ViewTransformDirection::FromReference,
    ] {
        match (vt1.transform(dir), vt2.transform(dir)) {
            (None, None) => {}
            (Some(t1), Some(t2)) => {
                // NB: This is a fast comparison that does not load file transforms.
                if format_transform(t1) != format_transform(t2) {
                    return false;
                }
            }
            // One of them has a transform but the other does not.
            _ => return false,
        }
    }
    true
}

/// The view_transforms section (and default_view_transform).
pub(crate) struct ViewTransformsMerger<'o, 'a> {
    o: &'o mut MergeHandlerOptions<'a>,
    strategy: MergeStrategies,
    input_to_base_scene: Option<Transform>,
    input_to_base_display: Option<Transform>,
}

impl<'o, 'a> ViewTransformsMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let strategy = section_strategy(o.params.view_transforms(), o.params);
        let mut m = Self {
            o,
            strategy,
            input_to_base_scene: None,
            input_to_base_display: None,
        };
        if m.o.params.is_adjust_input_reference_space() {
            let (scene, display) =
                initialize_ref_space_converters(m.o.base_config, m.o.input_config)?;
            m.input_to_base_scene = Some(scene);
            m.input_to_base_display = Some(display);
        }
        Ok(m)
    }

    fn add_view_transform(&mut self, cfg: &Config, name: &str, is_input: bool) -> Result<()> {
        let vt = match cfg.view_transform(name) {
            Some(vt) => vt,
            None => return Ok(()),
        };
        if !is_input || !self.o.params.is_adjust_input_reference_space() {
            self.o.merged_config.add_view_transform(vt)
        } else {
            // Add the reference space adapter transforms.
            let (scene, display) = match (&self.input_to_base_scene, &self.input_to_base_display) {
                (Some(s), Some(d)) => (s, d),
                _ => return Err(Error::msg(
                    "Could not update view transform reference spaces, converter transforms were not initialized.",
                )),
            };
            let mut e = vt.clone();
            update_reference_view(&mut e, scene, display);
            self.o.merged_config.add_view_transform(&e)
        }
    }

    fn add_unique_view_transforms(&mut self, cfg: &Config, is_input: bool) -> Result<()> {
        for i in 0..cfg.num_view_transforms() {
            let name = cfg.view_transform_name_by_index(i);
            // Take the view transform from the config if it does not exist in the merged config.
            if self.o.merged_config.view_transform(name).is_none() {
                self.add_view_transform(cfg, name, is_input)?;
            }
        }
        Ok(())
    }

    fn process_view_transforms(
        &mut self,
        first: &Config,
        second: &Config,
        prefer_second: bool,
        second_is_input: bool,
    ) -> Result<()> {
        for i in 0..first.num_view_transforms() {
            let name = first.view_transform_name_by_index(i);
            if name.is_empty() {
                continue;
            }
            let vt2 = second.view_transform(name);
            if vt2.is_some() && !view_transforms_are_equal(first, second, name) {
                notify(
                    &format!(
                        "The Input config contains a value that would override the Base config: view_transforms: {name}"
                    ),
                    self.o.params.is_error_on_conflict(),
                )?;
            }
            if vt2.is_some() && prefer_second {
                self.add_view_transform(second, name, second_is_input)?;
            } else {
                self.add_view_transform(first, name, !second_is_input)?;
            }
        }

        // Add the remaining unique view transforms.
        self.add_unique_view_transforms(second, second_is_input)
    }

    fn default_view_transform_conflict(&self) -> Result<()> {
        let base_name = self.o.base_config.default_view_transform_name();
        let input_name = self.o.input_config.default_view_transform_name();
        if !compare(base_name, input_name) {
            notify(
                &format!(
                    "The Input config contains a value that would override the Base config: default_view_transform: {input_name}"
                ),
                self.o.params.is_error_on_conflict(),
            )?;
        }
        Ok(())
    }
}

impl SectionMerger for ViewTransformsMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "View Transforms"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Merge view_transforms.
        self.o.merged_config.clear_view_transforms();
        if self.o.params.is_input_first() {
            self.process_view_transforms(input, base, false, false)?;
        } else {
            self.process_view_transforms(base, input, true, true)?;
        }

        // Merge default_view_transform.
        self.default_view_transform_conflict()?;
        // If the input config does not specify a default, keep the one from the base.
        let input_name = input.default_view_transform_name();
        if !input_name.is_empty() {
            self.o
                .merged_config
                .set_default_view_transform_name(input_name);
        }
        Ok(())
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Merge view_transforms.
        self.o.merged_config.clear_view_transforms();
        if self.o.params.is_input_first() {
            self.process_view_transforms(input, base, true, false)?;
        } else {
            self.process_view_transforms(base, input, false, true)?;
        }

        // Merge default_view_transform.
        self.default_view_transform_conflict()?;
        // Only use the input if the base is missing.
        if base.default_view_transform_name().is_empty() {
            self.o
                .merged_config
                .set_default_view_transform_name(input.default_view_transform_name());
        }
        Ok(())
    }
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.o.input_config;
        // Merge view_transforms.
        self.o.merged_config.clear_view_transforms();
        self.add_unique_view_transforms(input, true)?;
        // Merge default_view_transform.
        self.o
            .merged_config
            .set_default_view_transform_name(input.default_view_transform_name());
        Ok(())
    }
    fn handle_base_only(&mut self) -> Result<()> {
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Remove from view_transforms: add the view transforms of the base config NOT present
        // in the input config.
        self.o.merged_config.clear_view_transforms();
        for i in 0..base.num_view_transforms() {
            let name = base.view_transform_name_by_index(i);
            if input.view_transform(name).is_none() {
                if let Some(vt) = base.view_transform(name) {
                    self.o.merged_config.add_view_transform(vt)?;
                }
            }
        }

        // Handle default_view_transform: leave the base alone unless it identified a view
        // transform that was removed.
        let base_name = base.default_view_transform_name();
        if self.o.merged_config.view_transform(base_name).is_none() {
            // Set to empty string, the first view transform will be used by default.
            self.o.merged_config.set_default_view_transform_name("");
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// LooksMerger

declare_merger!(
    /// The looks section.
    LooksMerger
);

impl<'o, 'a> LooksMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        let strategy = section_strategy(o.params.looks(), o.params);
        Self { o, strategy }
    }

    // Add all the looks of `cfg`, replacing the ones with the same name.
    fn add_all(&mut self, cfg: &Config) -> Result<()> {
        for i in 0..cfg.num_looks() {
            if let Some(look) = cfg.look(cfg.look_name_by_index(i)) {
                self.o.merged_config.add_look(look)?;
            }
        }
        Ok(())
    }

    // Add the looks of `cfg` that do not exist in the merged config.
    fn add_missing(&mut self, cfg: &Config) -> Result<()> {
        for i in 0..cfg.num_looks() {
            let name = cfg.look_name_by_index(i);
            if self.o.merged_config.look(name).is_none() {
                if let Some(look) = cfg.look(name) {
                    self.o.merged_config.add_look(look)?;
                }
            }
        }
        Ok(())
    }
}

impl SectionMerger for LooksMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "Looks"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        self.o.merged_config.clear_looks();
        if self.o.params.is_input_first() {
            // Add the looks of the input config, then the missing ones of the base config.
            self.add_all(input)?;
            self.add_missing(base)
        } else {
            // Add the looks of the base config, then the input ones (overwriting the looks
            // having the same name).
            self.add_all(base)?;
            self.add_all(input)
        }
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        self.o.merged_config.clear_looks();
        if self.o.params.is_input_first() {
            // Add the looks of the input config, then the base ones (overwriting the looks
            // having the same name).
            self.add_all(input)?;
            self.add_all(base)
        } else {
            // Add the looks of the base config, then the missing ones of the input config.
            self.add_all(base)?;
            self.add_missing(input)
        }
    }
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.o.input_config;
        self.o.merged_config.clear_looks();
        self.add_all(input)
    }
    fn handle_base_only(&mut self) -> Result<()> {
        // Supported, but nothing to do.
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        self.o.merged_config.clear_looks();
        // Add the looks of the base config that do not exist in the input config.
        for i in 0..base.num_looks() {
            let name = base.look_name_by_index(i);
            if input.look(name).is_none() {
                if let Some(look) = base.look(name) {
                    self.o.merged_config.add_look(look)?;
                }
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// ColorspacesMerger

fn has_search_path(cfg: &Config, path: &str) -> bool {
    (0..cfg.num_search_paths()).any(|i| compare(cfg.search_path_by_index(i), path))
}

fn clean_up_inactive_list(config: &mut Config) {
    let original = split_active_list(config.inactive_color_spaces());
    let mut valid = Vec::new();
    for item in &original {
        let name = trim(item);
        if let Some(cs) = config.get_color_space(name) {
            // Don't want aliases in the inactive list.
            if compare(cs.name(), name) {
                valid.push(name.to_string());
            }
        } else if let Some(nt) = config.get_named_transform(name) {
            if compare(nt.name(), name) {
                valid.push(name.to_string());
            }
        }
    }
    config.set_inactive_color_spaces(&join(&valid, ','));
}

fn replace_separator(s: &str, in_sep: char, out_sep: char) -> String {
    s.chars()
        .map(|c| if c == in_sep { out_sep } else { c })
        .collect()
}

/// Update the family of an item: separator update and family prefix
/// (`updateFamily`, same implementation for color spaces and named
/// transforms, both driven by the color space strategy as in OCIO).
fn update_family(
    params: &ConfigMergingParameters,
    base: &Config,
    input: &Config,
    merged_sep: char,
    family: &mut String,
    from_base: bool,
) {
    // Note that if a prefix is present, it is always added, even if the item did not have a
    // family.
    let mut updated_prefix = String::new();
    match params.colorspaces() {
        MergeStrategies::PreferInput => {
            if from_base {
                // If the item is from the base config, need to update its family separator.
                if !family.is_empty() {
                    *family = replace_separator(family, base.family_separator(), merged_sep);
                }
                // Note: The family prefix argument must always use the default slash separator.
                updated_prefix = replace_separator(params.base_family_prefix(), '/', merged_sep);
            } else {
                updated_prefix = replace_separator(params.input_family_prefix(), '/', merged_sep);
            }
        }
        MergeStrategies::PreferBase => {
            if from_base {
                updated_prefix = replace_separator(params.base_family_prefix(), '/', merged_sep);
            } else {
                // If the item is from the input config, need to update its family separator.
                if !family.is_empty() {
                    *family = replace_separator(family, input.family_separator(), merged_sep);
                }
                updated_prefix = replace_separator(params.input_family_prefix(), '/', merged_sep);
            }
        }
        _ => {}
    }
    // Note that the prefix should end with a separator, if desired. Not adding one here.
    *family = format!("{updated_prefix}{family}");
}

fn all_color_space_names(config: &Config) -> Vec<String> {
    let n =
        config.num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    (0..n)
        .map(|i| {
            config
                .color_space_name_by_index_filtered(
                    SearchReferenceSpaceType::All,
                    ColorSpaceVisibility::All,
                    i,
                )
                .to_string()
        })
        .collect()
}

fn all_named_transform_names(config: &Config) -> Vec<String> {
    let n = config.num_named_transforms_filtered(NamedTransformVisibility::All);
    (0..n)
        .map(|i| {
            config
                .named_transform_name_by_index_filtered(NamedTransformVisibility::All, i)
                .to_string()
        })
        .collect()
}

/// The color spaces section: colorspaces, display_colorspaces, environment,
/// search_path, family_separator and inactive_colorspaces.
pub(crate) struct ColorspacesMerger<'o, 'a> {
    o: &'o mut MergeHandlerOptions<'a>,
    strategy: MergeStrategies,
    input_to_base_scene: Option<Transform>,
    input_to_base_display: Option<Transform>,
}

impl<'o, 'a> ColorspacesMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Result<Self> {
        let strategy = section_strategy(o.params.colorspaces(), o.params);
        let mut m = Self {
            o,
            strategy,
            input_to_base_scene: None,
            input_to_base_display: None,
        };
        if m.o.params.is_adjust_input_reference_space() {
            let (scene, display) =
                initialize_ref_space_converters(m.o.base_config, m.o.input_config)?;
            m.input_to_base_scene = Some(scene);
            m.input_to_base_display = Some(display);
        }
        Ok(m)
    }

    fn error_on_conflict(&self) -> bool {
        self.o.params.is_error_on_conflict()
    }

    fn process_search_paths(&mut self) {
        let base = self.o.base_config;
        let input = self.o.input_config;
        let params = self.o.params;

        let search_paths = params.search_path();
        if !search_paths.is_empty() {
            // Use the override.
            self.o.merged_config.set_search_path(&search_paths);
            return;
        }

        // Ignoring is_input_first for the ordering of search paths because it should really be
        // driven by the strategy. E.g., if both base and input have a "luts" directory, want to
        // be looking in the right one.

        // Upstream OCIO note: Absolute paths should be set up for input since the working dir is
        // from the base config.

        if params.colorspaces() == MergeStrategies::PreferInput {
            self.o.merged_config.clear_search_paths();
            // Add all from the input config.
            for i in 0..input.num_search_paths() {
                self.o
                    .merged_config
                    .add_search_path(input.search_path_by_index(i));
            }
            // Only add the new ones from the base config.
            for i in 0..base.num_search_paths() {
                let p = base.search_path_by_index(i);
                if !has_search_path(input, p) {
                    self.o.merged_config.add_search_path(p);
                }
            }
        } else {
            // NB: The merged config is initialized with the content of the base config.
            for i in 0..input.num_search_paths() {
                let p = input.search_path_by_index(i);
                if !has_search_path(base, p) {
                    self.o.merged_config.add_search_path(p);
                }
            }
        }
    }

    fn update_family(&self, family: &mut String, from_base: bool) {
        update_family(
            self.o.params,
            self.o.base_config,
            self.o.input_config,
            self.o.merged_config.family_separator(),
            family,
            from_base,
        );
    }

    fn attempt_to_add_alias(
        &self,
        merge_config: &Config,
        dupe_cs: &mut ColorSpace,
        input_cs: &ColorSpace,
        alias_name: &str,
    ) -> Result<()> {
        // It is assumed that the base and input configs start out in a legal state, however,
        // when adding anything from one config to another, must always check that it doesn't
        // conflict with anything.

        // It is assumed that the strategy is prefer_base when this function is called.

        // It's OK if the alias is used in the duplicate color space itself.
        if compare(dupe_cs.name(), alias_name) || dupe_cs.has_alias(alias_name) {
            // It's already present, no need to add anything.
            return Ok(());
        }

        // Check if the alias is already a name that is used in the config.
        if let Some(conflicting) = merge_config.get_color_space(alias_name) {
            // The conflict could be the name of a color space, an alias, or a role. But it
            // doesn't matter, the strategy is prefer base so don't want to remove this conflict
            // from the base config to accommodate adding an alias from the input config.
            return notify(
                &format!(
                    "Input color space '{}' is a duplicate of base color space '{}' but was unable to add alias '{}' since it conflicts with base color space '{}'.",
                    input_cs.name(),
                    dupe_cs.name(),
                    alias_name,
                    conflicting.name()
                ),
                self.error_on_conflict(),
            );
        }

        // No conflicts encountered, it's ok to add the alias.
        dupe_cs.add_alias(alias_name);
        Ok(())
    }

    fn handle_avoid_duplicates_option(
        &self,
        fingerprints: &ColorSpaceFingerprints,
        e_base: &mut Config,
        input_config: &Config,
        input_cs: &mut ColorSpace,
    ) -> Result<bool> {
        let not_duplicate = true;

        if !self.o.params.is_avoid_duplicates() {
            return Ok(not_duplicate);
        }
        // If a color space has the allow-duplicate category, don't check if it's a duplicate.
        if input_cs.has_category("allow-duplicate") {
            return Ok(not_duplicate);
        }

        // Note: The search for duplicate color spaces only searches for color spaces with the
        // same reference space type (i.e., scene or display), so it won't remove spaces that
        // are otherwise equivalent (e.g., an sRGB transform).
        //
        // However, some configs may intentionally have duplicate color spaces (e.g., aliases
        // from v1 configs). Since this search is only done using spaces from the input config,
        // those duplicates in the base won't be removed. But if the input config contains
        // duplicates, those will be condensed into one space containing aliases for all of the
        // names of the duplicates.
        //
        // Note: By design, inactive color spaces are included in the search for equivalents.
        let duplicate_in_base = find_equivalent_colorspace(fingerprints, input_config, input_cs);
        if duplicate_in_base.is_empty() {
            return Ok(not_duplicate);
        }

        match self.o.params.colorspaces() {
            MergeStrategies::PreferInput => {
                // Add the name and aliases of the duplicate color space to the input color
                // space. They should not conflict with the base config (where they originated),
                // but may conflict with other color spaces of the input config, handled later
                // when merging the color spaces.
                //
                // If the base config has more than one color space equivalent to the input
                // color space, only the first is replaced.
                if let Some(dupe_cs) = e_base.get_color_space(&duplicate_in_base) {
                    // Note that add_alias checks the argument and won't add it if it matches
                    // the name of the color space or one of the existing aliases.
                    input_cs.add_alias(dupe_cs.name());
                    for a in dupe_cs.aliases() {
                        input_cs.add_alias(a);
                    }
                    // Upstream OCIO note: This should be controlled by a merge option.
                    for c in dupe_cs.categories() {
                        input_cs.add_category(c);
                    }

                    // Merging the input color space would now give misleading notifications
                    // about conflicts from the newly added aliases, so remove the duplicate. If
                    // more than one input color space duplicates a given base color space, the
                    // duplicate has been removed and is now an alias, so get the name of the
                    // color space having that alias.
                    let current_name = e_base.canonical_name(&duplicate_in_base).to_string();
                    e_base.remove_color_space(&current_name);

                    notify(
                        &format!(
                            "Equivalent input color space '{}' replaces '{}' in the base config, preserving aliases.",
                            input_cs.name(),
                            current_name
                        ),
                        self.error_on_conflict(),
                    )?;

                    // Still want the caller to proceed merging the input color space.
                    return Ok(true);
                }
            }
            MergeStrategies::PreferBase => {
                // Don't add the input color space, but add its name and aliases to the
                // duplicate.
                //
                // If the input config has more than one color space equivalent to a base color
                // space, they are all condensed into that first equivalent base color space.
                if let Some(cs) = e_base.get_color_space(&duplicate_in_base) {
                    let mut e_cs = cs.clone();

                    let input_name = input_cs.name().to_string();
                    self.attempt_to_add_alias(e_base, &mut e_cs, input_cs, &input_name)?;
                    for a in input_cs.aliases().to_vec() {
                        self.attempt_to_add_alias(e_base, &mut e_cs, input_cs, &a)?;
                    }
                    for c in input_cs.categories() {
                        e_cs.add_category(c);
                    }

                    // Replace the color space in the merge config (this preserves its order).
                    e_base.add_color_space(&e_cs)?;

                    notify(
                        &format!(
                            "Equivalent base color space '{}' overrides '{}' in the input config, preserving aliases.",
                            duplicate_in_base,
                            input_cs.name()
                        ),
                        self.error_on_conflict(),
                    )?;

                    // The base color space is edited here, don't want to add the input one.
                    return Ok(false);
                }
            }
            _ => {}
        }
        Ok(not_duplicate)
    }

    fn color_space_may_be_merged(
        &self,
        merge_config: &Config,
        input_cs: &ColorSpace,
    ) -> Result<bool> {
        // This should only be called on color spaces from the input config.

        // NB: This routine assumes all named transforms have been removed from the merge
        // config. Color spaces have precedence.

        let name = input_cs.name();

        // This will compare the name against roles, color space names, and aliases.
        let existing = match merge_config.get_color_space(name) {
            // No name conflicts, go ahead and add it.
            None => return Ok(true),
            Some(cs) => cs,
        };

        // Something has this name, figure out what it is.

        // Does it have the same name as a role?
        if merge_config.has_role(name) {
            // Don't merge it if it would override a role.
            notify(
                &format!("Color space '{name}' was not merged as it's identical to a role name."),
                self.error_on_conflict(),
            )?;
            return Ok(false);
        }

        let strategy = self.o.params.colorspaces();
        let prefer_input =
            strategy == MergeStrategies::PreferInput || strategy == MergeStrategies::InputOnly;

        if compare(existing.name(), name) {
            // The name matches a color space name in the merge config. Whether to allow the
            // merge is based on the merge strategy.
            if prefer_input {
                notify(
                    &format!("Color space '{name}' will replace a color space in the base config."),
                    self.error_on_conflict(),
                )?;
                Ok(true)
            } else {
                // Don't merge since it would replace a color space from the base config.
                notify(
                    &format!(
                        "Color space '{name}' was not merged as it's already present in the base config."
                    ),
                    self.error_on_conflict(),
                )?;
                Ok(false)
            }
        } else if prefer_input {
            // The name conflicts with an alias of another color space.
            notify(
                &format!(
                    "The name of merged color space '{}' has a conflict with an alias in color space '{}'.",
                    name,
                    existing.name()
                ),
                self.error_on_conflict(),
            )?;
            Ok(true)
        } else {
            // Don't merge it if it would replace an alias from the base config.
            notify(
                &format!(
                    "Color space '{}' was not merged as it conflicts with an alias in color space '{}'.",
                    name,
                    existing.name()
                ),
                self.error_on_conflict(),
            )?;
            Ok(false)
        }
    }

    fn merge_color_space(
        &self,
        merge_config: &mut Config,
        e_input_cs: &mut ColorSpace,
        added_input_color_spaces: &mut Vec<String>,
    ) -> Result<()> {
        // NB: This routine assumes all named transforms have been removed from the merge
        // config. Color spaces have precedence.

        let name = e_input_cs.name().to_string();

        // Check if the merge config already has a color space with the same name.
        if let Some(original) = merge_config.get_color_space(&name) {
            // The color space which gets discarded and the color space being added may not have
            // the same reference space type. This is currently allowed but log a warning.
            if e_input_cs.reference_space_type() != original.reference_space_type() {
                notify(
                    &format!(
                        "Merged color space '{name}' has a different reference space type than the color space it's replacing."
                    ),
                    false,
                )?;
            }

            // If there is a color space with this name in the existing config, remove it (and
            // any aliases it may contain). This is the case when the strategy calls for
            // replacing an existing color space. If the name matched an alias rather than a
            // color space name, this does nothing (and the alias is handled below).
            merge_config.remove_color_space(&name);
        }

        // Handle conflicts of the name with aliases of other color spaces.
        if let Some(existing) = merge_config.get_color_space(&name) {
            // Verify that the name is actually an alias rather than some other conflict.
            // (Should never happen.)
            if !existing.has_alias(&name) {
                return Err(Error::msg(format!(
                    "Problem merging color space: '{name}'."
                )));
            }

            // Remove the alias from that existing color space. Note that this conflict was
            // detected and allowed in color_space_may_be_merged based on the merge strategy.
            let mut e_existing = existing.clone();
            e_existing.remove_alias(&name);
            merge_config.add_color_space(&e_existing)?;
        }

        let strategy = self.o.params.colorspaces();
        let prefer_input =
            strategy == MergeStrategies::PreferInput || strategy == MergeStrategies::InputOnly;

        // Handle conflicts of the aliases with other color spaces or aliases.

        // First initialize the list of names, since the color space is edited within the loop.
        let alias_names: Vec<String> = e_input_cs.aliases().to_vec();

        for alias in &alias_names {
            let conflicting = match merge_config.get_color_space(alias) {
                Some(c) => c.clone(),
                None => continue,
            };
            let msg;
            if compare(conflicting.name(), alias) {
                // The alias conflicts with the name of an existing color space.
                msg = format!(
                    "Merged color space '{}' has an alias '{}' that conflicts with color space '{}'.",
                    name,
                    alias,
                    conflicting.name()
                );
                if prefer_input {
                    // Remove that base color space.
                    merge_config.remove_color_space(conflicting.name());
                } else {
                    // Remove the alias from the input color space.
                    e_input_cs.remove_alias(alias);
                }
            } else if conflicting.has_alias(alias) {
                // The alias conflicts with an alias of the conflicting color space.
                msg = format!(
                    "Merged color space '{}' has a conflict with alias '{}' in color space '{}'.",
                    name,
                    alias,
                    conflicting.name()
                );
                if prefer_input {
                    // Remove the alias from that base color space.
                    let mut e_conflicting = conflicting.clone();
                    e_conflicting.remove_alias(alias);
                    merge_config.add_color_space(&e_conflicting)?;
                } else {
                    // Remove the alias from the input color space.
                    e_input_cs.remove_alias(alias);
                }
            } else if merge_config.has_role(alias) {
                msg = format!(
                    "Merged color space '{name}' has an alias '{alias}' that conflicts with a role."
                );
                // Remove the alias from the input color space.
                e_input_cs.remove_alias(alias);
            } else {
                // (Should never happen.)
                return Err(Error::msg(format!(
                    "Problem merging color space: '{name}' due to its aliases."
                )));
            }
            notify(&msg, self.error_on_conflict())?;
        }

        // Add the color space. This fails if a problem is found (but all name conflicts should
        // have been handled already).
        merge_config.add_color_space(e_input_cs)?;

        // Keep a record that this input color space was added to allow reordering later.
        added_input_color_spaces.push(name);

        // Upstream OCIO note: When color spaces or aliases are removed above, it's possible it
        // could break some other part of the config that referenced them.
        Ok(())
    }

    fn add_color_spaces(&mut self) -> Result<()> {
        // Delete all the named transforms, color spaces take precedence, so don't want them
        // interfering with merges by causing name conflicts.

        // NB: This is only intended to be called for the prefer_input and prefer_base
        // strategies.

        self.o.merged_config.clear_named_transforms();

        // Make a temp copy to merge the input color spaces into (will reorder them later).
        let mut merge_config = self.o.merged_config.create_editable_copy();
        merge_config.clear_named_transforms();

        let base = self.o.base_config;
        let input = self.o.input_config;
        let params = self.o.params;

        // Loop over all active and inactive color spaces of all reference types in the input
        // config. Merge them into the temp config (which already contains the base color
        // spaces).
        let mut added: Vec<String> = Vec::new();

        let fingerprints = if params.is_avoid_duplicates() {
            initialize_color_space_fingerprints(base)
        } else {
            ColorSpaceFingerprints::default()
        };

        for name in all_color_space_names(input) {
            let cs = match input.get_color_space(&name) {
                Some(cs) => cs,
                None => continue,
            };
            let mut e_cs = cs.clone();

            if params.is_adjust_input_reference_space() {
                let conv = if e_cs.reference_space_type() == ReferenceSpaceType::Display {
                    &self.input_to_base_display
                } else {
                    &self.input_to_base_scene
                };
                match conv {
                    Some(t) => update_reference_colorspace(&mut e_cs, t),
                    None => {
                        return Err(Error::msg(
                            "Could not update reference space, converter transform was not initialized.",
                        ))
                    }
                }
            }

            // Doing this against the merge config rather than the base config so that the most
            // recent state of any aliases that get added or color spaces that are removed are
            // considered by the duplicate consolidation process.
            let not_duplicate = self.handle_avoid_duplicates_option(
                &fingerprints,
                &mut merge_config,
                input,
                &mut e_cs,
            )?;

            if not_duplicate && self.color_space_may_be_merged(&merge_config, &e_cs)? {
                // NB: This may change existing color spaces of the merge config to resolve name
                // conflicts.
                self.merge_color_space(&mut merge_config, &mut e_cs, &mut added)?;
            }
        }

        self.o.merged_config.clear_color_spaces();

        // Add the color spaces to the real merged config.

        if params.is_input_first() {
            // Add color spaces from the input config.
            for name in &added {
                if let Some(cs) = merge_config.get_color_space(name) {
                    let mut e_cs = cs.clone();
                    // Add family prefix.
                    let mut family = e_cs.family().to_string();
                    self.update_family(&mut family, false);
                    e_cs.set_family(&family);
                    self.o.merged_config.add_color_space(&e_cs)?;
                    merge_config.remove_color_space(name);
                }
            }

            // Add color spaces from the base config.
            for name in all_color_space_names(&merge_config) {
                // Note that during the merge process, some of the color spaces from the base
                // config may be replaced if their aliases are edited. This does not change their
                // order in the config.
                if let Some(cs) = merge_config.get_color_space(&name) {
                    let mut e_cs = cs.clone();
                    let mut family = e_cs.family().to_string();
                    self.update_family(&mut family, true);
                    e_cs.set_family(&family);
                    self.o.merged_config.add_color_space(&e_cs)?;
                }
            }
        } else {
            // The color spaces should already be in the correct order. Copy them into the real
            // merged config and add the family prefix.
            for name in all_color_space_names(&merge_config) {
                if let Some(cs) = merge_config.get_color_space(&name) {
                    let mut e_cs = cs.clone();
                    let from_base = !added.iter().any(|a| a == &name);
                    let mut family = e_cs.family().to_string();
                    self.update_family(&mut family, from_base);
                    e_cs.set_family(&family);
                    self.o.merged_config.add_color_space(&e_cs)?;
                }
            }
        }

        // (Not cleaning up the inactive list here, it would remove named transforms, wait until
        // after the named transforms.)

        // Upstream OCIO note: What if the environment contains a color space that was removed?
        Ok(())
    }

    fn set_environment_from_overrides(&mut self) {
        let params = self.o.params;
        self.o.merged_config.clear_environment_vars();
        for i in 0..params.num_environment_vars() {
            self.o.merged_config.add_environment_var(
                params.environment_var(i),
                Some(params.environment_var_value(i)),
            );
        }
    }

    fn merge_inactive_color_spaces(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        let params = self.o.params;

        let inactive = params.inactive_color_spaces();
        if !inactive.is_empty() {
            // Take the inactive color spaces from the overrides.
            self.o.merged_config.set_inactive_color_spaces(inactive);
        } else {
            // Add the inactive color spaces of the input config to the base config.
            let merged = if params.is_input_first() {
                let base_list = split_active_list(base.inactive_color_spaces());
                let mut merged = split_active_list(input.inactive_color_spaces());
                merge_strings_without_duplicates(&base_list, &mut merged);
                merged
            } else {
                let input_list = split_active_list(input.inactive_color_spaces());
                let mut merged = split_active_list(self.o.merged_config.inactive_color_spaces());
                merge_strings_without_duplicates(&input_list, &mut merged);
                merged
            };
            self.o
                .merged_config
                .set_inactive_color_spaces(&join(&merged, ','));
        }
        Ok(())
    }
}

impl SectionMerger for ColorspacesMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "Color Spaces"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        let input = self.o.input_config;

        // Set environment. Since the environment variables are stored inside a map, the keys
        // are ordered alphabetically. Therefore, there is no point to look at input_first.
        if self.o.params.num_environment_vars() > 0 {
            // Take environment variables from overrides.
            self.set_environment_from_overrides();
        } else {
            // Add environment variables from the input config to the base config (overwriting
            // any existing env. variable with the same name).
            for i in 0..input.num_environment_vars() {
                let name = input.environment_var_name_by_index(i);
                self.o
                    .merged_config
                    .add_environment_var(name, Some(input.environment_var_default(name)));
            }
        }

        // Set search_path.
        self.process_search_paths();

        // Set inactive_colorspaces.
        self.merge_inactive_color_spaces()?;

        // Set family_separator.
        self.o
            .merged_config
            .set_family_separator(input.family_separator())?;

        // Merge the color spaces.
        self.add_color_spaces()
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Set environment.
        if self.o.params.num_environment_vars() > 0 {
            // Take environment variables from overrides.
            self.set_environment_from_overrides();
        } else {
            // Take environment variables from the config.
            for i in 0..input.num_environment_vars() {
                let name = input.environment_var_name_by_index(i);
                // If the var's default value is empty, it doesn't exist, so nothing to
                // overwrite.
                let does_not_exist = self
                    .o
                    .merged_config
                    .environment_var_default(name)
                    .is_empty();
                if does_not_exist {
                    self.o
                        .merged_config
                        .add_environment_var(name, Some(input.environment_var_default(name)));
                }
            }
        }

        // Set search_path.
        self.process_search_paths();

        // Set inactive_colorspaces.
        self.merge_inactive_color_spaces()?;

        // Set family_separator.
        self.o
            .merged_config
            .set_family_separator(base.family_separator())?;

        // Merge the color spaces.
        self.add_color_spaces()
    }
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.o.input_config;
        let params = self.o.params;

        // Set environment.
        if params.num_environment_vars() > 0 {
            // Take environment variables from overrides.
            self.set_environment_from_overrides();
        } else {
            // Take environment variables from the config.
            self.o.merged_config.clear_environment_vars();
            for i in 0..input.num_environment_vars() {
                let name = input.environment_var_name_by_index(i);
                self.o
                    .merged_config
                    .add_environment_var(name, Some(input.environment_var_default(name)));
            }
        }

        // Set search_path.
        let search_paths = params.search_path();
        if !search_paths.is_empty() {
            self.o.merged_config.set_search_path(&search_paths);
        } else {
            self.o.merged_config.set_search_path(&input.search_path());
        }

        // Set inactive_colorspaces.
        let inactive = params.inactive_color_spaces();
        if !inactive.is_empty() {
            self.o.merged_config.set_inactive_color_spaces(inactive);
        } else {
            self.o
                .merged_config
                .set_inactive_color_spaces(input.inactive_color_spaces());
        }

        // Set family_separator.
        self.o
            .merged_config
            .set_family_separator(input.family_separator())?;

        // Remove all the color spaces of the base config.
        self.o.merged_config.clear_color_spaces();

        // Avoid any conflicts with the named transforms of the base config.
        self.o.merged_config.clear_named_transforms();

        // Take the color spaces from the input config.
        for name in all_color_space_names(input) {
            if let Some(cs) = input.get_color_space(&name) {
                self.o.merged_config.add_color_space(cs)?;
            }
        }
        Ok(())
    }
    fn handle_base_only(&mut self) -> Result<()> {
        // Process the overrides only since the merged config is initialized to the base config.
        let params = self.o.params;

        // Do search_path override.
        let search_paths = params.search_path();
        if !search_paths.is_empty() {
            self.o.merged_config.set_search_path(&search_paths);
        }

        // Do environment override.
        if params.num_environment_vars() > 0 {
            self.set_environment_from_overrides();
        }

        // Do inactive_colorspaces override.
        let inactive = params.inactive_color_spaces();
        if !inactive.is_empty() {
            self.o.merged_config.set_inactive_color_spaces(inactive);
        }

        // Nothing to do for display_colorspaces and colorspaces as the merged config is
        // initialized to the base config.
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;

        // Handle environment. If an environment variable is used somewhere and got removed,
        // validating the config will return an error.
        for i in 0..input.num_environment_vars() {
            let name = input.environment_var_name_by_index(i);
            let exists = !self
                .o
                .merged_config
                .environment_var_default(name)
                .is_empty();
            if exists {
                self.o.merged_config.add_environment_var(name, None);
            }
        }

        // Handle search_path.
        self.o.merged_config.clear_search_paths();
        let input_search_path = input.search_path();
        for i in 0..base.num_search_paths() {
            let p = base.search_path_by_index(i);
            if !input_search_path.contains(p) {
                self.o.merged_config.add_search_path(p);
            }
        }

        // Handle inactive_colorspaces.
        let base_inactive = split_active_list(base.inactive_color_spaces());
        let input_inactive: Vec<String> = split_active_list(input.inactive_color_spaces())
            .iter()
            .map(|s| trim(s).to_string())
            .collect();
        let mut merged_inactive = Vec::new();
        for name in &base_inactive {
            let t = trim(name);
            if !t.is_empty() && !contain(&input_inactive, t) {
                merged_inactive.push(t.to_string());
            }
        }
        self.o
            .merged_config
            .set_inactive_color_spaces(&join(&merged_inactive, ','));

        // The family_separator never gets removed.

        // Handle display_colorspaces and colorspaces. This could obviously break any other part
        // of the base config that references the removed color space, so it is up to the user
        // to know what they are doing.
        for name in all_color_space_names(input) {
            // Note: The remove does nothing if the color space is not present.
            self.o.merged_config.remove_color_space(&name);
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// NamedTransformsMerger

declare_merger!(
    /// The named_transforms section.
    NamedTransformsMerger
);

impl<'o, 'a> NamedTransformsMerger<'o, 'a> {
    pub fn new(o: &'o mut MergeHandlerOptions<'a>) -> Self {
        let strategy = section_strategy(o.params.named_transforms(), o.params);
        Self { o, strategy }
    }

    fn update_family(&self, family: &mut String, from_base: bool) {
        update_family(
            self.o.params,
            self.o.base_config,
            self.o.input_config,
            self.o.merged_config.family_separator(),
            family,
            from_base,
        );
    }
}

fn named_transform_may_be_merged(
    params: &ConfigMergingParameters,
    merge_config: &Config,
    nt: &NamedTransform,
    from_base: bool,
) -> Result<bool> {
    let name = nt.name();

    // This compares the name against roles, color space names, and aliases.
    let existing_cs = merge_config.get_color_space(name);
    let existing_nt = merge_config.get_named_transform(name);

    if existing_cs.is_none() && existing_nt.is_none() {
        // No name conflicts, go ahead and add it.
        return Ok(true);
    }

    // Something has this name, figure out what it is.

    // Does it have the same name as a role?
    if merge_config.has_role(name) {
        // Don't merge it if it would override a role.
        notify(
            &format!("Named transform '{name}' was not merged as it's identical to a role name."),
            params.is_error_on_conflict(),
        )?;
        return Ok(false);
    }

    if let Some(cs) = existing_cs {
        if compare(cs.name(), name) {
            // The name matches a color space name. Don't merge it, color spaces always have
            // precedence.
            notify(
                    &format!(
                        "Named transform '{name}' was not merged as there's a color space with that name."
                    ),
                    params.is_error_on_conflict(),
                )?;
        } else {
            // The name conflicts with an alias of a color space.
            notify(
                    &format!(
                        "Named transform '{name}' was not merged as there's a color space alias with that name."
                    ),
                    params.is_error_on_conflict(),
                )?;
        }
        return Ok(false);
    }

    if let Some(ent) = existing_nt {
        if from_base {
            // Should not happen if the base config was legal.
            notify(
                    &format!(
                        "Named transform '{name}' was not merged as there's more than one with that name in the base config."
                    ),
                    params.is_error_on_conflict(),
                )?;
            return Ok(false);
        }

        let strategy = params.named_transforms();
        let prefer_input =
            strategy == MergeStrategies::PreferInput || strategy == MergeStrategies::InputOnly;

        // At this point, only dealing with transforms from the input config.

        if compare(ent.name(), name) {
            // The name matches a named transform name. Whether to allow the merge is based
            // on the merge strategy.
            if prefer_input {
                notify(
                        &format!(
                            "Named transform '{name}' will replace a named transform in the base config."
                        ),
                        params.is_error_on_conflict(),
                    )?;
                return Ok(true);
            } else {
                notify(
                        &format!(
                            "Named transform '{name}' was not merged as it's already present in the base config."
                        ),
                        params.is_error_on_conflict(),
                    )?;
                return Ok(false);
            }
        } else if prefer_input {
            // The name conflicts with an alias of another named transform.
            notify(
                    &format!(
                        "The name of merged named transform '{}' has a conflict with an alias in named transform '{}'.",
                        name,
                        ent.name()
                    ),
                    params.is_error_on_conflict(),
                )?;
            return Ok(true);
        } else {
            notify(
                    &format!(
                        "Named transform '{}' was not merged as it conflicts with an alias in named transform '{}'.",
                        name,
                        ent.name()
                    ),
                    params.is_error_on_conflict(),
                )?;
            return Ok(false);
        }
    }
    Ok(false)
}

fn merge_named_transform(
    params: &ConfigMergingParameters,
    merge_config: &mut Config,
    e_nt: &mut NamedTransform,
    from_base: bool,
    added_input_named_transforms: &mut Vec<String>,
) -> Result<()> {
    let name = e_nt.name().to_string();

    if merge_config.get_named_transform(&name).is_some() {
        // If there is a named transform with this name in the existing config, remove it
        // (and any aliases it may contain). If the name matched an alias rather than a
        // named transform name, this does nothing (and the alias is handled below).
        merge_config.remove_named_transform(&name);
    }

    // Handle conflicts of the name with aliases of other named transforms. NB: Would not be
    // here if there is a name conflict with anything other than named transforms since the
    // decision would have been not to merge it.
    if let Some(existing) = merge_config.get_named_transform(&name) {
        // Verify that the name is actually an alias rather than some other conflict.
        // (Should never happen.)
        if !existing.has_alias(&name) {
            return Err(Error::msg(format!(
                "Problem merging named transform: '{name}'."
            )));
        }
        // Remove the alias from that existing named transform.
        let mut e_existing = existing.clone();
        e_existing.remove_alias(&name);
        merge_config.add_named_transform(&e_existing)?;
    }

    let strategy = if from_base {
        MergeStrategies::PreferBase
    } else {
        params.named_transforms()
    };
    let prefer_input =
        strategy == MergeStrategies::PreferInput || strategy == MergeStrategies::InputOnly;

    // Handle conflicts of the aliases with other color spaces, named transforms, etc.

    // First initialize the list of names, since the named transform is edited within the
    // loop.
    let alias_names: Vec<String> = e_nt.aliases().to_vec();
    let source = if from_base { "Base" } else { "Input" };

    for alias in &alias_names {
        // Conflicts with color spaces or roles (always remove this alias).
        let mut msg = String::new();

        if let Some(conflicting) = merge_config.get_color_space(alias).cloned() {
            if compare(conflicting.name(), alias) {
                // The alias conflicts with the name of the conflicting color space.
                msg.push_str(&format!(
                        "Merged {} named transform '{}' has an alias '{}' that conflicts with color space '{}'.",
                        source,
                        name,
                        alias,
                        conflicting.name()
                    ));
                e_nt.remove_alias(alias);
            } else if conflicting.has_alias(alias) {
                // The alias conflicts with an alias of the conflicting color space.
                msg.push_str(&format!(
                        "Merged {} named transform '{}' has a conflict with alias '{}' in color space '{}'.",
                        source,
                        name,
                        alias,
                        conflicting.name()
                    ));
                e_nt.remove_alias(alias);
            } else if merge_config.has_role(alias) {
                msg.push_str(&format!(
                        "Merged {source} named transform '{name}' has an alias '{alias}' that conflicts with a role."
                    ));
                e_nt.remove_alias(alias);
            } else {
                // (Should never happen.)
                return Err(Error::msg(format!(
                    "Problem merging named transform: '{name}' due to its aliases."
                )));
            }
            // Fail if requested, otherwise log a warning.
            notify(&msg, params.is_error_on_conflict())?;
        }

        // Conflicts of the alias with other named transforms.
        if let Some(conflicting) = merge_config.get_named_transform(alias).cloned() {
            if compare(conflicting.name(), alias) {
                // The alias conflicts with the name of an existing named transform.
                msg.push_str(&format!(
                        "Merged {} named transform '{}' has an alias '{}' that conflicts with named transform '{}'.",
                        source,
                        name,
                        alias,
                        conflicting.name()
                    ));
                if prefer_input {
                    // Remove that base named transform.
                    merge_config.remove_named_transform(conflicting.name());
                } else {
                    // Remove the alias from the input named transform.
                    e_nt.remove_alias(alias);
                }
            } else if conflicting.has_alias(alias) {
                // The alias conflicts with an alias of the conflicting named transform.
                msg.push_str(&format!(
                        "Merged {} named transform '{}' has a conflict with alias '{}' in named transform '{}'.",
                        source,
                        name,
                        alias,
                        conflicting.name()
                    ));
                if prefer_input {
                    // Remove the alias from that base named transform.
                    let mut e_conflicting = conflicting.clone();
                    e_conflicting.remove_alias(alias);
                    merge_config.add_named_transform(&e_conflicting)?;
                } else {
                    // Remove the alias from the input named transform.
                    e_nt.remove_alias(alias);
                }
            } else {
                // (Should never happen.)
                return Err(Error::msg(format!(
                    "Problem merging named transform: '{name}' due to its aliases."
                )));
            }
            notify(&msg, params.is_error_on_conflict())?;
        }
    }

    // Add the named transform. This fails if a problem is found (but all name conflicts
    // should have been handled already).
    merge_config.add_named_transform(e_nt)?;

    // Keep a record that this input named transform was added to allow reordering later.
    if !from_base {
        added_input_named_transforms.push(name);
    }
    Ok(())
}

impl NamedTransformsMerger<'_, '_> {
    fn add_named_transforms(&mut self) -> Result<()> {
        // Need to add even the base ones to ensure there are no conflicts with the merged color
        // spaces.

        self.o.merged_config.clear_named_transforms();

        // Make a temp copy to merge the named transforms into (will reorder them later).
        let mut merge_config = self.o.merged_config.create_editable_copy();

        let base = self.o.base_config;
        let input = self.o.input_config;

        let mut added: Vec<String> = Vec::new();

        // Merge from the base config.
        for name in all_named_transform_names(base) {
            let mut e_nt = match base.get_named_transform(&name) {
                Some(nt) => nt.clone(),
                None => continue,
            };
            if named_transform_may_be_merged(self.o.params, &merge_config, &e_nt, true)? {
                merge_named_transform(
                    self.o.params,
                    &mut merge_config,
                    &mut e_nt,
                    true,
                    &mut added,
                )?;
            }
        }

        // Merge from the input config.
        for name in all_named_transform_names(input) {
            let mut e_nt = match input.get_named_transform(&name) {
                Some(nt) => nt.clone(),
                None => continue,
            };
            // Upstream OCIO note: Handle duplicate named transforms.
            if named_transform_may_be_merged(self.o.params, &merge_config, &e_nt, false)? {
                merge_named_transform(
                    self.o.params,
                    &mut merge_config,
                    &mut e_nt,
                    false,
                    &mut added,
                )?;
            }
        }

        self.o.merged_config.clear_named_transforms();

        // Add the named transforms to the real merged config.

        if self.o.params.is_input_first() {
            // Add the named transforms from the input config.
            for name in &added {
                if let Some(nt) = merge_config.get_named_transform(name) {
                    let mut e_nt = nt.clone();
                    let mut family = e_nt.family().to_string();
                    self.update_family(&mut family, false);
                    e_nt.set_family(&family);
                    self.o.merged_config.add_named_transform(&e_nt)?;
                    merge_config.remove_named_transform(name);
                }
            }

            // Add the named transforms from the base config.
            for name in all_named_transform_names(&merge_config) {
                if let Some(nt) = merge_config.get_named_transform(&name) {
                    let mut e_nt = nt.clone();
                    let mut family = e_nt.family().to_string();
                    self.update_family(&mut family, true);
                    e_nt.set_family(&family);
                    self.o.merged_config.add_named_transform(&e_nt)?;
                }
            }
        } else {
            // The named transforms should already be in the correct order. Copy them into the
            // real merged config and add the family prefix.
            for name in all_named_transform_names(&merge_config) {
                if let Some(nt) = merge_config.get_named_transform(&name) {
                    let mut e_nt = nt.clone();
                    let from_base = !added.iter().any(|a| a == &name);
                    let mut family = e_nt.family().to_string();
                    self.update_family(&mut family, from_base);
                    e_nt.set_family(&family);
                    self.o.merged_config.add_named_transform(&e_nt)?;
                }
            }
        }

        // Ensure the inactive_colorspaces doesn't contain anything that was removed.
        clean_up_inactive_list(self.o.merged_config);

        // Upstream OCIO note: What if the environment contains a color space that was removed?
        Ok(())
    }

    // Merge the named transforms of `names` from `cfg` directly into the merged config.
    fn merge_into_merged(&mut self, cfg: &Config, names: &[String], from_base: bool) -> Result<()> {
        for name in names {
            let mut e_nt = match cfg.get_named_transform(name) {
                Some(nt) => nt.clone(),
                None => continue,
            };
            let mut added = Vec::new();
            let params = self.o.params;
            if named_transform_may_be_merged(params, self.o.merged_config, &e_nt, from_base)? {
                merge_named_transform(
                    params,
                    self.o.merged_config,
                    &mut e_nt,
                    from_base,
                    &mut added,
                )?;
            }
        }
        Ok(())
    }
}

impl SectionMerger for NamedTransformsMerger<'_, '_> {
    fn name(&self) -> &'static str {
        "Named Transforms"
    }
    fn strategy(&self) -> MergeStrategies {
        self.strategy
    }
    fn handle_prefer_input(&mut self) -> Result<()> {
        self.add_named_transforms()
    }
    fn handle_prefer_base(&mut self) -> Result<()> {
        self.add_named_transforms()
    }
    fn handle_input_only(&mut self) -> Result<()> {
        let input = self.o.input_config;
        self.o.merged_config.clear_named_transforms();
        // Add the named transforms of the input config.
        self.merge_into_merged(input, &all_named_transform_names(input), false)?;
        // Ensure the inactive_colorspaces doesn't contain anything that was removed.
        clean_up_inactive_list(self.o.merged_config);
        Ok(())
    }
    fn handle_base_only(&mut self) -> Result<()> {
        let base = self.o.base_config;
        self.o.merged_config.clear_named_transforms();
        // Add the named transforms of the base config.
        self.merge_into_merged(base, &all_named_transform_names(base), true)?;
        clean_up_inactive_list(self.o.merged_config);
        Ok(())
    }
    fn handle_remove(&mut self) -> Result<()> {
        let base = self.o.base_config;
        let input = self.o.input_config;
        self.o.merged_config.clear_named_transforms();
        // Add the named transforms of the base config that do not exist in the input config.
        let names: Vec<String> = all_named_transform_names(base)
            .into_iter()
            .filter(|n| input.get_named_transform(n).is_none())
            .collect();
        self.merge_into_merged(base, &names, true)?;
        clean_up_inactive_list(self.o.merged_config);
        Ok(())
    }
}
