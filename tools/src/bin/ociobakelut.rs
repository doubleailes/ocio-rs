//! ociobakelut -- create a new LUT from an OCIO config or LUT file(s) (port
//! of the OCIO `ociobakelut` app).
//!
//! The ICC profile output of the C++ app (which relies on LittleCMS) is not
//! available: the ICC options are accepted but `--format icc` is an error.

use ocio::config::transform_display::format_transform;
use ocio::config::ColorSpace;
use ocio::{
    Baker, CdlTransform, ColorSpaceDirection, Config, Error, FileTransform, GroupTransform,
    Interpolation, ReferenceSpaceType, Transform, TransformDirection,
};
use ocio_tools::argparse::{atof, ArgParse};
use std::io::Write;
use std::process::ExitCode;

/// Build the transforms of the config-free LUT baking options, in the order
/// of the command line (port of `parse_luts`).
fn parse_luts(argv: &[String]) -> Result<GroupTransform, Error> {
    let mut group = GroupTransform::new();
    let mut last_ccc_id: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let need = |n: usize, name: &str| -> Result<(), Error> {
            if i + n >= argv.len() {
                Err(Error::msg(format!(
                    "Error parsing --{name}. Invalid num args"
                )))
            } else {
                Ok(())
            }
        };
        let triple = |scale: f64| -> [f64; 3] {
            [
                atof(&argv[i + 1]) / scale,
                atof(&argv[i + 2]) / scale,
                atof(&argv[i + 3]) / scale,
            ]
        };

        match arg {
            "--lut" | "-lut" => {
                need(1, "lut")?;
                let mut t = FileTransform::new(&argv[i + 1]);
                t.interpolation = Interpolation::Best;
                if let Some(id) = &last_ccc_id {
                    t.ccc_id = id.clone();
                }
                group.append(t);
                i += 1;
            }
            "--cccid" | "-cccid" => {
                need(1, "cccid")?;
                last_ccc_id = Some(argv[i + 1].clone());
                i += 1;
            }
            "--invlut" | "-invlut" => {
                need(1, "invlut")?;
                let mut t = FileTransform::new(&argv[i + 1]);
                t.interpolation = Interpolation::Best;
                t.direction = TransformDirection::Inverse;
                // Deviation: OCIO's ociobakelut ignores --cccid for --invlut,
                // although the option applies to "any following LUTs".
                if let Some(id) = &last_ccc_id {
                    t.ccc_id = id.clone();
                }
                group.append(t);
                i += 1;
            }
            "--slope" | "-slope" => {
                need(3, "slope")?;
                let mut t = CdlTransform::new();
                t.slope = triple(1.0);
                group.append(t);
                i += 3;
            }
            "--offset" | "-offset" => {
                need(3, "offset")?;
                let mut t = CdlTransform::new();
                t.offset = triple(1.0);
                group.append(t);
                i += 3;
            }
            "--offset10" | "-offset10" => {
                need(3, "offset10")?;
                let mut t = CdlTransform::new();
                t.offset = triple(1023.0);
                group.append(t);
                i += 3;
            }
            "--power" | "-power" => {
                need(3, "power")?;
                let mut t = CdlTransform::new();
                t.power = triple(1.0);
                group.append(t);
                i += 3;
            }
            "--sat" | "-sat" => {
                need(1, "sat")?;
                let mut t = CdlTransform::new();
                t.sat = atof(&argv[i + 1]);
                group.append(t);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    Ok(group)
}

fn fail(msg: &str) -> ExitCode {
    eprint!("{msg}");
    eprintln!("See --help for more info.");
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();

    // What are the allowed baker output formats?
    let formats = (0..Baker::num_formats())
        .map(|i| {
            format!(
                "{} (.{})",
                Baker::format_name_by_index(i).unwrap_or(""),
                Baker::format_extension_by_index(i).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let formatstr = format!("the LUT format to bake: {formats}");

    let mut ap = ArgParse::new(
        "ociobakelut -- create a new LUT or ICC profile from an OCIO config or LUT file(s)\n\n\
         usage:  ociobakelut [options] <OUTPUTFILE.LUT>\n\n\
         example:  ociobakelut --inputspace lg10 --outputspace srgb8 --format flame lg_to_srgb.3dl\n\
         example:  ociobakelut --lut filmlut.3dl --lut calibration.3dl --format flame display.3dl\n\
         example:  ociobakelut --cccid 0 --lut cdlgrade.ccc --lut calibration.3dl --format flame graded_display.3dl\n\
         example:  ociobakelut --lut look.3dl --offset 0.01 -0.02 0.03 --lut display.3dl --format flame display_with_look.3dl\n\
         example:  ociobakelut --inputspace lg10 --outputspace srgb8 --format icc ~/Library/ColorSync/Profiles/test.icc\n\
         example:  ociobakelut --inputspace lin --shaperspace lg10 --outputspace lg10 --format spi1d lintolog.spi1d\n\
         example:  ociobakelut --inputspace lg10 --displayview sRGB Film --format spi3d display_view.spi3d\n\
         example:  ociobakelut --lut filmlut.3dl --lut calibration.3dl --format icc ~/Library/ColorSync/Profiles/test.icc\n\n",
    )
    .positional("")
    .separator("Using Existing OCIO Configurations")
    .separator("    (use either displayview or outputspace, but not both)")
    .option("--inputspace %s", &["inputspace"], "Input OCIO ColorSpace (or Role)")
    .option(
        "--displayview %s %s",
        &["display", "view"],
        "Output OCIO Display and View",
    )
    .option("--outputspace %s", &["outputspace"], "Output OCIO ColorSpace (or Role)")
    .option(
        "--shaperspace %s",
        &["shaperspace"],
        "the OCIO ColorSpace or Role, for the shaper",
    )
    .option("--looks %s", &["looks"], "the OCIO looks to apply")
    .option(
        "--iconfig %s",
        &["iconfig"],
        "Input .ocio configuration file (default: $OCIO)\n",
    )
    .separator("Config-Free LUT Baking")
    .separator("    (all options can be specified multiple times, each is applied in order)")
    .option("--cccid %s", &["dummystr"], "Specify a CCCId for any following LUTs")
    .option("--lut %s", &["dummystr"], "Specify a LUT (forward direction)")
    .option("--invlut %s", &["dummystr"], "Specify a LUT (inverse direction)")
    .option("--slope %f %f %f", &["d1", "d2", "d3"], "slope")
    .option("--offset %f %f %f", &["d1", "d2", "d3"], "offset (float)")
    .option("--offset10 %f %f %f", &["d1", "d2", "d3"], "offset (10-bit)")
    .option("--power %f %f %f", &["d1", "d2", "d3"], "power")
    .option(
        "--sat %f",
        &["d1"],
        "saturation (ASC-CDL luma coefficients)\n",
    )
    .separator("Baking Options")
    .option("--format %s", &["format"], &formatstr)
    .option(
        "--shapersize %d",
        &["shapersize"],
        "size of the shaper (default: format specific)",
    )
    .option(
        "--cubesize %d",
        &["cubesize"],
        "size of the main LUT (3d or 1d) (default: format specific)",
    )
    .flag("--stdout", "stdout", "Write to stdout (rather than file)")
    .flag("--v", "verbose", "Verbose")
    .flag("--help", "help", "Print help message\n")
    .separator("ICC Options")
    .option(
        "--whitepoint %d",
        &["whitepoint"],
        "whitepoint for the profile (default: 6505)",
    )
    .option(
        "--displayicc %s",
        &["displayicc"],
        "an ICC profile which matches the OCIO profiles target display",
    )
    .option(
        "--description %s",
        &["description"],
        "a meaningful description, this will show up in UI like photoshop (defaults to \"filename.icc\")",
    )
    .option(
        "--copyright %s",
        &["copyright"],
        "a copyright field added in the file (default: \"No copyright. Use freely.\")\n",
    );

    if ap.parse(&argv).is_err() {
        println!("{}", ap.geterror());
        ap.print_usage();
        println!();
        return ExitCode::from(1);
    }

    if ap.get_bool("help") || argv.len() == 1 {
        ap.print_usage();
        println!();
        return ExitCode::from(1);
    }

    let outputfile = ap.args().last().cloned().unwrap_or_default();
    let format = ap.get_string("format", "");
    let mut inputspace = ap.get_string("inputspace", "");
    let shaperspace = ap.get_string("shaperspace", "");
    let looks = ap.get_string("looks", "");
    let mut outputspace = ap.get_string("outputspace", "");
    let display = ap.get_string("display", "");
    let view = ap.get_string("view", "");
    let inputconfig = ap.get_string("iconfig", "");
    let cubesize = ap.get_int("cubesize", -1);
    let shapersize = ap.get_int("shapersize", -1);
    let usestdout = ap.get_bool("stdout");
    // If we're printing to stdout, disable verbose printouts.
    let verbose = ap.get_bool("verbose") && !usestdout;

    let group = match parse_luts(&argv) {
        Ok(g) => g,
        Err(e) => return fail(&format!("\nERROR: {e}\n")),
    };

    // If --luts have been specified, synthesize a new (temporary) config
    // with the transformation embedded in a colorspace.
    let config: Config = if group.num_transforms() > 0 {
        if !inputspace.is_empty() {
            return fail("\nERROR: --inputspace is not allowed when using --lut\n\n");
        }
        if !outputspace.is_empty() {
            return fail("\nERROR: --outputspace is not allowed when using --lut\n\n");
        }
        if !looks.is_empty() {
            return fail("\nERROR: --looks is not allowed when using --lut\n\n");
        }
        if !shaperspace.is_empty() {
            return fail("\nERROR: --shaperspace is not allowed when using --lut\n\n");
        }
        if !display.is_empty() || !view.is_empty() {
            return fail("\nERROR: --displayview is not allowed when using --lut\n\n");
        }

        let mut editable = Config::create();

        inputspace = "RawInput".to_string();
        let mut input_cs = ColorSpace::new(ReferenceSpaceType::Scene);
        input_cs.set_name(&inputspace);

        outputspace = "ProcessedOutput".to_string();
        let mut output_cs = ColorSpace::new(ReferenceSpaceType::Scene);
        output_cs.set_name(&outputspace);
        output_cs.set_transform(
            Some(Transform::Group(group.clone())),
            ColorSpaceDirection::FromReference,
        );

        if verbose {
            print!("[OpenColorIO DEBUG]: Specified Transform:");
            print!("{}", format_transform(&Transform::Group(group.clone())));
            println!();
        }

        if let Err(e) = editable
            .add_color_space(&input_cs)
            .and_then(|_| editable.add_color_space(&output_cs))
        {
            return fail(&format!("OCIO Error: {e}\n"));
        }
        editable
    } else {
        if inputspace.is_empty() {
            return fail("\nERROR: You must specify the --inputspace.\n\n");
        }
        if outputspace.is_empty() && display.is_empty() && view.is_empty() {
            return fail("\nERROR: You must specify either --outputspace or --displayview.\n\n");
        }
        if display.is_empty() ^ view.is_empty() {
            return fail("\nERROR: You must specify both display and view with --displayview.\n\n");
        }
        if format.is_empty() {
            return fail("\nERROR: You must specify the LUT format using --format.\n\n");
        }

        let loaded = if !inputconfig.is_empty() {
            if verbose {
                println!("[OpenColorIO INFO]: Loading {inputconfig}");
            }
            Config::create_from_file(&inputconfig)
        } else if let Some(env) = ocio_tools::env_variable("OCIO") {
            if verbose {
                println!("[OpenColorIO INFO]: Loading $OCIO {env}");
            }
            Config::create_from_env()
        } else {
            eprint!("ERROR: You must specify an input OCIO configuration ");
            eprint!("(either with --iconfig or $OCIO).\n\n");
            ap.print_usage();
            return ExitCode::from(1);
        };
        match loaded {
            Ok(c) => c,
            Err(e) => return fail(&format!("OCIO Error: {e}\n")),
        }
    };

    if outputfile.is_empty() && !usestdout {
        return fail("\nERROR: You must specify the outputfile or --stdout.\n\n");
    }

    if format == "icc" {
        return fail(
            "\nERROR: ICC profile output is not supported by this version of ociobakelut.\n\n",
        );
    }

    let mut baker = Baker::new();

    // Setup the baker for our LUT type.
    baker.set_config(&config);
    if let Err(e) = baker.set_format(&format) {
        return fail(&format!("OCIO Error: {e}\n"));
    }
    baker.set_input_space(&inputspace);
    baker.set_shaper_space(&shaperspace);
    baker.set_looks(&looks);
    baker.set_target_space(&outputspace);
    baker.set_display_view(&display, &view);
    if shapersize != -1 {
        baker.set_shaper_size(usize::try_from(shapersize).ok());
    }
    if cubesize != -1 {
        baker.set_cube_size(usize::try_from(cubesize).ok());
    }

    if verbose {
        println!("[OpenColorIO INFO]: Baking '{format}' LUT");
    }

    if usestdout {
        match baker.bake() {
            Ok(data) => {
                let mut out = std::io::stdout();
                if out.write_all(&data).and_then(|_| out.flush()).is_err() {
                    return fail("Error: could not write to stdout\n");
                }
            }
            Err(e) => return fail(&format!("OCIO Error: {e}\n")),
        }
    } else {
        let mut file = match std::fs::File::create(&outputfile) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("ERROR: Non-writable file path {outputfile} specified.");
                return ExitCode::from(1);
            }
        };
        match baker.bake() {
            Ok(data) => {
                if let Err(e) = file.write_all(&data) {
                    return fail(&format!("Error: {e}\n"));
                }
            }
            Err(e) => return fail(&format!("OCIO Error: {e}\n")),
        }
        if verbose {
            println!("[OpenColorIO INFO]: Wrote '{outputfile}'");
        }
    }

    ExitCode::SUCCESS
}
