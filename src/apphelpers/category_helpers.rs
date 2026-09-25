//! Category and encoding filtering of color spaces and named transforms
//! (port of `apphelpers/CategoryHelpers.cpp`).

use super::color_space_helpers::ColorSpaceInfo;
use crate::config::logging::{log_info, logging_level};
use crate::config::utils::{compare, lower, split, trim};
use crate::config::{ColorSpace, Config, NamedTransform};
use crate::types::{ColorSpaceVisibility, LoggingLevel, SearchReferenceSpaceType};

/// A list of categories (lower case).
pub type Categories = Vec<String>;
/// A list of encodings (lower case).
pub type Encodings = Vec<String>;
/// A list of color space names.
pub type ColorSpaceNames = Vec<String>;
/// A list of menu entries.
pub type Infos = Vec<ColorSpaceInfo>;

pub(crate) type ColorSpaceVec<'a> = Vec<&'a ColorSpace>;
pub(crate) type NamedTransformVec<'a> = Vec<&'a NamedTransform>;

/// Add `elt` if not already present (identity comparison, all the elements
/// come from the same config).
fn add_element<'a, T>(vec: &mut Vec<&'a T>, elt: &'a T) {
    if vec.iter().any(|e| std::ptr::eq(*e, elt)) {
        return;
    }
    vec.push(elt);
}

trait CategoryItem {
    fn item_has_category(&self, category: &str) -> bool;
    fn item_num_categories(&self) -> usize;
    fn item_encoding(&self) -> &str;
    fn item_name(&self) -> &str;
}

impl CategoryItem for ColorSpace {
    fn item_has_category(&self, category: &str) -> bool {
        self.has_category(category)
    }
    fn item_num_categories(&self) -> usize {
        self.num_categories()
    }
    fn item_encoding(&self) -> &str {
        self.encoding()
    }
    fn item_name(&self) -> &str {
        self.name()
    }
}

impl CategoryItem for NamedTransform {
    fn item_has_category(&self, category: &str) -> bool {
        self.has_category(category)
    }
    fn item_num_categories(&self) -> usize {
        self.num_categories()
    }
    fn item_encoding(&self) -> &str {
        self.encoding()
    }
    fn item_name(&self) -> &str {
        self.name()
    }
}

fn has_encoding<T: CategoryItem>(elt: &T, encoding: &str) -> bool {
    compare(encoding, elt.item_encoding())
}

/// The active color spaces of the requested reference space type.
fn active_color_spaces(config: &Config, cs_type: SearchReferenceSpaceType) -> Vec<&ColorSpace> {
    let num = config.num_color_spaces_filtered(cs_type, ColorSpaceVisibility::Active);
    (0..num)
        .filter_map(|idx| {
            config.get_color_space(config.color_space_name_by_index_filtered(
                cs_type,
                ColorSpaceVisibility::Active,
                idx,
            ))
        })
        .collect()
}

/// The active named transforms.
fn active_named_transforms(config: &Config) -> Vec<&NamedTransform> {
    (0..config.num_named_transforms())
        .filter_map(|idx| config.get_named_transform(config.named_transform_name_by_index(idx)))
        .collect()
}

/// Active color spaces having one of the categories and one of the encodings.
pub(crate) fn get_color_spaces_cat_enc<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    treat_no_category_as_any: bool,
    cs_type: SearchReferenceSpaceType,
    categories: &Categories,
    encodings: &Encodings,
) -> ColorSpaceVec<'a> {
    let mut css = Vec::new();
    if include_color_spaces && !categories.is_empty() && !encodings.is_empty() {
        for cs in active_color_spaces(config, cs_type) {
            let ignore_category = treat_no_category_as_any && cs.num_categories() == 0;
            for cat in categories {
                for enc in encodings {
                    if (ignore_category || cs.item_has_category(cat)) && has_encoding(cs, enc) {
                        add_element(&mut css, cs);
                    }
                }
            }
        }
    }
    css
}

/// Active color spaces having one of the categories.
pub(crate) fn get_color_spaces<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    treat_no_category_as_any: bool,
    cs_type: SearchReferenceSpaceType,
    categories: &Categories,
) -> ColorSpaceVec<'a> {
    let mut css = Vec::new();
    if include_color_spaces && !categories.is_empty() {
        for cs in active_color_spaces(config, cs_type) {
            let ignore_category = treat_no_category_as_any && cs.num_categories() == 0;
            for cat in categories {
                if ignore_category || cs.item_has_category(cat) {
                    add_element(&mut css, cs);
                }
            }
        }
    }
    css
}

/// Active color spaces having one of the encodings.
pub(crate) fn get_color_spaces_from_encodings<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    cs_type: SearchReferenceSpaceType,
    encodings: &Encodings,
) -> ColorSpaceVec<'a> {
    let mut css = Vec::new();
    if include_color_spaces && !encodings.is_empty() {
        for cs in active_color_spaces(config, cs_type) {
            for enc in encodings {
                if has_encoding(cs, enc) {
                    add_element(&mut css, cs);
                }
            }
        }
    }
    css
}

/// Active named transforms having one of the categories and one of the
/// encodings.
pub(crate) fn get_named_transforms_cat_enc<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    categories: &Categories,
    encodings: &Encodings,
) -> NamedTransformVec<'a> {
    let mut nts = Vec::new();
    if include_named_transforms && !categories.is_empty() && !encodings.is_empty() {
        for nt in active_named_transforms(config) {
            let ignore_category = treat_no_category_as_any && nt.num_categories() == 0;
            for cat in categories {
                for enc in encodings {
                    if (ignore_category || nt.item_has_category(cat)) && has_encoding(nt, enc) {
                        add_element(&mut nts, nt);
                    }
                }
            }
        }
    }
    nts
}

/// Active named transforms having one of the categories.
pub(crate) fn get_named_transforms<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    categories: &Categories,
) -> NamedTransformVec<'a> {
    let mut nts = Vec::new();
    if include_named_transforms && !categories.is_empty() {
        for nt in active_named_transforms(config) {
            let ignore_category = treat_no_category_as_any && nt.item_num_categories() == 0;
            for cat in categories {
                if ignore_category || nt.item_has_category(cat) {
                    add_element(&mut nts, nt);
                }
            }
        }
    }
    nts
}

/// Active named transforms having one of the encodings.
pub(crate) fn get_named_transforms_from_encodings<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    encodings: &Encodings,
) -> NamedTransformVec<'a> {
    let mut nts = Vec::new();
    if include_named_transforms && !encodings.is_empty() {
        for nt in active_named_transforms(config) {
            for enc in encodings {
                if has_encoding(nt, enc) {
                    add_element(&mut nts, nt);
                }
            }
        }
    }
    nts
}

fn get_infos(config: &Config, css: &[&ColorSpace], nts: &[&NamedTransform]) -> Infos {
    let mut all = Vec::with_capacity(css.len() + nts.len());
    for cs in css {
        all.push(ColorSpaceInfo::from_color_space(config, cs));
    }
    for nt in nts {
        all.push(ColorSpaceInfo::from_named_transform(config, nt));
    }
    all
}

fn get_names<T: CategoryItem>(list: &[&T]) -> ColorSpaceNames {
    list.iter().map(|i| i.item_name().to_string()).collect()
}

fn intersection<'a, T>(list0: &[&'a T], list1: &[&'a T]) -> Vec<&'a T> {
    list0
        .iter()
        .filter(|i0| list1.iter().any(|i1| std::ptr::eq(**i0, *i1)))
        .copied()
        .collect()
}

/// Split a comma-separated list of tokens into separate strings, making each
/// string lower case (`ExtractItems`). Empty items are dropped.
pub fn extract_items(strings: &str) -> Vec<String> {
    split(&lower(strings), ',')
        .iter()
        .map(|v| trim(v).to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// All the active color space names having at least one of the categories
/// (`FindColorSpaceNames`).
pub fn find_color_space_names(config: &Config, categories: &Categories) -> ColorSpaceNames {
    let all = get_color_spaces(
        config,
        true,
        false,
        SearchReferenceSpaceType::All,
        categories,
    );
    get_names(&all)
}

// Used by find_color_space_infos to identify and log if a fall-back was
// required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CategoryUsage {
    NotUsed,
    ShouldBeUsed,
    Ignored,
    NoneFound,
}

struct LogMessageHelper {
    ignore_encodings: bool,
    ignore_categories: bool,
    empty_intersection: bool,
    app_cats: CategoryUsage,
    user_cats: CategoryUsage,
}

impl LogMessageHelper {
    fn new() -> Self {
        Self {
            ignore_encodings: false,
            ignore_categories: false,
            empty_intersection: false,
            app_cats: CategoryUsage::NotUsed,
            user_cats: CategoryUsage::NotUsed,
        }
    }
}

impl Drop for LogMessageHelper {
    fn drop(&mut self) {
        let level = logging_level();
        if level != LoggingLevel::Unknown
            && level >= LoggingLevel::Info
            && (self.empty_intersection
                || self.ignore_encodings
                || self.ignore_categories
                || self.app_cats == CategoryUsage::NoneFound
                || self.user_cats == CategoryUsage::NoneFound
                || self.user_cats == CategoryUsage::Ignored)
        {
            let mut os = String::from("All parameters could not be used to create the menu:");
            if self.empty_intersection {
                os.push_str(
                    " Intersection of color spaces with app categories and color spaces with \
                     user categories is empty.",
                );
            }
            if self.app_cats == CategoryUsage::NoneFound {
                os.push_str(" Found no color space using app categories.");
                if self.user_cats == CategoryUsage::Ignored
                    || self.user_cats == CategoryUsage::NoneFound
                {
                    self.ignore_categories = true;
                }
            }
            if self.user_cats == CategoryUsage::NoneFound {
                os.push_str(" Found no color space using user categories.");
            } else if self.user_cats == CategoryUsage::Ignored {
                os.push_str(" User categories have been ignored.");
            }
            if self.ignore_encodings {
                os.push_str(" Encodings have been ignored since they matched no color spaces.");
            }
            if self.ignore_categories {
                os.push_str(" Categories have been ignored since they matched no color spaces.");
            }
            log_info(&os);
        }
    }
}

/// Find the menu items using the app categories, user categories and
/// encodings, with the fall-backs described in
/// [`ColorSpaceMenuParameters`](super::ColorSpaceMenuParameters)
/// (`FindColorSpaceInfos`).
#[allow(clippy::too_many_arguments)]
pub fn find_color_space_infos(
    config: &Config,
    app_categories: &Categories,
    user_categories: &Categories,
    include_color_spaces: bool,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    encodings: &Encodings,
    cs_type: SearchReferenceSpaceType,
) -> Infos {
    // At least one of include flags is true.

    let mut log = LogMessageHelper::new();

    // V1 does not have categories and encodings, skip them.
    if config.major_version() >= 2 {
        let mut app_cs: ColorSpaceVec = Vec::new();
        let mut app_nt: NamedTransformVec = Vec::new();
        let mut app_cs_no_enc: ColorSpaceVec = Vec::new();
        let mut app_nt_no_enc: NamedTransformVec = Vec::new();
        let mut app_no_enc_computed = false;

        let mut app_size = 0usize;

        let mut encs_ignored = encodings.is_empty();

        if !app_categories.is_empty() {
            // 3a) Use categories and encodings, fallback to only categories, fallback to only
            //     encodings.

            log.app_cats = CategoryUsage::ShouldBeUsed;

            // Use categories and encodings.
            if !encs_ignored {
                app_cs = get_color_spaces_cat_enc(
                    config,
                    include_color_spaces,
                    treat_no_category_as_any,
                    cs_type,
                    app_categories,
                    encodings,
                );
                app_nt = get_named_transforms_cat_enc(
                    config,
                    include_named_transforms,
                    treat_no_category_as_any,
                    app_categories,
                    encodings,
                );
                app_size = app_cs.len() + app_nt.len();
            }

            // Do not use encodings if empty or drop them if no result is found with them.
            if app_size == 0 {
                encs_ignored = true;
                log.ignore_encodings = !encodings.is_empty();
                app_cs = get_color_spaces(
                    config,
                    include_color_spaces,
                    treat_no_category_as_any,
                    cs_type,
                    app_categories,
                );
                app_nt = get_named_transforms(
                    config,
                    include_named_transforms,
                    treat_no_category_as_any,
                    app_categories,
                );
                app_size = app_cs.len() + app_nt.len();

                // Keep these results in case we need them later.
                app_no_enc_computed = true;
                app_cs_no_enc = app_cs.clone();
                app_nt_no_enc = app_nt.clone();
            }

            // Drop app categories and use encoding if no results.
            if app_size == 0 && !encodings.is_empty() {
                encs_ignored = false;
                log.ignore_encodings = false;
                log.app_cats = CategoryUsage::NoneFound;
                app_cs = get_color_spaces_from_encodings(
                    config,
                    include_color_spaces,
                    cs_type,
                    encodings,
                );
                app_nt = get_named_transforms_from_encodings(
                    config,
                    include_named_transforms,
                    encodings,
                );
                app_size = app_cs.len() + app_nt.len();
            }

            if app_size == 0 {
                log.app_cats = CategoryUsage::NoneFound;
            }
        } else if !encs_ignored {
            app_cs =
                get_color_spaces_from_encodings(config, include_color_spaces, cs_type, encodings);
            app_nt =
                get_named_transforms_from_encodings(config, include_named_transforms, encodings);
            app_size = app_cs.len() + app_nt.len();
        }

        let mut user_cs: ColorSpaceVec = Vec::new();
        let mut user_nt: NamedTransformVec = Vec::new();
        let mut user_size = 0usize;

        if !user_categories.is_empty() {
            // 3b) Items using user categories.
            user_cs = get_color_spaces(
                config,
                include_color_spaces,
                treat_no_category_as_any,
                cs_type,
                user_categories,
            );
            user_nt = get_named_transforms(
                config,
                include_named_transforms,
                treat_no_category_as_any,
                user_categories,
            );
            user_size = user_cs.len() + user_nt.len();
            if user_size == 0 {
                log.user_cats = CategoryUsage::NoneFound;
            }
        }

        if app_size != 0 && user_size != 0 {
            // 3c) and 3d) Use intersection of app and user categories.

            let mut use_no_enc = false;
            let encs_ignored_back = encs_ignored;
            let ignore_encodings_back = log.ignore_encodings;

            // Allow to run twice, with and without encodings.
            loop {
                let (test_cs, test_nt) = if use_no_enc {
                    (&app_cs_no_enc, &app_nt_no_enc)
                } else {
                    (&app_cs, &app_nt)
                };
                let css = intersection(test_cs, &user_cs);
                let nts = intersection(test_nt, &user_nt);

                if !css.is_empty() || !nts.is_empty() {
                    // 3c) or 3d) Intersection is not empty.
                    return get_infos(config, &css, &nts);
                }

                if !encs_ignored && !encodings.is_empty() {
                    // Intersection is empty, but encodings can be dropped if they were not dropped
                    // already.
                    encs_ignored = true;
                    log.ignore_encodings = true;
                    if !app_no_enc_computed {
                        // If not already computed, compute list with app categories and no
                        // encodings.
                        app_cs_no_enc = get_color_spaces(
                            config,
                            include_color_spaces,
                            treat_no_category_as_any,
                            cs_type,
                            app_categories,
                        );
                        app_nt_no_enc = get_named_transforms(
                            config,
                            include_named_transforms,
                            treat_no_category_as_any,
                            app_categories,
                        );
                    }
                    use_no_enc = true;
                } else {
                    break;
                }
            }
            log.empty_intersection = true;
            encs_ignored = encs_ignored_back;
            log.ignore_encodings = ignore_encodings_back;
        }
        let _ = encs_ignored;

        if app_size != 0 {
            // 3e) Only use app categories. Use the result of 3a).
            if !user_categories.is_empty() && log.user_cats != CategoryUsage::NoneFound {
                log.user_cats = CategoryUsage::Ignored;
            }
            return get_infos(config, &app_cs, &app_nt);
        }

        if user_size != 0 {
            // 3f) Only use user categories.
            return get_infos(config, &user_cs, &user_nt);
        }

        // Fallback to ignoring categories and encodings.
        log.ignore_categories = !app_categories.is_empty() || !user_categories.is_empty();
    }

    // 3g) Ignore all categories and encodings and return all items.

    let mut all_infos = Vec::new();
    for cs in active_color_spaces(config, cs_type) {
        all_infos.push(ColorSpaceInfo::from_color_space(config, cs));
    }

    if include_named_transforms {
        for nt in active_named_transforms(config) {
            all_infos.push(ColorSpaceInfo::from_named_transform(config, nt));
        }
    }

    // Nothing is found, no need to log anything.
    if all_infos.is_empty() {
        log.app_cats = CategoryUsage::NotUsed;
        log.user_cats = CategoryUsage::NotUsed;
        log.empty_intersection = false;
        log.ignore_categories = false;
        log.ignore_encodings = false;
    }
    all_infos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apphelpers::tests_data::CATEGORY_TEST_CONFIG;

    fn names(v: &[&ColorSpace]) -> Vec<String> {
        v.iter().map(|c| c.name().to_string()).collect()
    }

    fn nt_names(v: &[&NamedTransform]) -> Vec<String> {
        v.iter().map(|c| c.name().to_string()).collect()
    }

    #[test]
    fn category_helpers_categories() {
        let cats = extract_items("iNpuT");
        assert_eq!(cats, vec!["input"]);
        let cats = extract_items("    iNpuT     ");
        assert_eq!(cats, vec!["input"]);
        let cats = extract_items(",,iNpuT,    ,,");
        assert_eq!(cats, vec!["input"]);
        let cats = extract_items(",,iNpuT,    ,,lut_input_SPACE");
        assert_eq!(cats, vec!["input", "lut_input_space"]);
    }

    #[test]
    fn category_helpers_basic() {
        // This is testing internals. These do not include the various fallbacks that are
        // included at the ColorSpaceHelpers level.
        let config = Config::create_from_str(CATEGORY_TEST_CONFIG).unwrap();
        config.validate().unwrap();

        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&["sdr-video", "log"]);
            let css = get_color_spaces_cat_enc(
                &config,
                true,
                true,
                SearchReferenceSpaceType::Scene,
                &categories,
                &encodings,
            );
            assert_eq!(names(&css), vec!["log_1", "in_1", "in_2", "view_1"]);

            let css = get_color_spaces_cat_enc(
                &config,
                true,
                false,
                SearchReferenceSpaceType::Scene,
                &categories,
                &encodings,
            );
            assert_eq!(names(&css), vec!["log_1", "in_1", "in_2"]);

            let css = get_color_spaces_cat_enc(
                &config,
                false,
                true,
                SearchReferenceSpaceType::Scene,
                &categories,
                &encodings,
            );
            assert_eq!(css.len(), 0);
        }
        {
            let categories = s(&[]);
            let encodings = s(&["sdr-video", "log"]);
            let css = get_color_spaces_cat_enc(
                &config,
                true,
                false,
                SearchReferenceSpaceType::Scene,
                &categories,
                &encodings,
            );
            assert_eq!(css.len(), 0);
            let css = get_color_spaces_from_encodings(
                &config,
                true,
                SearchReferenceSpaceType::Scene,
                &encodings,
            );
            assert_eq!(css.len(), 4);
        }
        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&[]);
            let css = get_color_spaces_cat_enc(
                &config,
                true,
                false,
                SearchReferenceSpaceType::Scene,
                &categories,
                &encodings,
            );
            assert_eq!(css.len(), 0);

            let css = get_color_spaces(
                &config,
                true,
                false,
                SearchReferenceSpaceType::Scene,
                &categories,
            );
            assert_eq!(css.len(), 7);
            let css = get_color_spaces(
                &config,
                true,
                true,
                SearchReferenceSpaceType::Scene,
                &categories,
            );
            assert_eq!(css.len(), 9);
        }
        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&["sdr-video", "log"]);
            let css = get_color_spaces_cat_enc(
                &config,
                true,
                true,
                SearchReferenceSpaceType::Display,
                &categories,
                &encodings,
            );
            assert_eq!(names(&css), vec!["display_lin_2", "display_log_1"]);
        }
        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&["sdr-video", "log"]);
            let css = get_color_spaces_cat_enc(
                &config,
                true,
                false,
                SearchReferenceSpaceType::All,
                &categories,
                &encodings,
            );
            assert_eq!(
                names(&css),
                vec!["log_1", "in_1", "in_2", "display_lin_2", "display_log_1"]
            );
        }
        {
            let categories = s(&["file-io", "working-space"]);
            let css = get_color_spaces(
                &config,
                true,
                false,
                SearchReferenceSpaceType::All,
                &categories,
            );
            assert_eq!(
                names(&css),
                vec![
                    "lin_1",
                    "lin_2",
                    "log_1",
                    "in_1",
                    "in_2",
                    "in_3",
                    "lut_input_3",
                    "display_lin_1",
                    "display_lin_2",
                    "display_log_1"
                ]
            );

            let css = get_color_spaces(
                &config,
                true,
                true,
                SearchReferenceSpaceType::All,
                &categories,
            );
            assert_eq!(css.len(), 12);

            let css = get_color_spaces(
                &config,
                false,
                true,
                SearchReferenceSpaceType::All,
                &categories,
            );
            assert_eq!(css.len(), 0);
        }
        {
            let encodings = s(&["sdr-video", "log"]);
            let css = get_color_spaces_from_encodings(
                &config,
                true,
                SearchReferenceSpaceType::All,
                &encodings,
            );
            assert_eq!(
                names(&css),
                vec![
                    "log_1",
                    "in_1",
                    "in_2",
                    "view_1",
                    "display_lin_2",
                    "display_log_1"
                ]
            );

            let css = get_color_spaces_from_encodings(
                &config,
                false,
                SearchReferenceSpaceType::All,
                &encodings,
            );
            assert_eq!(css.len(), 0);
        }

        // Named Transforms

        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&["sdr-video", "log"]);
            let nts = get_named_transforms_cat_enc(&config, true, true, &categories, &encodings);
            assert_eq!(nt_names(&nts), vec!["nt1", "nt2", "nt3"]);

            let nts = get_named_transforms_cat_enc(&config, true, false, &categories, &encodings);
            assert_eq!(nt_names(&nts), vec!["nt1", "nt3"]);

            let nts = get_named_transforms_cat_enc(&config, false, true, &categories, &encodings);
            assert_eq!(nts.len(), 0);
        }
        {
            let categories = s(&[]);
            let encodings = s(&["sdr-video", "log"]);
            let nts = get_named_transforms_cat_enc(&config, true, true, &categories, &encodings);
            assert_eq!(nts.len(), 0);
        }
        {
            let categories = s(&["file-io", "working-space"]);
            let encodings = s(&[]);
            let nts = get_named_transforms_cat_enc(&config, true, true, &categories, &encodings);
            assert_eq!(nts.len(), 0);
        }
        {
            let categories = s(&["file-io"]);
            let nts = get_named_transforms(&config, true, true, &categories);
            assert_eq!(nt_names(&nts), vec!["nt2", "nt3"]);

            let nts = get_named_transforms(&config, true, false, &categories);
            assert_eq!(nt_names(&nts), vec!["nt3"]);

            let nts = get_named_transforms(&config, false, true, &categories);
            assert_eq!(nts.len(), 0);
        }
        {
            let encodings = s(&["log"]);
            let nts = get_named_transforms_from_encodings(&config, true, &encodings);
            assert_eq!(nt_names(&nts), vec!["nt2"]);

            let nts = get_named_transforms_from_encodings(&config, false, &encodings);
            assert_eq!(nts.len(), 0);
        }
    }
}
