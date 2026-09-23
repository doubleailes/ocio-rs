//! ociowrite -- write a color transformation to a file (port of the OCIO
//! `ociowrite` app).

use ocio::{Config, GroupTransform, Processor, TransformDirection};
use ocio_tools::argparse::ArgParse;
use std::process::ExitCode;

fn fail(msg: &str) -> ExitCode {
    eprintln!();
    eprintln!("{msg}");
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();

    // What are the allowed writing output formats?
    let mut formats = String::from(
        "\n                            Formats to write to:\n                             ",
    );
    for i in 0..GroupTransform::num_write_formats() {
        formats.push_str(&format!(
            "{} (.{})\n                             ",
            GroupTransform::format_name_by_index(i),
            GroupTransform::format_extension_by_index(i)
        ));
    }
    let path_desc = format!("Transform file path. Format is implied by extension. {formats}");

    let mut ap = ArgParse::new(
        "ociowrite -- write a color transformation to a file\n\n\
         usage: ociowrite [options] --file outputfile\n\n",
    )
    .flag("--h", "help", "Display the help and exit")
    .flag("--help", "help", "Display the help and exit")
    .flag("--v", "verbose", "Display some general information")
    .option(
        "--colorspaces %s %s",
        &["incs", "outcs"],
        "Provide the input and output color spaces to apply on the image",
    )
    .option(
        "--view %s %s %s",
        &["incs", "display", "view"],
        "Provide the input color space and (display, view) pair to apply on the image",
    )
    .option(
        "--displayview %s %s %s",
        &["incs", "display", "view"],
        "(Deprecated) Provide the input color space and (display, view) pair to apply on the image",
    )
    .option(
        "--invertview %s %s %s",
        &["display", "view", "outcs"],
        "Provide the (display, view) pair and output color space to apply on the image",
    )
    .option("--file %s", &["file"], &path_desc);

    if argv.len() <= 1 || ap.parse(&argv).is_err() {
        eprintln!("{}", ap.geterror());
        ap.print_usage();
        return ExitCode::from(1);
    }

    if ap.get_bool("help") {
        ap.print_usage();
        return ExitCode::from(1);
    }

    let verbose = ap.get_bool("verbose");
    let input_cs = ap.get_string("incs", "");
    let output_cs = ap.get_string("outcs", "");
    let display = ap.get_string("display", "");
    let view = ap.get_string("view", "");
    let filepath = ap.get_string("file", "");

    let env = ocio_tools::env_variable("OCIO").filter(|e| !e.is_empty());

    if verbose {
        println!();
        println!("OCIO Version: {}", ocio_tools::version());
        if let Some(env) = &env {
            println!();
            println!("OCIO Configuration: '{env}'");
            match Config::create_from_env() {
                Ok(config) => println!("OCIO search_path:    {}", config.search_path()),
                Err(_) => {
                    eprint!("Error loading the config file: '{env}'");
                    return ExitCode::from(1);
                }
            }
        }
    }

    if filepath.is_empty() {
        return fail("The output transform filepath is missing.");
    }

    let mut transform_file_format = String::new();
    if let Some(pos) = filepath.rfind('.') {
        let requested_ext = filepath[pos + 1..].to_ascii_lowercase();
        for i in 0..GroupTransform::num_write_formats() {
            if requested_ext == GroupTransform::format_extension_by_index(i) {
                transform_file_format = GroupTransform::format_name_by_index(i).to_string();
                break;
            }
        }
    }

    if transform_file_format.is_empty() {
        return fail(&format!(
            "Could not find a valid format from the extension of: '{filepath}'. {formats}"
        ));
    } else if verbose {
        println!();
        println!("File format being used: {transform_file_format}");
    }

    println!();

    // Checking for an input colorspace or input (display, view) pair.
    if input_cs.is_empty() && (display.is_empty() || view.is_empty()) {
        return fail("Colorspaces or (display,view) pair must be specified as source.");
    }

    if env.is_none() {
        return fail("Missing the ${OCIO} env. variable.");
    }

    if verbose {
        println!();
        let pair = format!("({display}, {view})");
        let input_str = if input_cs.is_empty() {
            &pair
        } else {
            &input_cs
        };
        let output_str = if output_cs.is_empty() {
            &pair
        } else {
            &output_cs
        };
        println!("Processing from '{input_str}' to '{output_str}'");
    }

    let config = match Config::create_from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("OCIO Error: {e}");
            return ExitCode::from(1);
        }
    };

    if verbose {
        println!();
        print!(
            "Config: {} - version: {}",
            config.description(),
            config.major_version()
        );
        let minor = config.minor_version();
        if minor != 0 {
            print!(".{minor}");
        }
        println!();
    }

    // --colorspaces
    let use_colorspaces = !input_cs.is_empty() && !output_cs.is_empty();
    // --view
    let use_displayview = !input_cs.is_empty() && !display.is_empty() && !view.is_empty();
    // --invertview
    let use_invertview = !display.is_empty() && !view.is_empty() && !output_cs.is_empty();

    // Exactly one of the three options must be used.
    let num_used = [use_colorspaces, use_displayview, use_invertview]
        .iter()
        .filter(|u| **u)
        .count();
    if num_used != 1 {
        return fail("Any combinations of --colorspaces, --view or --invertview is invalid.");
    }

    let processor: ocio::Result<Processor> = if use_colorspaces {
        // Color space to color space.
        config.get_processor(&input_cs, &output_cs)
    } else if use_displayview {
        // Color space to (display, view) pair.
        config.get_display_view_processor_dir(
            &input_cs,
            &display,
            &view,
            TransformDirection::Forward,
        )
    } else {
        // (display, view) pair to color space.
        config.get_display_view_processor_dir(
            &output_cs,
            &display,
            &view,
            TransformDirection::Inverse,
        )
    };

    let processor = match processor {
        Ok(p) => p,
        Err(e) => {
            eprintln!("OCIO Error: {e}");
            return ExitCode::from(1);
        }
    };

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&filepath);
    let mut file = match file {
        Ok(f) => f,
        Err(_) => return fail(&format!("Could not open file: {filepath}")),
    };

    let group = processor.create_group_transform();
    match group.write(&config, &transform_file_format) {
        Ok(text) => {
            use std::io::Write;
            if let Err(e) = file.write_all(text.as_bytes()) {
                eprintln!("OCIO Error: {e}");
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            eprintln!("OCIO Error: {e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
