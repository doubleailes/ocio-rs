//! ociochecklut -- check any LUT file and optionally convert a pixel (port
//! of the OCIO `ociochecklut` app).
//!
//! The GPU options are recognized but, as this port has no GPU support, they
//! produce the same error as an OCIO build without OpenGL.

use ocio::config::logging::{logging_level, set_logging_level, LogGuard};
use ocio::config::transform_display::format_transform;
use ocio::{
    Config, CpuProcessor, FileTransform, Interpolation, LoggingLevel, Transform, TransformDirection,
};
use ocio_tools::argparse::{strtof, ArgParse};
use ocio_tools::fmt_float;
use std::process::ExitCode;

const DESC_STRING: &str = "\n\
OCIOCHECKLUT loads any LUT type supported by OCIO and prints any errors\n\
encountered.  Provide a normalized RGB or RGBA value to send that through\n\
the LUT.  Alternatively use the -t option to evaluate a set of test values.\n\
Otherwise, if no RGB value is provided, a list of the operators in the LUT is printed.\n\
Use -v to print warnings while parsing the LUT.\n";

/// The OCIO log messages are printed to stdout (the C++ app installs a
/// custom logging function doing so).
fn flush_log(guard: &LogGuard) {
    let out = guard.output();
    if !out.is_empty() {
        print!("{out}");
        guard.clear();
    }
}

fn to_string(v: f32) -> String {
    fmt_float(f64::from(v), 7)
}

fn to_strings(vec: &[f32], index: usize, comp: usize) -> Vec<String> {
    (0..comp).map(|i| to_string(vec[index + i])).collect()
}

/// Print the components of `strs`, right aligned on the widths of `align`.
fn aligned_vec(strs: &[String], align: &[String]) -> String {
    strs.iter()
        .zip(align.iter())
        .map(|(s, a)| {
            let width = s.len().max(a.len());
            format!("{s:>width$}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_in_out(input: &[String], output: &[String], comp: usize) {
    let chans = if comp == 4 { "R G B A" } else { "R G B" };
    println!("Input  [{chans}]: [{}]", aligned_vec(input, output));
    println!("Output [{chans}]: [{}]", aligned_vec(output, input));
}

fn apply(cpu: &CpuProcessor, pixel: &mut [f32; 4]) {
    cpu.apply_rgba(pixel);
}

const INPUT_4_TEST: [f32; 27] = [
    0.0, 0.0, 0.0, //
    0.18, 0.18, 0.18, //
    0.5, 0.5, 0.5, //
    1.0, 1.0, 1.0, //
    2.0, 2.0, 2.0, //
    100.0, 100.0, 100.0, //
    1.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, //
    0.0, 0.0, 1.0,
];

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ociochecklut -- check any LUT file and optionally convert a pixel\n\n\
         usage:  ociochecklut <INPUTFILE> <R G B> or <R G B A>\n",
    )
    .positional("")
    .separator("Options:")
    .flag("-t", "test", "Test a set a predefined RGB values")
    .flag("-v", "verbose", "Verbose")
    .flag(
        "-s",
        "step",
        "Print the output after each step in a multi - transform LUT",
    )
    .flag("--help", "help", "Print help message")
    .flag("--inv", "inv", "Apply LUT in inverse direction")
    .flag("--gpu", "gpu", "Use GPU instead of CPU")
    .flag(
        "--gpulegacy",
        "gpulegacy",
        "Use the legacy (i.e. baked) GPU color processing instead of the CPU one (--gpu is ignored)",
    )
    .flag("--gpuinfo", "gpuinfo", "Output the OCIO shader program");

    let parsed = ap.parse(&argv).is_ok();
    let help = ap.get_bool("help");

    let mut inputfile = String::new();
    let mut input: Vec<f32> = Vec::new();
    for (i, a) in ap.args().iter().enumerate() {
        if i == 0 {
            inputfile = a.clone();
        } else {
            input.push(strtof(a));
        }
    }

    if !parsed || help || inputfile.is_empty() {
        println!("{}", ap.geterror());
        ap.print_usage();
        println!("{DESC_STRING}");
        if help {
            // What are the allowed formats?
            println!("Formats supported:");
            for i in 0..FileTransform::num_formats() {
                println!(
                    "{} (.{})",
                    FileTransform::format_name_by_index(i),
                    FileTransform::format_extension_by_index(i)
                );
            }
            return ExitCode::SUCCESS;
        }
        return ExitCode::from(1);
    }

    let verbose = ap.get_bool("verbose");
    let mut test = ap.get_bool("test");
    let invlut = ap.get_bool("inv");
    let step_info = ap.get_bool("step");

    if verbose {
        println!();
        println!("OCIO Version: {}", ocio_tools::version());
    }

    if ap.get_bool("gpu") || ap.get_bool("gpuinfo") || ap.get_bool("gpulegacy") {
        eprintln!("Compiled without OpenGL support, GPU options are not available.");
        return ExitCode::from(1);
    }

    set_logging_level(LoggingLevel::Warning);
    // The OCIO log goes to stdout, so print any log messages associated with
    // reading the transform.
    let guard = LogGuard::with_level(logging_level());

    let printops = input.is_empty() && !test;

    let config = Config::create();

    // Create the OCIO processor for the specified transform.
    let mut ft = FileTransform::new(&inputfile);
    ft.interpolation = Interpolation::Best;
    ft.direction = if invlut {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    };
    let t = Transform::File(ft);

    let processor = match config.get_processor_for_transform(&t, TransformDirection::Forward) {
        Ok(p) => p,
        Err(e) => {
            flush_log(&guard);
            eprintln!("ERROR: {e}");
            return ExitCode::from(1);
        }
    };
    flush_log(&guard);

    if printops {
        let group = processor.create_group_transform();
        println!("Transform operators: ");
        for tr in &group.transforms {
            println!("\t{}", format_transform(tr));
        }
        if group.transforms.is_empty() {
            println!("No transform.");
        }
    }
    let mut cpu = processor.default_cpu_processor();

    if printops {
        // Only displays the LUT file content.
        return ExitCode::SUCCESS;
    }

    // Validate the input values.
    let mut num_input = input.len();

    if test && num_input > 0 {
        eprintln!(
            "ERROR: Expecting either RGB (or RGBA) pixel or predefined RGB values (i.e. -t)."
        );
        return ExitCode::from(1);
    }

    let mut comp = 3;
    if num_input == 4 {
        comp = 4;
    } else if num_input != 3 && !test {
        eprintln!("ERROR: Expecting either RGB or RGBA pixel.");
        return ExitCode::from(1);
    }

    // Process the input values.
    let mut cur_pix = 0;

    if verbose || step_info {
        println!();
    }

    loop {
        if cur_pix + comp <= num_input {
            let mut pixel = [
                input[cur_pix],
                input[cur_pix + 1],
                input[cur_pix + 2],
                if comp == 3 { 0.0 } else { input[cur_pix + 3] },
            ];

            if step_info {
                // Process each step in a multi-transform LUT: create the
                // group transform so that each can be processed one at a time.
                let group = processor.create_group_transform();
                let mut input_pixel = pixel;
                let mut output_pixel = pixel;

                println!();

                for tr in &group.transforms {
                    let step =
                        match config.get_processor_for_transform(tr, TransformDirection::Forward) {
                            Ok(p) => p,
                            Err(e) => {
                                flush_log(&guard);
                                eprintln!("ERROR: {e}");
                                return ExitCode::from(1);
                            }
                        };
                    flush_log(&guard);
                    cpu = step.default_cpu_processor();

                    // Process the pixel.
                    apply(&cpu, &mut output_pixel);

                    // Print the input/output pixel.
                    let ins = to_strings(&input_pixel, 0, comp);
                    let outs = to_strings(&output_pixel, 0, comp);

                    println!("\n{}", format_transform(tr));
                    print_in_out(&ins, &outs, comp);

                    input_pixel = output_pixel;
                }
                cur_pix += comp;
            } else {
                // Process in a single step.
                apply(&cpu, &mut pixel);

                // Print to string so that in & out values can be aligned if needed.
                let outs = to_strings(&pixel, 0, comp);

                println!();

                if verbose {
                    let ins = to_strings(&input, cur_pix, comp);
                    print_in_out(&ins, &outs, comp);
                } else {
                    println!("{}", outs.join(" "));
                }
                cur_pix += comp;
            }
        } else if test {
            if verbose {
                println!("Testing with predefined set of RGB pixels.");
            }
            input = INPUT_4_TEST.to_vec();
            comp = 3;
            num_input = input.len();
            cur_pix = 0;
            test = false;
        } else {
            break;
        }
    }

    ExitCode::SUCCESS
}
