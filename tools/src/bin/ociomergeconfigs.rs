//! ociomergeconfigs -- merge configs using an OCIOM file with merge
//! parameters (port of the OCIO `ociomergeconfigs` app).

use ocio::apphelpers::ConfigMerger;
use ocio::config::logging::LogGuard;
use ocio_tools::argparse::ArgParse;
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ociomergeconfigs -- Merge configs using an OCIOM file with merge parameters\n\n\
         Usage:\n\
         \x20   ociomergeconfigs [options] mergeFile.ociom --out mergedConfig.ocio\n",
    )
    .positional("")
    .separator("Options:")
    .option("--out %s", &["out"], "Filepath to save the merged config")
    .flag("--validate", "validate", "Validate the final merged config")
    .flag(
        "--show-last",
        "showlast",
        "Display the last merged config to screen",
    )
    .flag(
        "--show-all",
        "showall",
        "Display ALL merged configs to screen",
    )
    .flag(
        "--show-params",
        "showparams",
        "Display merger options from OCIOM file",
    )
    .flag("--help", "help", "Display the help and exit")
    .flag("-h", "help", "Display the help and exit");

    if ap.parse(&argv).is_err() {
        eprintln!("{}", ap.geterror());
        ap.print_usage();
        return ExitCode::from(1);
    } else if ap.args().len() != 1 {
        eprintln!("ERROR: Expecting 1 arguments, found {}.", ap.args().len());
        ap.print_usage();
        return ExitCode::from(1);
    }

    let merge_parameters = ap.args()[0].clone();

    if ap.get_bool("help") {
        ap.print_usage();
        return ExitCode::SUCCESS;
    }

    // Load the options from the OCIOM file.
    let merger = match ConfigMerger::create_from_file(&merge_parameters) {
        Ok(m) => m,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };

    let new_merger = match merger.merge_configs() {
        Ok(m) => m,
        Err(e) => {
            eprint!("{e}");
            return ExitCode::from(1);
        }
    };

    let merged = match new_merger.merged_config() {
        Some(c) => c,
        None => {
            eprint!("No merged config.");
            return ExitCode::from(1);
        }
    };

    if ap.get_bool("validate") {
        // The validation log messages are captured (and discarded).
        let _guard = LogGuard::new();
        if let Err(e) = merged.validate() {
            drop(_guard);
            println!("{e}");
            return ExitCode::from(1);
        }
    }

    let serialize = |c: &ocio::Config| -> Result<String, ExitCode> {
        c.serialize().map_err(|e| {
            eprint!("{e}");
            ExitCode::from(1)
        })
    };

    if ap.get_bool("showparams") {
        println!("********************");
        println!("Merger options");
        println!("********************");
        match new_merger.serialize() {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprint!("{e}");
                return ExitCode::from(1);
            }
        }
        println!();
    }

    // The "show-all" option takes priority over the "show-last" option.
    let show_all = ap.get_bool("showall");
    if show_all {
        for i in 0..merger.num_config_merging_parameters() {
            println!("*********************");
            println!("Merged Config {i}");
            println!("*********************");
            if let Some(c) = new_merger.merged_config_at(i) {
                match serialize(c) {
                    Ok(text) => println!("{text}"),
                    Err(code) => return code,
                }
            }
        }
    }

    if ap.get_bool("showlast") && !show_all {
        println!("********************");
        println!("Last Merged Config");
        println!("********************");
        match serialize(merged) {
            Ok(text) => println!("{text}"),
            Err(code) => return code,
        }
    }

    let output_file = ap.get_string("out", "");
    if !output_file.is_empty() {
        let text = match serialize(merged) {
            Ok(t) => t,
            Err(code) => return code,
        };
        if let Err(e) = std::fs::write(ocio::path_utils::absolute(&output_file), text) {
            eprint!("{e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
