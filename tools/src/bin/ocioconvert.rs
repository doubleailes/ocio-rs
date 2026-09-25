//! ocioconvert -- apply a color space transform to an image (port of the OCIO
//! `ocioconvert` app).
//!
//! Images are read and written with the pure Rust image I/O of
//! `ocio_tools::imageio` (OpenEXR, PNG, TIFF, JPEG). The GPU options are
//! recognized but, as this port has no GPU support, they produce the same
//! error as an OCIO build without OpenGL.

use ocio::{
    BitDepth, Config, DisplayViewTransform, Error, FileTransform, Interpolation, OptimizationFlags,
    Processor, Transform, TransformDirection,
};
use ocio_tools::argparse::ArgParse;
use ocio_tools::imageio::{AttributeValue, ImageIO};
use std::process::ExitCode;
use std::time::Instant;

/// Split a `name=value` pair.
fn parse_name_value_pair(input: &str) -> Option<(String, String)> {
    let pos = input.find('=')?;
    Some((input[..pos].to_string(), input[pos + 1..].to_string()))
}

/// Parse the first white space separated token of `s` as a C++ stream would
/// (the longest valid prefix), failing if there is none.
fn stream_parse<T: std::str::FromStr>(s: &str) -> Option<T> {
    let t = s.trim_start();
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    let t = &t[..end];
    (1..=t.len())
        .rev()
        .filter(|i| t.is_char_boundary(*i))
        .find_map(|i| t[..i].parse::<T>().ok())
}

fn string_to_float(s: &str) -> Option<f32> {
    stream_parse::<f32>(s)
}

fn string_to_int(s: &str) -> Option<i32> {
    stream_parse::<i32>(s)
}

fn usage_error(ap: &ArgParse, msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ap.print_usage();
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ocioconvert -- apply colorspace transform to an image \n\n\
         usage: ocioconvert [options] inputimage inputcolorspace outputimage outputcolorspace\n\
         \x20  or: ocioconvert [options] --lut lutfile inputimage outputimage\n\
         \x20  or: ocioconvert [options] --view inputimage inputcolorspace outputimage displayname viewname\n\
         \x20  or: ocioconvert [options] --invertview inputimage displayname viewname outputimage outputcolorspace\n\
         \x20  or: ocioconvert [options] --namedtransform transformname inputimage outputimage\n\
         \x20  or: ocioconvert [options] --invnamedtransform transformname inputimage outputimage\n\n",
    )
    .positional("")
    .separator("Options:")
    .flag("--lut", "lut", "Convert using a LUT rather than a config file")
    .flag(
        "--view",
        "view",
        "Convert to a (display,view) pair rather than to an output color space",
    )
    .flag(
        "--invertview",
        "invertview",
        "Convert from a (display,view) pair rather than from a color space",
    )
    .flag(
        "--namedtransform",
        "namedtransform",
        "Convert using a named transform in the forward direction",
    )
    .flag(
        "--invnamedtransform",
        "invnamedtransform",
        "Convert using a named transform in the inverse direction",
    )
    .flag(
        "--gpu",
        "gpu",
        "Use GPU color processing instead of CPU (CPU is the default)",
    )
    .flag(
        "--gpulegacy",
        "gpulegacy",
        "Use the legacy (i.e. baked) GPU color processing instead of the CPU one (--gpu is ignored)",
    )
    .flag("--gpuinfo", "gpuinfo", "Output the OCIO shader program")
    .flag("--h", "help", "Display the help and exit")
    .flag("--help", "help", "Display the help and exit")
    .flag("-v", "verbose", "Display general information")
    .option(
        "--iconfig %s",
        &["iconfig"],
        "Input .ocio configuration file (default: $OCIO)",
    )
    .separator("\nOpenImageIO or OpenEXR options:")
    .option("--bitdepth %s", &["bitdepth"], "Output image bitdepth")
    .option(
        "--float-attribute %L",
        &["floatattrs"],
        "\"name=float\" pair defining OIIO float attribute for outputimage",
    )
    .option(
        "--int-attribute %L",
        &["intattrs"],
        "\"name=int\" pair defining an int attribute for outputimage",
    )
    .option(
        "--string-attribute %L",
        &["stringattrs"],
        "\"name=string\" pair defining a string attribute for outputimage",
    );

    if ap.parse(&argv).is_err() {
        eprintln!("{}", ap.geterror());
        ap.print_usage();
        return ExitCode::from(1);
    }

    if ap.get_bool("help") {
        ap.print_usage();
        return ExitCode::SUCCESS;
    }

    if ap.get_bool("gpu") || ap.get_bool("gpuinfo") || ap.get_bool("gpulegacy") {
        eprintln!("Compiled without OpenGL support, GPU options are not available.");
        return ExitCode::from(1);
    }

    let verbose = ap.get_bool("verbose");
    let use_lut = ap.get_bool("lut");
    let use_display_view = ap.get_bool("view");
    let use_invert_view = ap.get_bool("invertview");
    let use_named_transform = ap.get_bool("namedtransform");
    let use_inv_named_transform = ap.get_bool("invnamedtransform");
    let mut inputconfig = ap.get_string("iconfig", "");

    let output_depth = ap.get_string("bitdepth", "");
    let user_output_bit_depth = match output_depth.as_str() {
        "" => BitDepth::Unknown,
        "uint8" => BitDepth::UInt8,
        "uint16" => BitDepth::UInt16,
        "half" => BitDepth::F16,
        "float" => BitDepth::F32,
        _ => {
            return usage_error(
                &ap,
                "ERROR: Unsupported output bitdepth, must be uint8, uint16, half or float.",
            )
        }
    };

    let args = ap.args().to_vec();
    let arg = |i: usize| args[i].clone();

    let inputimage: String;
    let mut inputcolorspace = String::new();
    let outputimage: String;
    let mut outputcolorspace: Option<String> = None;
    let mut lut_file = String::new();
    let mut display = String::new();
    let mut view = String::new();
    let mut namedtransform = String::new();

    // The transform modes are exclusive. Deviation: OCIO has these checks
    // too, but most of them are unreachable in its mode selection (e.g.
    // `--lut --invertview` silently ignores `--invertview`).
    let conflict = if use_lut && use_display_view {
        Some("ERROR: Options lut & view can't be used at the same time.")
    } else if use_display_view && use_invert_view {
        Some("ERROR: Options view & invertview can't be used at the same time.")
    } else if use_lut && use_invert_view {
        Some("ERROR: Options lut & invertview can't be used at the same time.")
    } else if use_named_transform
        && (use_lut || use_display_view || use_invert_view || use_inv_named_transform)
    {
        Some(
            "ERROR: Option namedtransform can't be used with lut, view, invertview, \
             or invnamedtransform at the same time.",
        )
    } else if use_inv_named_transform && (use_lut || use_display_view || use_invert_view) {
        Some(
            "ERROR: Option invnamedtransform can't be used with lut, view, invertview, \
             or namedtransform at the same time.",
        )
    } else {
        None
    };
    if let Some(msg) = conflict {
        return usage_error(&ap, msg);
    }

    if !use_lut
        && !use_display_view
        && !use_invert_view
        && !use_named_transform
        && !use_inv_named_transform
    {
        if args.len() != 4 {
            return usage_error(
                &ap,
                &format!("ERROR: Expecting 4 arguments, found {}.", args.len()),
            );
        }
        inputimage = arg(0);
        inputcolorspace = arg(1);
        outputimage = arg(2);
        outputcolorspace = Some(arg(3));
    } else if use_lut {
        if args.len() != 3 {
            return usage_error(
                &ap,
                &format!(
                    "ERROR: Expecting 3 arguments for --lut option, found {}.",
                    args.len()
                ),
            );
        }
        lut_file = arg(0);
        inputimage = arg(1);
        outputimage = arg(2);
    } else if use_display_view {
        if args.len() != 5 {
            return usage_error(
                &ap,
                &format!(
                    "ERROR: Expecting 5 arguments for --view option, found {}.",
                    args.len()
                ),
            );
        }
        inputimage = arg(0);
        inputcolorspace = arg(1);
        outputimage = arg(2);
        display = arg(3);
        view = arg(4);
    } else if use_invert_view {
        if args.len() != 5 {
            return usage_error(
                &ap,
                &format!(
                    "ERROR: Expecting 5 arguments for --invertview option, found {}.",
                    args.len()
                ),
            );
        }
        inputimage = arg(0);
        display = arg(1);
        view = arg(2);
        outputimage = arg(3);
        outputcolorspace = Some(arg(4));
    } else if use_named_transform {
        if args.len() != 3 {
            return usage_error(
                &ap,
                &format!(
                    "ERROR: Expecting 3 arguments for --namedtransform option, found {}.",
                    args.len()
                ),
            );
        }
        namedtransform = arg(0);
        inputimage = arg(1);
        outputimage = arg(2);
    } else {
        if args.len() != 3 {
            return usage_error(
                &ap,
                &format!(
                    "ERROR: Expecting 3 arguments for --invnamedtransform option, found {}.",
                    args.len()
                ),
            );
        }
        namedtransform = arg(0);
        inputimage = arg(1);
        outputimage = arg(2);
    }

    // Load the current config.
    let config = if use_lut {
        Ok(Config::create_raw())
    } else if !inputconfig.is_empty() {
        Config::create_from_file(&inputconfig)
    } else {
        if let Some(env) = ocio_tools::env_variable("OCIO").filter(|e| !e.is_empty()) {
            inputconfig = env;
        }
        Config::create_from_env()
    };
    let config = match config {
        Ok(c) => c,
        Err(e) => {
            println!("ERROR loading config file: {e}");
            return ExitCode::from(1);
        }
    };

    if verbose {
        println!();
        println!("{}", ImageIO::version());
        println!("OCIO Version: {}", ocio_tools::version());
        if !use_lut {
            println!();
            println!("OCIO Config. file:    '{inputconfig}'");
            println!(
                "OCIO Config. version: {}.{}",
                config.major_version(),
                config.minor_version()
            );
            println!("OCIO search_path:     {}", config.search_path());
        }
    }

    // Load the image.
    println!();
    println!("Loading {inputimage}");
    let mut img_input = match ImageIO::open(&inputimage) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("ERROR: Loading file failed: {e}");
            return ExitCode::from(1);
        }
    };
    println!("{}", img_input.image_desc_str());

    // Get the processor.
    let processor: ocio::Result<Processor> = if use_lut {
        // Create the OCIO processor for the specified transform.
        let mut t = FileTransform::new(&lut_file);
        t.interpolation = Interpolation::Best;
        config.get_processor_for_transform(&Transform::File(t), TransformDirection::Forward)
    } else if use_display_view {
        let t = DisplayViewTransform::new(&inputcolorspace, &display, &view);
        config.get_processor_for_transform(&Transform::DisplayView(t), TransformDirection::Forward)
    } else if use_invert_view {
        let t =
            DisplayViewTransform::new(outputcolorspace.as_deref().unwrap_or(""), &display, &view);
        config.get_processor_for_transform(&Transform::DisplayView(t), TransformDirection::Inverse)
    } else if use_named_transform || use_inv_named_transform {
        let dir = if use_named_transform {
            TransformDirection::Forward
        } else {
            TransformDirection::Inverse
        };
        match config.get_named_transform(&namedtransform) {
            Some(nt) => config.get_processor_for_named_transform(nt, dir),
            None => {
                println!("ERROR: Could not get NamedTransform {namedtransform}");
                return ExitCode::from(1);
            }
        }
    } else {
        config.get_processor(&inputcolorspace, outputcolorspace.as_deref().unwrap_or(""))
    };

    let processor = match processor {
        Ok(p) => p,
        Err(e) => {
            println!("ERROR: OCIO failed with: {e}");
            return ExitCode::from(1);
        }
    };

    // Set the bit-depth of the output buffer.
    //
    // The CPU processor can be optimized for a specific input and output
    // bit-depth. The converted image may require more bits than the source
    // image. For example, converting a log image to linear requires at least
    // a half-float output format. For most cases, half-float strikes a good
    // balance between precision and storage space. But if the input depth
    // would lose precision when converted to half-float, use float for the
    // output depth instead.
    let input_bit_depth = img_input.bit_depth();
    let output_bit_depth = if user_output_bit_depth != BitDepth::Unknown {
        user_output_bit_depth
    } else {
        match input_bit_depth {
            BitDepth::UInt16 | BitDepth::F32 => BitDepth::F32,
            BitDepth::UInt8 | BitDepth::F16 => BitDepth::F16,
            _ => {
                eprintln!(
                    "ERROR: OCIO failed with: Unsupported input bitdepth, must be uint8, uint16, half or float."
                );
                return ExitCode::from(1);
            }
        }
    };

    let cpu = processor.optimized_cpu_processor_with_bit_depths(
        input_bit_depth,
        output_bit_depth,
        OptimizationFlags::DEFAULT,
    );

    let use_output_buffer = input_bit_depth != output_bit_depth;

    let start = Instant::now();

    let mut img_output_cpu = ImageIO::default();
    let result: ocio::Result<()> = if use_output_buffer {
        img_output_cpu
            .init_from(&img_input, output_bit_depth)
            .and_then(|_| {
                let src = img_input.image_desc()?;
                let mut dst = img_output_cpu.image_desc()?;
                cpu.apply_src_dst(&src, &mut dst)
            })
    } else {
        img_input.image_desc().map(|mut desc| cpu.apply(&mut desc))
    };
    if let Err(e) = result {
        eprintln!("ERROR: OCIO failed with: {e}");
        return ExitCode::from(1);
    }

    if verbose {
        println!();
        println!(
            "CPU processing took: {} ms",
            ocio_tools::fmt_default(f64::from(ocio_tools::millis(start.elapsed())))
        );
    }

    // Default is to perform in-place conversion.
    let img_output = if use_output_buffer {
        &mut img_output_cpu
    } else {
        &mut img_input
    };

    // Set the provided image attributes.
    let mut parse_error = false;
    for attr in ap.get_list("floatattrs") {
        match parse_name_value_pair(&attr).and_then(|(n, v)| string_to_float(&v).map(|f| (n, f))) {
            Some((name, value)) => img_output.attribute(&name, AttributeValue::Float(value)),
            None => {
                eprintln!(
                    "ERROR: Attribute string '{attr}' should be in the form name=floatvalue."
                );
                parse_error = true;
            }
        }
    }
    for attr in ap.get_list("intattrs") {
        match parse_name_value_pair(&attr).and_then(|(n, v)| string_to_int(&v).map(|i| (n, i))) {
            Some((name, value)) => img_output.attribute(&name, AttributeValue::Int(value)),
            None => {
                eprintln!("ERROR: Attribute string '{attr}' should be in the form name=intvalue.");
                parse_error = true;
            }
        }
    }
    for attr in ap.get_list("stringattrs") {
        match parse_name_value_pair(&attr) {
            Some((name, value)) => img_output.attribute(&name, AttributeValue::Str(value)),
            None => {
                eprintln!("ERROR: Attribute string '{attr}' should be in the form name=value.");
                parse_error = true;
            }
        }
    }
    if parse_error {
        return ExitCode::from(1);
    }

    // Write out the result.
    if use_display_view {
        outputcolorspace = Some(
            config
                .display_view_color_space_name(&display, &view)
                .to_string(),
        );
    }

    if let Some(out_cs) = &outputcolorspace {
        img_output.attribute("oiio:ColorSpace", AttributeValue::Str(out_cs.clone()));

        // Set the color space interop id if available.
        if let Some(cs) = config.get_color_space(out_cs) {
            let interop_id = cs.interop_id();
            if !interop_id.is_empty() {
                img_output.attribute(
                    "colorInteropID",
                    AttributeValue::Str(interop_id.to_string()),
                );
            }
        }
    }

    let written: Result<(), Error> = img_output.write(&outputimage, user_output_bit_depth);
    if written.is_err() {
        eprintln!("ERROR: Writing file \"{outputimage}\".");
        return ExitCode::from(1);
    }

    println!("Wrote {outputimage}");
    println!("{}", img_output.image_desc_str());

    ExitCode::SUCCESS
}
