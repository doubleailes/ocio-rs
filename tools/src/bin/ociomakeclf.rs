//! ociomakeclf -- convert a LUT into CLF format, optionally making it an ACES
//! LMT (port of the OCIO `ociomakeclf` app).

use ocio::builtins::registry::BuiltinTransformRegistry;
use ocio::config::utils::compare;
use ocio::fileformats::FILEFORMAT_CLF;
use ocio::{
    BuiltinTransform, Config, Error, FileTransform, GroupTransform, Interpolation,
    OptimizationFlags, Result, Transform, TransformDirection, METADATA_DESCRIPTION,
    METADATA_ID_ELEMENT, METADATA_INPUT_DESCRIPTOR, METADATA_OUTPUT_DESCRIPTOR,
};
use ocio_tools::argparse::ArgParse;
use ocio_tools::Measure;
use std::process::ExitCode;

/// The LMT must accept and produce ACES2065-1 so look for all the built-in
/// transforms that produce that (based on the naming conventions).
const BUILTIN_SUFFIX: &str = "_to_ACES2065-1";

/// Format the processor cache identifier as a UUID (8-4-4-4-12 form), as
/// the OCIO processor cache identifiers are. The identifiers of this port
/// are MD5 hashes (not always hexadecimal, e.g. for no-op processors) so they
/// are hashed again when needed.
fn cache_id_uuid(cache_id: &str) -> String {
    let hex = if cache_id.len() == 32 && cache_id.chars().all(|c| c.is_ascii_hexdigit()) {
        cache_id.to_ascii_lowercase()
    } else {
        format!("{:x}", md5::compute(cache_id.as_bytes()))
    };
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn create_output_lut_file(
    out_path: &str,
    transform: &GroupTransform,
    generate_id: bool,
) -> Result<()> {
    // Get a basic config to create a processor.
    let config = Config::create_raw();

    let processor = config.get_processor_for_transform(
        &Transform::Group(transform.clone()),
        TransformDirection::Forward,
    )?;

    // The CLF file format does not support inverse 1D LUTs, optimize the
    // processor to replace inverse 1D LUTs by 'fast forward' 1D LUTs.
    let opt_processor = processor.optimized(OptimizationFlags::LUT_INV_FAST);

    // Create the CLF file.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_path)
        .map_err(|_| Error::msg(format!("Could not open the file '{out_path}'.\n")))?;

    let write = || -> Result<String> {
        let mut group = opt_processor.create_group_transform();
        if generate_id {
            let id = format!("urn:uuid:{}", cache_id_uuid(&opt_processor.cache_id()));
            group.metadata.add_child_element(METADATA_ID_ELEMENT, &id);
        }
        group.write(&config, FILEFORMAT_CLF)
    };

    match write() {
        Ok(text) => {
            use std::io::Write;
            file.write_all(text.as_bytes())?;
            Ok(())
        }
        Err(e) => {
            drop(file);
            let _ = std::fs::remove_file(out_path);
            Err(e)
        }
    }
}

fn run(in_path: &str, out_path: &str, csc: &str, original_csc: &str, ap: &ArgParse) -> Result<()> {
    let verbose = ap.get_bool("verbose");
    let measure = ap.get_bool("measure");
    let generate_id = ap.get_bool("generateid");

    if verbose {
        println!("Building the transformation.");
    }

    let mut grp = GroupTransform::new();
    grp.direction = TransformDirection::Forward;

    if !csc.is_empty() {
        let description = format!(
            "ACES LMT transform built from a look LUT expecting color space: {original_csc}"
        );
        grp.metadata
            .add_child_element(METADATA_DESCRIPTION, &description);
    }

    let description = format!("Original LUT name: {in_path}");
    grp.metadata
        .add_child_element(METADATA_DESCRIPTION, &description);

    if !csc.is_empty() {
        // TODO: It should overwrite existing input and output descriptors if any.
        grp.metadata
            .add_child_element(METADATA_INPUT_DESCRIPTOR, "ACES2065-1");
        grp.metadata
            .add_child_element(METADATA_OUTPUT_DESCRIPTOR, "ACES2065-1");

        // Create the color transformation from ACES2065-1 to the CSC color space.
        let mut in_builtin = BuiltinTransform::new(csc);
        in_builtin.direction = TransformDirection::Inverse;
        grp.append(in_builtin);
    }

    // Create the file transform for the input LUT file.
    let mut file = FileTransform::new(in_path);
    file.direction = TransformDirection::Forward;
    file.interpolation = Interpolation::Best;
    grp.append(file);

    if !csc.is_empty() {
        // Create the color transformation from the CSC color space to ACES2065-1.
        let mut out_builtin = BuiltinTransform::new(csc);
        out_builtin.direction = TransformDirection::Forward;
        grp.append(out_builtin);
    }

    const MSG: &str = "Creating the CLF lut file";

    if verbose && !measure {
        println!("{MSG}.");
    }

    if measure {
        let mut m = Measure::new(MSG);
        m.resume()?;
        // Create the CLF file.
        create_output_lut_file(out_path, &grp, generate_id)
    } else {
        // Create the CLF file.
        create_output_lut_file(out_path, &grp, generate_id)
    }
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ociomakeclf -- Convert a LUT into CLF format and optionally add conversions from/to ACES2065-1 to make it an LMT.\n\
         \x20              The generated file should be compatible with both SMPTE ST 2136-1 as well as the previous\n\
         \x20              Academy/ASC v3 version.\n\
         \x20              If the csc argument is used, the CLF will contain the transforms:\n\
         \x20              [ACES2065-1 to CSC space] [the LUT] [CSC space to ACES2065-1].\n\n\
         usage: ociomakeclf inLutFilepath outLutFilepath --csc cscColorSpace\n\
         \x20 or   ociomakeclf inLutFilepath outLutFilepath\n\
         \x20 or   ociomakeclf --list\n",
    )
    .positional("")
    .separator("Options:")
    .flag("--help", "help", "Print help message")
    .flag("--verbose", "verbose", "Display general information")
    .flag("--measure", "measure", "Measure (in ms) the CLF write")
    .flag("--list", "list", "List of the supported CSC color spaces")
    .option(
        "--csc %s",
        &["csc"],
        "The color space that the input LUT expects and produces",
    )
    .flag(
        "--generateid",
        "generateid",
        "Generates an id based on content and writes in SMPTE Id element format",
    );

    if ap.parse(&argv).is_err() {
        eprintln!();
        eprintln!("{}", ap.geterror());
        eprintln!();
        ap.print_usage();
        return ExitCode::from(1);
    }

    if ap.get_bool("help") {
        ap.print_usage();
        return ExitCode::SUCCESS;
    }

    let registry = BuiltinTransformRegistry::get();

    if ap.get_bool("list") {
        print!("The list of supported color spaces converting to ACES2065-1, is:");
        for idx in 0..registry.num_builtins() {
            if let Ok(style) = registry.builtin_style(idx) {
                if let Some(name) = style.strip_suffix(BUILTIN_SUFFIX) {
                    print!("\n\t{name}");
                }
            }
        }
        println!();
        println!();
        return ExitCode::SUCCESS;
    }

    let args = ap.args().to_vec();
    if args.len() != 2 {
        eprintln!("ERROR: Expecting 2 arguments, found {}.", args.len());
        ap.print_usage();
        return ExitCode::from(1);
    }

    let in_path = args[0].clone();
    let out_path = args[1].clone();

    let original_csc = ap.get_string("csc", "");
    let mut csc = original_csc.clone();

    if !csc.is_empty() {
        csc.push_str(BUILTIN_SUFFIX);
        let found = (0..registry.num_builtins())
            .filter_map(|idx| registry.builtin_style(idx).ok())
            .find(|style| compare(&csc, style));
        match found {
            // Save the builtin transform name with the right cases.
            Some(style) => csc = style.to_string(),
            None => {
                eprintln!("ERROR: The LUT color space name '{original_csc}' is not supported.");
                return ExitCode::from(1);
            }
        }
    }

    if out_path.is_empty() {
        eprintln!("ERROR: The output file path is missing.");
        return ExitCode::from(1);
    } else if !out_path.to_lowercase().ends_with(".clf") {
        eprintln!("ERROR: The output LUT file path '{out_path}' must have a .clf extension.");
        return ExitCode::from(1);
    }

    if ap.get_bool("verbose") {
        println!("OCIO Version: {}", ocio_tools::version());
    }

    match run(&in_path, &out_path, &csc, &original_csc, &ap) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("OCIO ERROR: {e}");
            ExitCode::from(1)
        }
    }
}
