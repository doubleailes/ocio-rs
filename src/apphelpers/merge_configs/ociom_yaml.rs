//! Reading and writing of OCIOM files (port of
//! `apphelpers/mergeconfigs/OCIOMYaml.cpp`).

use super::{ConfigMerger, ConfigMergingParameters, MergeStrategies};
use crate::config::logging::log_warning;
use crate::config::utils::{join, split, split_string_env_style_lossy};
use crate::config::yaml::emitter::{Emitter, Manip};
use crate::config::yaml::node::{self, Node};
use crate::error::{Error, Result};
use crate::path_utils;
use std::collections::HashSet;

fn load_string(node: &Node) -> Result<String> {
    node.as_string().map_err(|e| {
        Error::msg(format!(
            "At line {}, '{}' parsing string failed with: {}",
            node.line + 1,
            node.tag,
            e.message()
        ))
    })
}

fn load_string_vec(node: &Node) -> Result<Vec<String>> {
    node.as_string_vec().map_err(|e| {
        Error::msg(format!(
            "At line {}, '{}' parsing StringVec failed with: {}",
            node.line + 1,
            node.tag,
            e.message()
        ))
    })
}

fn value_error(node_name: &str, key: &Node, msg: &str) -> Error {
    let key_name = match load_string(key) {
        Ok(k) => k,
        Err(e) => return e,
    };
    Error::msg(format!(
        "At line {}, the value of the property '{}' from '{}' failed: {}",
        key.line + 1,
        key_name,
        node_name,
        msg
    ))
}

fn check_duplicates(node: &Node) -> Result<()> {
    let mut keys = HashSet::new();
    for (k, _) in node.map_entries() {
        let key = k.as_string()?;
        if !keys.insert(key.clone()) {
            return Err(value_error(
                &node.tag,
                k,
                &format!("Key-value pair with key '{key}' specified more than once. "),
            ));
        }
    }
    Ok(())
}

fn generic_strategy_handler(pnode: &Node, node: &Node) -> Result<MergeStrategies> {
    if !node.is_map() {
        return Err(value_error(
            &node.tag,
            pnode,
            "The value type of a property 'strategy' needs to be a map.",
        ));
    }
    let mut strategy = String::new();
    for (k, v) in node.map_entries() {
        let prop = k.as_string()?;
        let value = v.as_string()?;
        if prop == "strategy" {
            strategy = value;
        }
    }
    let e = MergeStrategies::from_str_lossy(&strategy);
    if e == MergeStrategies::Unspecified {
        return Err(value_error(
            &node.tag,
            pnode,
            &format!("The value '{strategy}' is not recognized. "),
        ));
    }
    Ok(e)
}

fn load_options(node: &Node, params: &mut ConfigMergingParameters) -> Result<()> {
    check_duplicates(node)?;
    for (k, v) in node.map_entries() {
        let key = k.as_string()?;
        match key.as_str() {
            "input_family_prefix" => params.set_input_family_prefix(&v.as_string()?),
            "base_family_prefix" => params.set_base_family_prefix(&v.as_string()?),
            "input_first" => params.set_input_first(v.as_bool()?),
            "error_on_conflict" => params.set_error_on_conflict(v.as_bool()?),
            "avoid_duplicates" => params.set_avoid_duplicates(v.as_bool()?),
            "adjust_input_reference_space" => params.set_adjust_input_reference_space(v.as_bool()?),
            // Supported as a synonym for adjust_input_reference_space.
            "assume_common_reference_space" => {
                params.set_adjust_input_reference_space(!v.as_bool()?)
            }
            "default_strategy" => {
                let strategy = v.as_string()?;
                let e = MergeStrategies::from_str_lossy(&strategy);
                if e == MergeStrategies::Unspecified {
                    return Err(value_error(
                        &node.tag,
                        k,
                        &format!("The value '{strategy}' is not recognized. "),
                    ));
                }
                params.set_default_strategy(e);
            }
            _ => {}
        }
    }
    Ok(())
}

fn load_overrides(node: &Node, params: &mut ConfigMergingParameters) -> Result<()> {
    check_duplicates(node)?;
    for (k, v) in node.map_entries() {
        let key = k.as_string()?;
        if v.is_null() {
            continue;
        }
        match key.as_str() {
            "name" => params.set_name(&load_string(v)?),
            "description" => params.set_description(&load_string(v)?),
            "search_path" => {
                if v.size() == 0 {
                    params.set_search_path(&load_string(v)?);
                } else {
                    for path in load_string_vec(v)? {
                        params.add_search_path(&path);
                    }
                }
            }
            "environment" => {
                if !v.is_map() {
                    return Err(value_error(
                        &node.tag,
                        k,
                        "The value type of key 'environment' needs to be a map.",
                    ));
                }
                for (ek, ev) in v.map_entries() {
                    params.add_environment_var(&ek.as_string()?, &ev.as_string()?);
                }
            }
            "active_displays" => {
                let list = load_string_vec(v)?;
                params.set_active_displays(&join(&list, ','))?;
            }
            "active_views" => {
                let list = load_string_vec(v)?;
                params.set_active_views(&join(&list, ','))?;
            }
            "inactive_colorspaces" => {
                let list = load_string_vec(v)?;
                params.set_inactive_color_spaces(&join(&list, ','));
            }
            _ => {}
        }
    }
    Ok(())
}

fn load_params(node: &Node, params: &mut ConfigMergingParameters) -> Result<()> {
    // Check for duplicates in params.
    check_duplicates(node)?;
    for (k, v) in node.map_entries() {
        let key = k.as_string()?;
        match key.as_str() {
            "roles" => params.set_roles(generic_strategy_handler(k, v)?),
            "file_rules" => params.set_file_rules(generic_strategy_handler(k, v)?),
            "display-views" => params.set_display_views(generic_strategy_handler(k, v)?),
            "view_transforms" => params.set_view_transforms(generic_strategy_handler(k, v)?),
            "looks" => params.set_looks(generic_strategy_handler(k, v)?),
            "colorspaces" => params.set_colorspaces(generic_strategy_handler(k, v)?),
            "named_transforms" => params.set_named_transforms(generic_strategy_handler(k, v)?),
            _ => log_warning(&format!("Unsupported property in merge params: {key}")),
        }
    }
    Ok(())
}

/// `std::stoi`: optional white spaces and sign, then at least one digit.
fn stoi(s: &str) -> Option<u32> {
    let t = s.trim_start();
    let t = t.strip_prefix('+').unwrap_or(t);
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

// Count the merges of an OCIOM document (`countMerges`).
fn count_merges(root: &Node) -> Result<usize> {
    let mut n = 0;
    check_duplicates(root)?;
    for (k, v) in root.map_entries() {
        let key = k.as_string()?;
        if v.is_null() {
            continue;
        }
        if key == "merge" {
            if !v.is_map() {
                return Err(value_error(
                    &v.tag,
                    k,
                    "The value type of the key 'merge' needs to be a map.",
                ));
            }
            check_duplicates(v)?;
            n += v.map_entries().len();
        }
    }
    Ok(n)
}

// Load an OCIOM document into the merger (`OCIOMYaml::load`).
fn load(root: &Node, merger: &mut ConfigMerger, filename: &str) -> Result<()> {
    check_duplicates(root)?;
    for (k, v) in root.map_entries() {
        let key = k.as_string()?;
        if v.is_null() {
            continue;
        }

        match key.as_str() {
            "ociom_version" => {
                let version = load_string(root.get("ociom_version").unwrap_or(v))?;
                let results = split(&version, '.');
                let parsed = match results.len() {
                    1 => stoi(&results[0]).map(|m| (m, 0)),
                    2 => match (stoi(&results[0]), stoi(&results[1])) {
                        (Some(a), Some(b)) => Some((a, b)),
                        _ => None,
                    },
                    _ => None,
                };
                match parsed {
                    Some((major, minor)) => merger.set_version(major, minor),
                    None => {
                        return Err(value_error(
                            &v.tag,
                            k,
                            &format!("The value '{version}' is not a valid OCIOM version."),
                        ))
                    }
                }
                if merger.major_version() > 1 || merger.minor_version() > 0 {
                    return Err(value_error(
                        &v.tag,
                        k,
                        "The highest supported OCIOM file version is 1.0.",
                    ));
                }
            }
            "search_path" => {
                if v.size() == 0 {
                    merger.set_search_path(&load_string(v)?);
                } else {
                    for path in load_string_vec(v)? {
                        merger.add_search_path(&path);
                    }
                }
            }
            "merge" => {
                if !v.is_map() {
                    return Err(value_error(
                        &v.tag,
                        k,
                        "The value type of the key 'merge' needs to be a map.",
                    ));
                }
                for (counter, (mk, mv)) in v.map_entries().iter().enumerate() {
                    let merged_name = mk.as_string()?;
                    let params = merger
                        .params_mut(counter)
                        .ok_or_else(|| Error::msg("Invalid number of merges."))?;
                    params.set_output_name(&merged_name);

                    for (pk, pv) in mv.map_entries() {
                        let pkey = pk.as_string()?;
                        match pkey.as_str() {
                            "base" => params.set_base_config_name(&pv.as_string()?),
                            "input" => params.set_input_config_name(&pv.as_string()?),
                            "options" => load_options(pv, params)?,
                            "overrides" => load_overrides(pv, params)?,
                            "params" => load_params(pv, params)?,
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        if !filename.is_empty() {
            // The working directory defaults to the directory of the OCIOM file.
            let real = path_utils::absolute(filename);
            merger.set_working_dir(&path_utils::dirname(&real));
        }
    }
    Ok(())
}

/// Parse an OCIOM document (`ConfigMerger::Impl::Read`).
pub(super) fn read(text: &str, filepath: &str) -> Result<ConfigMerger> {
    let parse = || -> Result<ConfigMerger> {
        let root = node::load(text)?;
        let num_merges = count_merges(&root)?;

        let mut merger = ConfigMerger::new();
        // Create the needed merge parameters.
        for _ in 0..num_merges {
            merger.add_params(ConfigMergingParameters::new());
        }

        load(&root, &mut merger, filepath)?;
        Ok(merger)
    };
    parse().map_err(|e| {
        Error::msg(format!(
            "Error: Loading the OCIOM Merge parameters '{}' failed. {}",
            filepath,
            e.message()
        ))
    })
}

fn split_or_empty(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        split_string_env_style_lossy(s)
    }
}

/// Serialize a merger to an OCIOM document (`OCIOMYaml::write`).
pub(super) fn write(merger: &ConfigMerger) -> Result<String> {
    use Manip::*;

    let mut out = Emitter::new();
    out.manip(Block);
    out.manip(BeginMap);
    out.key("ociom_version").string(&format!(
        "{}.{}",
        merger.major_version(),
        merger.minor_version()
    ));
    out.key("search_path");
    out.manip(BeginSeq);
    for i in 0..merger.num_search_paths() {
        out.string(merger.search_path(i));
    }
    out.manip(EndSeq);
    out.manip(Newline);

    out.key("merge");
    out.manip(BeginMap);

    for mp in 0..merger.num_config_merging_parameters() {
        let p = match merger.params(mp) {
            Some(p) => p,
            None => continue,
        };
        // Serialize every merge section.
        out.key(p.output_name());
        out.manip(BeginMap);

        out.key("base").string(p.base_config_name());
        out.key("input").string(p.input_config_name());
        out.manip(Newline);

        out.key("options");
        out.manip(BeginMap);
        out.key("input_family_prefix")
            .string(p.input_family_prefix());
        out.key("base_family_prefix").string(p.base_family_prefix());
        out.key("input_first").boolean(p.is_input_first());
        out.key("error_on_conflict")
            .boolean(p.is_error_on_conflict());
        out.key("default_strategy")
            .string(p.default_strategy().as_str());
        out.key("avoid_duplicates").boolean(p.is_avoid_duplicates());
        out.key("adjust_input_reference_space")
            .boolean(p.is_adjust_input_reference_space());
        // End of options section.
        out.manip(EndMap);
        out.manip(Newline);

        out.key("overrides");
        out.manip(BeginMap);
        out.key("name").string(p.name());
        out.key("description").string(p.description());
        out.key("search_path").string(&p.search_path());

        out.key("environment");
        out.manip(BeginMap);
        for i in 0..p.num_environment_vars() {
            out.key(p.environment_var(i));
            out.string(p.environment_var_value(i));
        }
        out.manip(EndMap);
        out.manip(Newline);

        out.key("active_displays");
        out.manip(Flow)
            .string_seq(&split_or_empty(&p.active_displays()));
        out.manip(Newline);

        out.key("active_views");
        out.manip(Flow)
            .string_seq(&split_or_empty(&p.active_views()));

        out.key("inactive_colorspaces");
        out.manip(Flow)
            .string_seq(&split_or_empty(p.inactive_color_spaces()));

        // End of overrides section.
        out.manip(EndMap);
        out.manip(Newline);

        out.key("params");
        out.manip(BeginMap);
        let sections = [
            ("roles", p.roles()),
            ("file_rules", p.file_rules()),
            ("display-views", p.display_views()),
            ("view_transforms", p.view_transforms()),
            ("looks", p.looks()),
            ("colorspaces", p.colorspaces()),
            ("named_transforms", p.named_transforms()),
        ];
        for (name, strategy) in sections {
            out.key(name);
            out.manip(BeginMap);
            out.key("strategy").string(strategy.as_str());
            out.manip(EndMap);
        }
        // End of params section.
        out.manip(EndMap);

        // End of the current merge section.
        out.manip(EndMap);
    }

    // End of the merges.
    out.manip(EndMap);
    out.manip(EndMap);

    Ok(out.into_string())
}
