//! ociocheck -- validate an OpenColorIO configuration (port of the OCIO
//! `ociocheck` app).

use ocio::config::logging::{set_logging_level, LogGuard};
use ocio::config::utils::{contain, split_by_lines};
use ocio::config::ColorSpace;
use ocio::{
    ColorSpaceDirection, ColorSpaceVisibility, Config, EnvironmentMode, FixedFunctionStyle,
    FixedFunctionTransform, LoggingLevel, NamedTransformVisibility, ProcessorCacheFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, Transform, TransformDirection, ViewType,
};
use ocio_tools::argparse::ArgParse;
use std::collections::BTreeSet;
use std::process::ExitCode;

const DESC_STRING: &str = "\n\n\
Ociocheck is useful to validate that the specified OCIO configuration\n\
is valid, and that all the color transforms are defined and loadable.\n\
For example, it is possible that the configuration may reference\n\
lookup tables that do not exist and ociocheck will find these cases.\n\
Unlike the config validate method, ociocheck parses all required LUTs.\n\
All display/view pairs, color spaces, and named transforms are checked,\n\
regardless of whether they are active or inactive.\n\n\
Ociocheck can also be used to clean up formatting on an existing profile\n\
that has been manually edited, using the '-o' option.\n";

const CIF_TEXTURE_IDS: &[&str] = &[
    "lin_ap1_scene",
    "lin_ap0_scene",
    "lin_rec709_scene",
    "lin_p3d65_scene",
    "lin_rec2020_scene",
    "lin_adobergb_scene",
    "lin_ciexyzd65_scene",
    "srgb_rec709_scene",
    "g22_rec709_scene",
    "g18_rec709_scene",
    "srgb_ap1_scene",
    "g22_ap1_scene",
    "srgb_p3d65_scene",
    "g22_adobergb_scene",
    "data",
    "unknown",
];

const CIF_DISPLAY_IDS: &[&str] = &[
    "srgb_rec709_display",
    "g24_rec709_display",
    "srgb_p3d65_display",
    "srgbe_p3d65_display",
    "pq_p3d65_display",
    "pq_rec2020_display",
    "hlg_rec2020_display",
    "g22_rec709_display",
    "g22_adobergb_display",
    "g26_p3d65_display",
    "g26_xyzd65_display",
    "pq_xyzd65_display",
];

/// Returns true if the interop id is valid (printing a warning otherwise).
fn is_valid_interop_id(id: &str) -> bool {
    // See https://github.com/AcademySoftwareFoundation/ColorInterop for the details.
    if id.is_empty() {
        return true;
    }
    let is_cif = |s: &str| CIF_TEXTURE_IDS.contains(&s) || CIF_DISPLAY_IDS.contains(&s);
    match id.find(':') {
        None => {
            // No namespace, so the ID must be in the Color Interop Forum ID list.
            if !is_cif(id) {
                println!(
                    "WARNING: InteropID '{id}' is not valid. It should either be one of the \
                     Color Interop Forum standard IDs or it must contain a namespace followed \
                     by ':', e.g. 'mycompany:mycolorspace'."
                );
                return false;
            }
        }
        Some(pos) => {
            // The ID should not be in the Color Interop Forum ID list.
            let cs = &id[pos + 1..];
            if is_cif(cs) {
                println!(
                    "WARNING: InteropID '{id}' is not valid. The ID part must not be one of \
                     the Color Interop Forum standard IDs when a namespace is used."
                );
                return false;
            }
        }
    }
    true
}

/// Try to build the processor of an optional transform, returning the error
/// message on failure.
fn check_transform(config: &Config, t: Option<&Transform>) -> Result<(), String> {
    if let Some(t) = t {
        config
            .get_processor_for_transform(t, TransformDirection::Forward)
            .map_err(|e| e.message().to_string())?;
    }
    Ok(())
}

/// Print the result of the two checks of an element.
fn report(name: &str, first: &Result<(), String>, second: &Result<(), String>) -> bool {
    if first.is_err() || second.is_err() {
        println!("{name} -- error");
        if let Err(e) = first {
            println!("\t{e}");
        }
        if let Err(e) = second {
            println!("\t{e}");
        }
        false
    } else {
        println!("{name}");
        true
    }
}

fn run(ap: &ArgParse) -> Result<(i32, i32), String> {
    let inputconfig = ap.get_string("iconfig", "");
    let outputconfig = ap.get_string("oconfig", "");

    let mut errorcount = 0;
    let mut warningcount = 0;

    println!();
    println!("OpenColorIO Library Version: {}", ocio_tools::version());
    println!(
        "OpenColorIO Library VersionHex: {}",
        ocio_tools::version_hex()
    );

    let src_config = if !inputconfig.is_empty() {
        println!();
        println!("Loading {inputconfig}");
        Config::create_from_file(&inputconfig).map_err(|e| e.to_string())?
    } else if let Some(env) = ocio_tools::env_variable("OCIO") {
        println!();
        println!("Loading $OCIO {env}");
        Config::create_from_env().map_err(|e| e.to_string())?
    } else {
        println!();
        print!("ERROR: You must specify an input OCIO configuration ");
        println!("(either with --iconfig or $OCIO).");
        ap.print_usage();
        print!("{DESC_STRING}");
        return Err(String::new());
    };

    // This program calls getProcessor for every color space and display/view
    // in the config, so turn off the Processor cache.
    let config = src_config.create_editable_copy();
    config.set_processor_cache_flags(ProcessorCacheFlags::OFF);

    println!();
    println!("** General **");

    if config.num_environment_vars() > 0 {
        println!("Environment:");
        for idx in 0..config.num_environment_vars() {
            let name = config.environment_var_name_by_index(idx);
            println!("  {}: {}", name, config.environment_var_default(name));
        }
    } else if config.environment_mode() == EnvironmentMode::LoadPredefined {
        println!("Environment: {{}}");
    } else {
        println!("Environment: <missing>");
    }

    println!("Search Path: {}", config.search_path());
    println!("Working Dir: {}", config.working_dir());

    if config.num_displays() == 0 {
        println!();
        println!("ERROR: At least one (display, view) pair must be defined.");
        errorcount += 1;
    } else {
        println!();
        let default_display = config.default_display();
        println!("Default Display: {default_display}");
        println!("Default View: {}", config.default_view(&default_display));

        // It's important that the getProcessor call below always loads the
        // transforms involved in each display/view pair. However, if the src
        // color space is a data space or if the view's colorspace happens to
        // be the same as the src color space, it will effectively bypass the
        // loading of the transform. The work-around used here is to create a
        // copy of the config and add a unique src color space that will
        // definitely allow the Processor to be created.
        let mut display_test_config = config.create_editable_copy();
        let src_color_space = "ocioCheckTotallyUniqueColorSpaceName";
        let mut cs = ColorSpace::new(ReferenceSpaceType::Scene);
        cs.set_name(src_color_space);
        let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow10, &[]);
        cs.set_transform(
            Some(Transform::FixedFunction(ff)),
            ColorSpaceDirection::ToReference,
        );
        display_test_config
            .add_color_space(&cs)
            .map_err(|e| e.to_string())?;

        if config.num_color_spaces() > 0 {
            println!();
            println!("** (Display, View) pairs **");

            // Iterate over all displays & views (active & inactive).
            for idx_disp in 0..config.num_displays_all() {
                let display = config.display_all(idx_disp).to_string();
                // Iterate over shared views, then over display-defined views.
                for ty in [ViewType::Shared, ViewType::DisplayDefined] {
                    for idx_view in 0..config.num_views_by_type(ty, &display) {
                        let view = config.view_by_type(ty, &display, idx_view).to_string();
                        match display_test_config.get_display_view_processor_dir(
                            src_color_space,
                            &display,
                            &view,
                            TransformDirection::Forward,
                        ) {
                            Ok(_) => println!("({display}, {view})"),
                            Err(e) => {
                                println!("ERROR: {e}");
                                errorcount += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    {
        println!();
        println!("** Roles **");

        // All roles defined in OpenColorTypes.h.
        let all_roles: BTreeSet<&str> = [
            ocio::ROLE_DEFAULT,
            ocio::ROLE_SCENE_LINEAR,
            ocio::ROLE_DATA,
            ocio::ROLE_REFERENCE,
            ocio::ROLE_COMPOSITING_LOG,
            ocio::ROLE_COLOR_TIMING,
            ocio::ROLE_COLOR_PICKING,
            ocio::ROLE_TEXTURE_PAINT,
            ocio::ROLE_MATTE_PAINT,
            ocio::ROLE_RENDERING,
            ocio::ROLE_INTERCHANGE_SCENE,
            ocio::ROLE_INTERCHANGE_DISPLAY,
        ]
        .into_iter()
        .collect();

        // Print the roles, appending ": user" if they are not one of the
        // "standard" roles defined in the library.
        for i in 0..config.num_roles() {
            let role = config.role_name(i);
            match config.get_color_space(role) {
                Some(cs) => {
                    if all_roles.contains(role) {
                        println!("{} ({})", cs.name(), role);
                    } else {
                        println!("{} ({}: user)", cs.name(), role);
                    }
                }
                None => {
                    // Note: The config validate check below will also fail due to this.
                    println!("ERROR: SPACE MISSING ({role})");
                    errorcount += 1;
                }
            }
        }

        // Roles that are actually used by the library or important
        // tools/plug-ins. Config::validate ensures these are present for 2.2
        // and later configs, but warn if they are missing in earlier configs.
        let essential_roles = [
            ocio::ROLE_SCENE_LINEAR,
            ocio::ROLE_COLOR_TIMING,
            ocio::ROLE_COMPOSITING_LOG,
            ocio::ROLE_INTERCHANGE_SCENE,
            ocio::ROLE_INTERCHANGE_DISPLAY,
        ];
        for role in essential_roles {
            if config.get_color_space(role).is_none() {
                println!("WARNING: NOT DEFINED ({role})");
                warningcount += 1;
            }
        }
    }

    {
        println!();
        println!("** ColorSpaces **");

        let num_cs = config
            .num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);

        let mut found_category = false;
        let mut found_no_category = false;

        for i in 0..num_cs {
            let name = config.color_space_name_by_index_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            );
            let cs = match config.get_color_space(name) {
                Some(cs) => cs,
                None => continue,
            };

            let interop_id = cs.interop_id();
            if !interop_id.is_empty() && !is_valid_interop_id(interop_id) {
                warningcount += 1;
            }

            if !config.is_inactive_color_space(cs.name()) {
                if cs.num_categories() > 0 {
                    found_category = true;
                } else {
                    found_no_category = true;
                }
            }

            // Try to load the transforms for both directions -- this will load any LUTs.
            let to_ref = check_transform(&config, cs.transform(ColorSpaceDirection::ToReference));
            let from_ref =
                check_transform(&config, cs.transform(ColorSpaceDirection::FromReference));
            if !report(cs.name(), &to_ref, &from_ref) {
                errorcount += 1;
            }
        }

        if found_category && found_no_category {
            // Categories should either be missing in all, or present in all active items.
            print!(
                "\nWARNING: The config has some color spaces where the categories are not set.\n"
            );
            warningcount += 1;
        }
    }

    {
        println!();
        println!("** Named Transforms **");

        // Iterate over active & inactive named transforms.
        let num_nt = config.num_named_transforms_filtered(NamedTransformVisibility::All);
        if num_nt == 0 {
            println!("no named transforms defined");
        }

        let mut found_category = false;
        let mut found_no_category = false;

        for i in 0..num_nt {
            let name =
                config.named_transform_name_by_index_filtered(NamedTransformVisibility::All, i);
            let nt = match config.get_named_transform(name) {
                Some(nt) => nt,
                None => continue,
            };

            if !config.is_inactive_color_space(nt.name()) {
                if nt.num_categories() > 0 {
                    found_category = true;
                } else {
                    found_no_category = true;
                }
            }

            // Try to load the transforms -- this will load any LUTs.
            let fwd = check_transform(&config, nt.transform(TransformDirection::Forward));
            let inv = check_transform(&config, nt.transform(TransformDirection::Inverse));
            if !report(nt.name(), &fwd, &inv) {
                errorcount += 1;
            }
        }

        if found_category && found_no_category {
            // Categories should either be missing in all, or present in all active items.
            print!(
                "\nWARNING: The config has some named transforms where the categories are not set.\n"
            );
            warningcount += 1;
        }
    }

    {
        println!();
        println!("** Looks **");

        let num_looks = config.num_looks();
        if num_looks == 0 {
            println!("no looks defined");
        }

        for i in 0..num_looks {
            let look = match config.look(config.look_name_by_index(i)) {
                Some(l) => l,
                None => continue,
            };
            // Try to load the transforms -- this will load any LUTs.
            let fwd = check_transform(&config, look.transform());
            let inv = check_transform(&config, look.inverse_transform());
            if !report(look.name(), &fwd, &inv) {
                errorcount += 1;
            }
        }
    }

    println!();
    println!("** Validation **");

    let mut cache_id = String::new();
    let mut is_archivable = false;
    {
        let guard = LogGuard::new();
        match config.validate() {
            Ok(()) => {
                let output = guard.output();
                print!("{output}");

                cache_id = config.cache_id();
                is_archivable = config.is_archivable();

                // Passed if there are no Error level logs.
                let lines = split_by_lines(&output);
                if !contain(&lines, "[OpenColorIO Error]") {
                    println!("Validation: passed");
                } else {
                    println!("Validation: failed");
                    errorcount += 1;
                }
            }
            Err(e) => {
                println!("ERROR:");
                errorcount += 1;
                println!("{e}");
                println!("Validation: failed");
            }
        }
    }

    println!();
    println!("** Miscellaneous **");
    println!("CacheID: {cache_id}");
    println!("Archivable: {}", if is_archivable { "yes" } else { "no" });

    if !outputconfig.is_empty() {
        let text = config.serialize().map_err(|e| e.to_string())?;
        match std::fs::write(&outputconfig, text) {
            Ok(()) => println!("Wrote {outputconfig}"),
            Err(_) => println!("Error opening {outputconfig} for writing."),
        }
    }

    Ok((errorcount, warningcount))
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ociocheck -- validate an OpenColorIO configuration\n\n\
         usage:  ociocheck [options]\n",
    )
    .flag("--help", "help", "Print help message")
    .option(
        "--iconfig %s",
        &["iconfig"],
        "Input .ocio configuration file (default: $OCIO)",
    )
    .option("--oconfig %s", &["oconfig"], "Output .ocio file");

    if ap.parse(&argv).is_err() {
        println!("{}", ap.geterror());
        ap.print_usage();
        print!("{DESC_STRING}");
        return ExitCode::from(1);
    }

    if ap.get_bool("help") {
        ap.print_usage();
        print!("{DESC_STRING}");
        return ExitCode::from(1);
    }

    // Set the logging level to INFO.
    set_logging_level(LoggingLevel::Info);

    let (errorcount, warningcount) = match run(&ap) {
        Ok(counts) => counts,
        Err(msg) => {
            if !msg.is_empty() {
                println!("ERROR: {msg}");
            }
            return ExitCode::from(1);
        }
    };

    if warningcount > 0 {
        println!("\nWarnings encountered: {warningcount}");
    }

    println!();
    if errorcount == 0 {
        println!("Tests complete.");
        println!();
        ExitCode::SUCCESS
    } else {
        println!("{errorcount} tests failed.");
        println!();
        ExitCode::from(1)
    }
}
