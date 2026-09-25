//! ocioperf -- apply and measure a color transformation processing (port of
//! the OCIO `ocioperf` app).
//!
//! This port has no GPU support, so the GPU processor and shader creation
//! measures of the C++ app are not performed.

use ocio::apphelpers::display_view_helpers::get_processor as get_display_view_processor;
use ocio::{
    BitDepth, Config, Error, FileTransform, ImageData, OptimizationFlags, PackedImageDesc,
    Processor, ProcessorCacheFlags, Result, Transform, TransformDirection,
};
use ocio_tools::argparse::ArgParse;
use ocio_tools::CustomMeasure;
use std::process::ExitCode;

const WIDTH: usize = 3840;
const HEIGHT: usize = 2160;
const NUM_CHANNELS: usize = 4;
const MAX_ELTS: usize = WIDTH * HEIGHT;

/// An image buffer of one of the supported bit depths.
#[derive(Clone)]
enum Buffer {
    F32(Vec<f32>),
    U16(Vec<u16>),
}

impl Buffer {
    fn desc(&mut self, width: usize, height: usize) -> Result<PackedImageDesc<'_>> {
        let data = match self {
            Buffer::F32(v) => ImageData::F32(v),
            Buffer::U16(v) => ImageData::U16(v),
        };
        PackedImageDesc::new(data, width, height, NUM_CHANNELS)
    }

    fn line(&mut self, y: usize) -> Result<PackedImageDesc<'_>> {
        let range = y * WIDTH * NUM_CHANNELS..(y + 1) * WIDTH * NUM_CHANNELS;
        let data = match self {
            Buffer::F32(v) => ImageData::F32(&mut v[range]),
            Buffer::U16(v) => ImageData::U16(&mut v[range]),
        };
        PackedImageDesc::new(data, WIDTH, 1, NUM_CHANNELS)
    }

    fn with_depth(bit_depth: BitDepth) -> Buffer {
        if bit_depth == BitDepth::F32 {
            Buffer::F32(vec![0.0; MAX_ELTS * NUM_CHANNELS])
        } else {
            Buffer::U16(vec![0; MAX_ELTS * NUM_CHANNELS])
        }
    }
}

/// Generate a synthetic image by emulating a LUT3D identity algorithm that
/// steps through many different colors. Need to avoid a constant image,
/// simple gradients, or anything that would result in more cache hits than
/// a typical image. Also, want to step through a wide range of colors,
/// including outside [0,1], in case some algorithms are faster or slower for
/// certain colors.
fn reference_image(bit_depth: BitDepth) -> Buffer {
    const LENGTH: usize = 201;
    const STEP: f32 = 1.0 / (LENGTH as f32 - 1.0);

    let comps = |idx: usize| -> [f32; 4] {
        [
            ((idx / LENGTH / LENGTH) % LENGTH) as f32 * STEP,
            ((idx / LENGTH) % LENGTH) as f32 * STEP,
            (idx % LENGTH) as f32 * STEP,
            idx as f32 / MAX_ELTS as f32,
        ]
    };

    if bit_depth == BitDepth::F32 {
        const MIN: f32 = -1.0;
        const MAX: f32 = 2.0;
        const RANGE: f32 = MAX - MIN;
        let mut img = vec![0.0f32; MAX_ELTS * NUM_CHANNELS];
        for idx in 0..MAX_ELTS {
            let c = comps(idx);
            for (k, v) in c.iter().enumerate() {
                img[NUM_CHANNELS * idx + k] = v * RANGE + MIN;
            }
        }
        Buffer::F32(img)
    } else {
        let mut img = vec![0u16; MAX_ELTS * NUM_CHANNELS];
        for idx in 0..MAX_ELTS {
            let c = comps(idx);
            for (k, v) in c.iter().enumerate() {
                img[NUM_CHANNELS * idx + k] = (v * 65535.0) as u16;
            }
        }
        Buffer::U16(img)
    }
}

fn bit_depth_from_string(s: &str) -> Result<BitDepth> {
    match s {
        "f32" => Ok(BitDepth::F32),
        "ui16" => Ok(BitDepth::UInt16),
        _ => Err(Error::msg(format!("Unsupported bit-depth: {s}"))),
    }
}

fn run(ap: &ArgParse) -> Result<()> {
    let verbose = ap.get_bool("verbose");
    let test_type = ap.get_int("test", -1);
    let transform_file = ap.get_string("transform", "");
    let in_cs = ap.get_string("incs", "");
    let out_cs = ap.get_string("outcs", "");
    let display = ap.get_string("display", "");
    let view = ap.get_string("view", "");
    let inputconfig = ap.get_string("iconfig", "");
    let iterations = ap.get_int("iter", 50).max(0) as u32;
    let in_bd_str = ap.get_string("inbd", "f32");
    let out_bd_str = ap.get_string("outbd", "f32");
    let nocache = ap.get_bool("nocache");
    let nooptim = ap.get_bool("nooptim");

    let cache_flags = if nocache {
        ProcessorCacheFlags::OFF
    } else {
        ProcessorCacheFlags::DEFAULT
    };

    let mut processor: Option<Processor> = None;

    if !transform_file.is_empty() {
        let config = Config::create_raw().create_editable_copy();
        config.set_processor_cache_flags(cache_flags);

        // Get the transform.
        let transform = Transform::File(FileTransform::new(&transform_file));

        let mut m = CustomMeasure::new("Create the processor:\t\t\t", iterations);
        for _ in 0..iterations {
            if nocache {
                ocio_tools::clear_all_caches();
            }
            m.resume()?;
            let p = config.get_processor_for_transform(&transform, TransformDirection::Forward);
            m.pause()?;
            processor = Some(p?);
        }
    } else if !in_cs.is_empty() || (!display.is_empty() && !view.is_empty()) {
        // Checking for an input colorspace or input (display, view) pair.
        let src_config = if !inputconfig.is_empty() {
            println!();
            println!("Loading {inputconfig}");
            Config::create_from_file(&inputconfig)?
        } else if let Some(env) = ocio_tools::env_variable("OCIO") {
            println!();
            println!("Loading $OCIO {env}");
            Config::create_from_env()?
        } else {
            return Err(Error::msg(
                "You must specify an input OCIO configuration (either with --iconfig or $OCIO).\n",
            ));
        };

        if verbose {
            println!();
            println!(
                "OCIO Config. version: {}.{}",
                src_config.major_version(),
                src_config.minor_version()
            );
            println!("OCIO search_path:     {}", src_config.search_path());
            println!();
            let pair = format!("({display}, {view})");
            let input_str = if in_cs.is_empty() { &pair } else { &in_cs };
            let output_str = if out_cs.is_empty() { &pair } else { &out_cs };
            println!("Processing from '{input_str}' to '{output_str}'");
        }

        let config = src_config.create_editable_copy();
        config.set_processor_cache_flags(cache_flags);

        {
            let mut m = CustomMeasure::new("Create the config identifier:\t\t", iterations);
            for _ in 0..iterations {
                m.resume()?;
                let _ = config.cache_id();
                m.pause()?;
            }
        }

        {
            let mut m = CustomMeasure::new("Create the context identifier:\t\t", iterations);
            for _ in 0..iterations {
                m.resume()?;
                let _ = config.current_context().cache_id();
                m.pause()?;
            }
        }

        // --colorspaces
        let use_colorspaces = !in_cs.is_empty() && !out_cs.is_empty();
        // --view
        let use_displayview = !in_cs.is_empty() && !display.is_empty() && !view.is_empty();
        // --invertview
        let use_invertview = !display.is_empty() && !view.is_empty() && !out_cs.is_empty();

        // Exactly one of the three options must be used.
        let num_used = [use_colorspaces, use_displayview, use_invertview]
            .iter()
            .filter(|u| **u)
            .count();
        let msg = if num_used == 1 {
            if use_colorspaces || use_invertview {
                "Create the colorspaces processor:\t"
            } else {
                "Create the (display, view) processor:\t"
            }
        } else {
            return Err(Error::msg(
                "Any combinations of --colorspaces, --view or --invertview is invalid.",
            ));
        };

        let mut m = CustomMeasure::new(msg, iterations);
        for _ in 0..iterations {
            if nocache {
                // Flush all the global internal caches.
                ocio_tools::clear_all_caches();
            }

            m.resume()?;
            let p = if use_colorspaces {
                // Processing colorspaces option.
                config.get_processor(&in_cs, &out_cs)
            } else if use_displayview {
                // Processing view option.
                get_display_view_processor(
                    &config,
                    &in_cs,
                    &display,
                    &view,
                    None,
                    TransformDirection::Forward,
                )
            } else {
                // Processing invertview option.
                get_display_view_processor(
                    &config,
                    &out_cs,
                    &display,
                    &view,
                    None,
                    TransformDirection::Inverse,
                )
            };
            m.pause()?;
            processor = Some(p?);
        }
    } else {
        return Err(Error::msg("Missing color transformation description."));
    }

    let processor = match processor {
        Some(p) => p,
        // No iteration: build the processor once to go on.
        None => {
            return Err(Error::msg(
                "No processor created (the number of iterations is 0).",
            ))
        }
    };

    let optim_flags = if nooptim {
        OptimizationFlags::NONE
    } else {
        OptimizationFlags::DEFAULT
    };

    let in_bd = bit_depth_from_string(&in_bd_str)?;
    let out_bd = bit_depth_from_string(&out_bd_str)?;

    // Get the optimized processor.
    let mut opt_processor = processor.clone();
    {
        let mut m = CustomMeasure::new("Create the optimized processor:\t\t", iterations);
        for _ in 0..iterations {
            m.resume()?;
            opt_processor = processor.optimized(optim_flags);
            m.pause()?;
        }
    }

    // Get the CPU processor.
    let mut cpu = opt_processor.optimized_cpu_processor_with_bit_depths(in_bd, out_bd, optim_flags);
    {
        let mut m = CustomMeasure::new("Create the CPU processor:\t\t", iterations);
        for _ in 0..iterations {
            m.resume()?;
            cpu = opt_processor.optimized_cpu_processor_with_bit_depths(in_bd, out_bd, optim_flags);
            m.pause()?;
        }
    }

    println!();
    println!();
    println!("Image processing statistics:");
    println!();

    // Create an arbitrary 4K RGBA image.
    let img_ref = reference_image(in_bd);

    if test_type == 0 || test_type == -1 {
        // Process the complete image (in place).
        if in_bd == out_bd {
            let mut m =
                CustomMeasure::new("Process the complete image (in place):\t\t\t\t", iterations);
            for _ in 0..iterations {
                let mut img = img_ref.clone();
                let mut desc = img.desc(WIDTH, HEIGHT)?;
                // Apply the color transformation.
                m.resume()?;
                cpu.apply(&mut desc);
                m.pause()?;
            }
        }

        // Process the complete image with input and output buffers.
        {
            let mut in_img = img_ref.clone();
            let in_desc = in_img.desc(WIDTH, HEIGHT)?;
            let mut out_img = Buffer::with_depth(out_bd);
            let mut out_desc = out_img.desc(WIDTH, HEIGHT)?;

            // Use a custom cpu processor as input and output bit depths
            // could be different.
            let cpu2 =
                opt_processor.optimized_cpu_processor_with_bit_depths(in_bd, out_bd, optim_flags);

            let mut m = CustomMeasure::new(
                "Process the complete image (two buffers):\t\t\t",
                iterations,
            );
            for _ in 0..iterations {
                // Apply the color transformation.
                m.resume()?;
                let res = cpu2.apply_src_dst(&in_desc, &mut out_desc);
                m.pause()?;
                res?;
            }
        }
    }

    if (test_type == 1 || test_type == -1) && in_bd == out_bd {
        // Process line by line.
        let mut img = img_ref.clone();
        let mut m = CustomMeasure::new(
            "Process the complete image (in place) but line by line:\t\t",
            iterations,
        );
        for _ in 0..iterations {
            // Always process the same complete image.
            m.resume()?;
            for y in 0..HEIGHT {
                let mut line = img.line(y)?;
                // Apply the color transformation (in place).
                cpu.apply(&mut line);
            }
            m.pause()?;
        }
    }

    if (test_type == 2 || test_type == -1) && in_bd == out_bd && in_bd == BitDepth::F32 {
        // Process pixel per pixel.
        if let Buffer::F32(mut img) = img_ref.clone() {
            let mut m = CustomMeasure::new(
                "Process the complete image (in place) but pixel per pixel:\t",
                iterations,
            );
            for _ in 0..iterations {
                // Always process the same complete image.
                m.resume()?;
                for px in img.chunks_exact_mut(NUM_CHANNELS) {
                    let mut rgba = [px[0], px[1], px[2], px[3]];
                    cpu.apply_rgba(&mut rgba);
                    px.copy_from_slice(&rgba);
                }
                m.pause()?;
            }
        }
    }

    println!();
    println!();

    Ok(())
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ocioperf -- apply and measure a color transformation processing\n\n\
         usage: ocioperf [options] --transform /path/to/file.clf\n\n",
    )
    .flag("--h", "help", "Display the help and exit")
    .flag("--help", "help", "Display the help and exit")
    .flag("--verbose", "verbose", "Display some general information")
    .option(
        "--test %d",
        &["test"],
        "Define the type of processing to measure: 0 means on the complete image (the default), \
         1 is line-by-line, 2 is pixel-per-pixel and -1 performs all the test types",
    )
    .option(
        "--transform %s",
        &["transform"],
        "Provide the transform file to apply on the image",
    )
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
        "(Deprecated) Provide the input and (display, view) pair to apply on the image",
    )
    .option(
        "--invertview %s %s %s",
        &["display", "view", "outcs"],
        "Provide the (display, view) pair and output color space to apply on the image",
    )
    .option(
        "--iconfig %s",
        &["iconfig"],
        "Input .ocio configuration file (default: $OCIO)",
    )
    .option(
        "--iter %d",
        &["iter"],
        "Provide the number of iterations on the processing. Default is 50",
    )
    .option(
        "--bitdepths %s %s",
        &["inbd", "outbd"],
        "Provide input and output bit-depths (i.e. ui16, f32). Default is f32",
    )
    .flag(
        "--nocache",
        "nocache",
        "Bypass all caches. Default is false",
    )
    .flag(
        "--nooptim",
        "nooptim",
        "Disable the processor optimizations. Default is false",
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

    if ap.get_bool("verbose") {
        println!();
        println!("OCIO Version: {}", ocio_tools::version());
    }

    let transform_file = ap.get_string("transform", "");
    if !transform_file.is_empty() {
        println!();
        println!("Processing using '{transform_file}'");
        println!();
    }

    println!();
    println!();
    println!("Processing statistics:");
    println!();

    match run(&ap) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("OCIO ERROR: {e}");
            ExitCode::from(1)
        }
    }
}
