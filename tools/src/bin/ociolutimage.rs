//! ociolutimage -- convert a 3D LUT to or from an image (port of the OCIO
//! `ociolutimage` app).

use ocio::{BitDepth, ChannelOrdering, Config, Error, Result};
use ocio_tools::argparse::ArgParse;
use ocio_tools::fmt_default;
use ocio_tools::imageio::ImageIO;
use std::io::Write;
use std::process::ExitCode;

/// Order of the 3D LUT entries in the lattice image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lut3DOrder {
    FastRed,
    #[allow(dead_code)]
    FastBlue,
}

fn get_lut_image_size(cubesize: i32, maxwidth: i32) -> (i32, i32) {
    // Compute the image width / height.
    let mut width = cubesize * cubesize;
    if maxwidth > 0 && width >= maxwidth {
        // TODO: Do something smarter here to find a better multiple, to
        // create a more pleasing gradient rendition. Use prime divisors /
        // lowest common denominator, if possible?
        width = width.min(maxwidth);
    }
    let numpixels = cubesize * cubesize * cubesize;
    let height = (numpixels as f32 / width as f32).ceil() as i32;
    (width, height)
}

fn get_lut3d_index_red_fast(r: usize, g: usize, b: usize, size_r: usize, size_g: usize) -> usize {
    3 * (r + size_r * (g + size_g * b))
}

fn generate_identity_lut3d(
    img: &mut [f32],
    edge_len: usize,
    num_channels: usize,
    order: Lut3DOrder,
) -> Result<()> {
    if num_channels < 3 {
        return Err(Error::msg(
            "Cannot generate identity 3D LUT with less than 3 channels.",
        ));
    }
    let c = 1.0f32 / (edge_len as f32 - 1.0);
    for i in 0..edge_len * edge_len * edge_len {
        let (r, g, b) = (
            (i % edge_len) as f32 * c,
            ((i / edge_len) % edge_len) as f32 * c,
            ((i / edge_len / edge_len) % edge_len) as f32 * c,
        );
        let px = &mut img[num_channels * i..num_channels * i + 3];
        match order {
            Lut3DOrder::FastRed => px.copy_from_slice(&[r, g, b]),
            Lut3DOrder::FastBlue => px.copy_from_slice(&[b, g, r]),
        }
    }
    Ok(())
}

fn write_lut3d(filename: &str, lutdata: &[f32], edge_len: usize) -> Result<()> {
    if !filename.ends_with(".spi3d") {
        return Err(Error::msg(
            "Only .spi3d writing is currently supported. As a work around, please write a \
             .spi3d file, and then use ociobakelut for transcoding.",
        ));
    }

    let mut output = String::new();
    output.push_str("SPILUT 1.0\n");
    output.push_str("3 3\n");
    output.push_str(&format!("{edge_len} {edge_len} {edge_len}\n"));

    for r in 0..edge_len {
        for g in 0..edge_len {
            for b in 0..edge_len {
                let index = get_lut3d_index_red_fast(r, g, b, edge_len, edge_len);
                output.push_str(&format!(
                    "{} {} {} {} {} {}\n",
                    r,
                    g,
                    b,
                    fmt_default(f64::from(lutdata[index])),
                    fmt_default(f64::from(lutdata[index + 1])),
                    fmt_default(f64::from(lutdata[index + 2]))
                ));
            }
        }
    }

    let mut file = std::fs::File::create(filename)
        .map_err(|_| Error::msg(format!("Error opening {filename} for writing.")))?;
    file.write_all(output.as_bytes())?;
    Ok(())
}

fn generate(
    cubesize: i32,
    maxwidth: i32,
    outputfile: &str,
    configfile: &str,
    incolorspace: &str,
    outcolorspace: &str,
) -> Result<()> {
    if cubesize < 2 {
        return Err(Error::msg(format!("Invalid cube size: {cubesize}.")));
    }
    let (width, height) = get_lut_image_size(cubesize, maxwidth);

    let mut img = ImageIO::new(
        width.max(0) as usize,
        height.max(0) as usize,
        ChannelOrdering::Rgb,
        BitDepth::F32,
    )?;

    if let Some(pixels) = img.data_f32_mut() {
        generate_identity_lut3d(pixels, cubesize as usize, 3, Lut3DOrder::FastRed)?;
    }

    if !incolorspace.is_empty() || !outcolorspace.is_empty() {
        let config = if !configfile.is_empty() {
            Config::create_from_file(configfile)?
        } else if ocio_tools::env_variable("OCIO").is_some() {
            Config::create_from_env()?
        } else {
            return Err(Error::msg(
                "You must specify an OCIO configuration (either with --config or $OCIO).",
            ));
        };

        let processor = config
            .get_processor(incolorspace, outcolorspace)?
            .default_cpu_processor();
        let mut desc = img.image_desc()?;
        processor.apply(&mut desc);
    }

    // TODO: If DPX, force 16-bit output?
    img.write(outputfile, BitDepth::Unknown)
}

fn extract(cubesize: i32, maxwidth: i32, inputfile: &str, outputfile: &str) -> Result<()> {
    // The lattice is read as float values whatever the file bit depth.
    let mut img = ImageIO::default();
    img.read(inputfile, BitDepth::F32)?;

    let (width, height) = get_lut_image_size(cubesize, maxwidth);

    if img.width() as i64 != i64::from(width) || img.height() as i64 != i64::from(height) {
        return Err(Error::msg(format!(
            "Image does not have expected dimensions. Expected {}x{}, Found {}x{}",
            width,
            height,
            img.width(),
            img.height()
        )));
    }

    if img.num_channels() != 3 {
        return Err(Error::msg("Image must have 3 channels."));
    }

    let lut3d_num_pixels = (cubesize * cubesize * cubesize).max(0) as usize;
    if img.width() * img.height() < lut3d_num_pixels || cubesize < 2 {
        return Err(Error::msg(
            "Image is not large enough to contain expected 3D LUT.",
        ));
    }

    let data = img.data_f32().unwrap_or(&[]);
    write_lut3d(outputfile, data, cubesize as usize)
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();

    // TODO: Add optional allocation transform instead of colorconvert.
    let mut ap = ArgParse::new(
        "ociolutimage -- Convert a 3D LUT to or from an image\n\n\
         usage:  ociolutimage [options] <OUTPUTFILE.LUT>\n\n\
         example:  ociolutimage --generate --output lut.exr\n\
         example:  ociolutimage --extract --input lut.exr --output output.spi3d\n",
    )
    .separator("")
    .flag("--generate", "generate", "Generate a lattice image")
    .flag(
        "--extract",
        "extract",
        "Extract a 3D LUT from an input image",
    )
    .separator("")
    .option(
        "--cubesize %d",
        &["cubesize"],
        "Size of the cube (default: 32)",
    )
    .option(
        "--maxwidth %d",
        &["maxwidth"],
        "Specify maximum width of the image (default: 2048)",
    )
    .option("--input %s", &["input"], "Specify the input filename")
    .option("--output %s", &["output"], "Specify the output filename")
    .separator("")
    .option(
        "--config %s",
        &["config"],
        ".ocio configuration file (default: $OCIO)",
    )
    .option(
        "--colorconvert %s %s",
        &["incs", "outcs"],
        "Apply a color space conversion to the image.",
    );

    if ap.parse(&argv).is_err() {
        println!("{}", ap.geterror());
        ap.print_usage();
        println!();
        return ExitCode::from(1);
    }

    if argv.len() == 1 {
        ap.print_usage();
        println!();
        return ExitCode::from(1);
    }

    let cubesize = ap.get_int("cubesize", 32);
    let maxwidth = ap.get_int("maxwidth", 2048);
    let inputfile = ap.get_string("input", "");
    let outputfile = ap.get_string("output", "");

    if ap.get_bool("generate") {
        if let Err(e) = generate(
            cubesize,
            maxwidth,
            &outputfile,
            &ap.get_string("config", ""),
            &ap.get_string("incs", ""),
            &ap.get_string("outcs", ""),
        ) {
            eprintln!("Error generating image: {e}");
            return ExitCode::from(1);
        }
    } else if ap.get_bool("extract") {
        if let Err(e) = extract(cubesize, maxwidth, &inputfile, &outputfile) {
            eprintln!("Error extracting LUT: {e}");
            return ExitCode::from(1);
        }
    } else {
        eprintln!("Must specify either --generate or --extract.");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
