// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! `CpuProcessor::apply` / `apply_src_dst` / `apply_rgba_slice` give the
//! same results for every image layout (the packed RGBA `f32` images
//! processed in place and the layouts converted row by row) as
//! `apply_pixels`. Images one pixel wide give the results of `apply_rgba`:
//! as in OCIO, some renderers (the 1D LUT) use another code path when a
//! single pixel is processed.

use ocio::{
    BitDepth, ChannelOrdering, Config, CpuProcessor, FileTransform, ImageData, OptimizationFlags,
    PackedImageDesc, PlanarImageDesc, Transform, TransformDirection,
};

const WIDTH: usize = 37;
const HEIGHT: usize = 5;

fn file_processor(name: &str) -> CpuProcessor {
    let path = format!("{}/tests/data/files/{name}", env!("CARGO_MANIFEST_DIR"));
    Config::create_raw()
        .get_processor_for_transform(
            &Transform::File(FileTransform::new(&path)),
            TransformDirection::Forward,
        )
        .unwrap()
        .optimized_cpu_processor(OptimizationFlags::DEFAULT)
}

/// Processors covering the matrix, gamma, log, 1D LUT and 3D LUT renderers.
fn processors() -> Vec<(&'static str, CpuProcessor)> {
    let config =
        Config::create_from_file("ocio://studio-config-v2.2.0_aces-v1.3_ocio-v2.4").unwrap();
    let cs = |src: &str, dst: &str| {
        config
            .get_processor(src, dst)
            .unwrap()
            .optimized_cpu_processor(OptimizationFlags::DEFAULT)
    };
    vec![
        (
            "ACEScg to sRGB",
            cs("ACEScg", "sRGB Encoded Rec.709 (sRGB)"),
        ),
        ("ACEScg to ACEScct", cs("ACEScg", "ACEScct")),
        ("lut1d", file_processor("clf/lut1d_long.clf")),
        ("lut3d", file_processor("clf/lut3d_17x17x17_10i_12i.clf")),
        ("multiple ops", file_processor("clf/multiple_ops.clf")),
    ]
}

/// RGBA pixels with in-range, out-of-range and special values.
fn pixels() -> Vec<[f32; 4]> {
    let specials = [
        0.0,
        -0.0,
        1.0,
        -0.25,
        1.5,
        1e-30,
        65504.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    (0..WIDTH * HEIGHT)
        .map(|i| {
            let v = |k: usize| {
                let j = i * 4 + k;
                if j.is_multiple_of(13) {
                    specials[(j / 13) % specials.len()]
                } else {
                    ((j * 7919) % 1201) as f32 / 1000.0 - 0.1
                }
            };
            [v(0), v(1), v(2), v(3)]
        })
        .collect()
}

/// The results of `apply_pixels` on all the pixels.
fn reference(cpu: &CpuProcessor, pixels: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let mut out = pixels.to_vec();
    cpu.apply_pixels(&mut out);
    out
}

/// The results of `apply_rgba`, one pixel at a time.
fn single_pixel_reference(cpu: &CpuProcessor, pixels: &[[f32; 4]]) -> Vec<[f32; 4]> {
    pixels
        .iter()
        .map(|p| {
            let mut p = *p;
            cpu.apply_rgba(&mut p);
            p
        })
        .collect()
}

fn assert_same(name: &str, layout: &str, actual: &[[f32; 4]], expected: &[[f32; 4]]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        for c in 0..4 {
            let same = a[c].to_bits() == e[c].to_bits() || (a[c].is_nan() && e[c].is_nan());
            assert!(
                same,
                "{name}, {layout}: pixel {i} channel {c}: {} instead of {}",
                a[c], e[c]
            );
        }
    }
}

fn flatten(pixels: &[[f32; 4]]) -> Vec<f32> {
    pixels.iter().flatten().copied().collect()
}

/// Pixels of a packed buffer with `x_stride` elements per pixel and the
/// channel offsets of `ordering`.
fn unpack(data: &[f32], x_stride: usize, ordering: ChannelOrdering) -> Vec<[f32; 4]> {
    let (rgb, alpha) = match ordering {
        ChannelOrdering::Rgba => ([0, 1, 2], Some(3)),
        ChannelOrdering::Bgra => ([2, 1, 0], Some(3)),
        ChannelOrdering::Abgr => ([3, 2, 1], Some(0)),
        ChannelOrdering::Rgb => ([0, 1, 2], None),
        ChannelOrdering::Bgr => ([2, 1, 0], None),
    };
    data.chunks(x_stride)
        .map(|p| [p[rgb[0]], p[rgb[1]], p[rgb[2]], alpha.map_or(1.0, |a| p[a])])
        .collect()
}

fn pack(pixels: &[[f32; 4]], x_stride: usize, ordering: ChannelOrdering) -> Vec<f32> {
    let mut data = vec![0.0f32; pixels.len() * x_stride];
    let (rgb, alpha) = match ordering {
        ChannelOrdering::Rgba => ([0, 1, 2], Some(3)),
        ChannelOrdering::Bgra => ([2, 1, 0], Some(3)),
        ChannelOrdering::Abgr => ([3, 2, 1], Some(0)),
        ChannelOrdering::Rgb => ([0, 1, 2], None),
        ChannelOrdering::Bgr => ([2, 1, 0], None),
    };
    for (p, out) in pixels.iter().zip(data.chunks_mut(x_stride)) {
        for c in 0..3 {
            out[rgb[c]] = p[c];
        }
        if let Some(a) = alpha {
            out[a] = p[3];
        }
    }
    data
}

#[test]
fn packed_f32_layouts_match_apply_pixels() {
    let input = pixels();
    for (name, cpu) in processors() {
        let expected = reference(&cpu, &input);

        // Tightly packed RGBA: processed in place.
        let mut data = flatten(&input);
        let mut img = PackedImageDesc::new(ImageData::F32(&mut data), WIDTH, HEIGHT, 4).unwrap();
        cpu.apply(&mut img);
        assert_same(
            name,
            "RGBA",
            &unpack(&data, 4, ChannelOrdering::Rgba),
            &expected,
        );

        // A row stride with padding is processed in place too.
        let y_stride = WIDTH * 4 + 3;
        let mut data = vec![-7.0f32; y_stride * HEIGHT];
        for (y, row) in input.chunks(WIDTH).enumerate() {
            data[y * y_stride..y * y_stride + WIDTH * 4].copy_from_slice(&flatten(row));
        }
        let mut img = PackedImageDesc::with_strides(
            ImageData::F32(&mut data),
            WIDTH,
            HEIGHT,
            ChannelOrdering::Rgba,
            BitDepth::F32,
            4,
            y_stride,
        )
        .unwrap();
        cpu.apply(&mut img);
        for y in 0..HEIGHT {
            let row = &data[y * y_stride..(y + 1) * y_stride];
            let pixels = unpack(&row[..WIDTH * 4], 4, ChannelOrdering::Rgba);
            let expected_row = &expected[y * WIDTH..(y + 1) * WIDTH];
            assert_same(name, "RGBA with row padding", &pixels, expected_row);
            assert!(
                row[WIDTH * 4..].iter().all(|v| *v == -7.0),
                "padding changed"
            );
        }

        // Converted layouts: other channel orders and a pixel stride.
        for (ordering, x_stride) in [
            (ChannelOrdering::Rgba, 5),
            (ChannelOrdering::Bgra, 4),
            (ChannelOrdering::Abgr, 4),
        ] {
            let mut data = pack(&input, x_stride, ordering);
            let mut img = PackedImageDesc::with_strides(
                ImageData::F32(&mut data),
                WIDTH,
                HEIGHT,
                ordering,
                BitDepth::F32,
                x_stride,
                x_stride * WIDTH,
            )
            .unwrap();
            cpu.apply(&mut img);
            let layout = format!("{ordering:?} x stride {x_stride}");
            assert_same(name, &layout, &unpack(&data, x_stride, ordering), &expected);
        }

        // RGB: alpha is 1.
        let rgb_input: Vec<[f32; 4]> = input.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
        let rgb_expected = reference(&cpu, &rgb_input);
        let mut data = pack(&rgb_input, 3, ChannelOrdering::Rgb);
        let mut img = PackedImageDesc::new(ImageData::F32(&mut data), WIDTH, HEIGHT, 3).unwrap();
        cpu.apply(&mut img);
        let actual: Vec<[f32; 4]> = unpack(&data, 3, ChannelOrdering::Rgb);
        let expected_rgb: Vec<[f32; 4]> = rgb_expected
            .iter()
            .map(|p| [p[0], p[1], p[2], 1.0])
            .collect();
        assert_same(name, "RGB", &actual, &expected_rgb);

        // apply_rgba_slice.
        let mut data = flatten(&input);
        cpu.apply_rgba_slice(&mut data);
        assert_same(
            name,
            "apply_rgba_slice",
            &unpack(&data, 4, ChannelOrdering::Rgba),
            &expected,
        );
    }
}

#[test]
fn planar_f32_matches_apply_pixels() {
    let input = pixels();
    for (name, cpu) in processors() {
        let expected = reference(&cpu, &input);
        let mut planes: Vec<Vec<f32>> = (0..4)
            .map(|c| input.iter().map(|p| p[c]).collect())
            .collect();
        let [r, g, b, a] = &mut planes[..] else {
            unreachable!()
        };
        let mut img = PlanarImageDesc::new(
            ImageData::F32(r),
            ImageData::F32(g),
            ImageData::F32(b),
            Some(ImageData::F32(a)),
            WIDTH,
            HEIGHT,
        )
        .unwrap();
        cpu.apply(&mut img);
        let actual: Vec<[f32; 4]> = (0..input.len())
            .map(|i| [planes[0][i], planes[1][i], planes[2][i], planes[3][i]])
            .collect();
        assert_same(name, "planar", &actual, &expected);
    }
}

#[test]
fn apply_src_dst_matches_apply_pixels() {
    let input = pixels();
    for (name, cpu) in processors() {
        let expected = reference(&cpu, &input);

        // RGBA to RGBA: copied into the destination and processed in place.
        let mut src = flatten(&input);
        let mut dst = vec![0.0f32; src.len()];
        let src_img = PackedImageDesc::new(ImageData::F32(&mut src), WIDTH, HEIGHT, 4).unwrap();
        let mut dst_img = PackedImageDesc::new(ImageData::F32(&mut dst), WIDTH, HEIGHT, 4).unwrap();
        cpu.apply_src_dst(&src_img, &mut dst_img).unwrap();
        assert_same(
            name,
            "src/dst RGBA",
            &unpack(&dst, 4, ChannelOrdering::Rgba),
            &expected,
        );
        assert_same(
            name,
            "src unchanged",
            &unpack(&src, 4, ChannelOrdering::Rgba),
            &input,
        );

        // BGRA to RGBA: converted into the destination.
        let mut src = pack(&input, 4, ChannelOrdering::Bgra);
        let mut dst = vec![0.0f32; src.len()];
        let src_img = PackedImageDesc::with_ordering(
            ImageData::F32(&mut src),
            WIDTH,
            HEIGHT,
            ChannelOrdering::Bgra,
        )
        .unwrap();
        let mut dst_img = PackedImageDesc::new(ImageData::F32(&mut dst), WIDTH, HEIGHT, 4).unwrap();
        cpu.apply_src_dst(&src_img, &mut dst_img).unwrap();
        assert_same(
            name,
            "src/dst BGRA to RGBA",
            &unpack(&dst, 4, ChannelOrdering::Rgba),
            &expected,
        );

        // RGBA to BGRA: converted from the result rows.
        let mut src = flatten(&input);
        let mut dst = vec![0.0f32; src.len()];
        let src_img = PackedImageDesc::new(ImageData::F32(&mut src), WIDTH, HEIGHT, 4).unwrap();
        let mut dst_img = PackedImageDesc::with_ordering(
            ImageData::F32(&mut dst),
            WIDTH,
            HEIGHT,
            ChannelOrdering::Bgra,
        )
        .unwrap();
        cpu.apply_src_dst(&src_img, &mut dst_img).unwrap();
        assert_same(
            name,
            "src/dst RGBA to BGRA",
            &unpack(&dst, 4, ChannelOrdering::Bgra),
            &expected,
        );
    }
}

/// `Converter::CastValue`: round half up and clamp to the code values.
fn quantize(v: f32, max: f32) -> f32 {
    let x = v * max + 0.5;
    if x.is_nan() || x < 0.0 {
        0.0
    } else if x > max {
        max
    } else {
        x.floor()
    }
}

#[test]
fn integer_images_match_apply_pixels() {
    let codes: Vec<[u16; 4]> = (0..WIDTH * HEIGHT)
        .map(|i| {
            let v = |k: usize| (((i * 4 + k) * 7919) % 65536) as u16;
            [v(0), v(1), v(2), v(3)]
        })
        .collect();
    for (name, cpu) in processors() {
        // 16-bit in place, through the generic conversions.
        let max = 65535.0f32;
        let normalized: Vec<[f32; 4]> = codes
            .iter()
            .map(|p| p.map(|v| v as f32 * (1.0 / max)))
            .collect();
        let expected: Vec<[f32; 4]> = reference(&cpu, &normalized)
            .iter()
            .map(|p| p.map(|v| quantize(v, max)))
            .collect();
        let mut data: Vec<u16> = codes.iter().flatten().copied().collect();
        let mut img = PackedImageDesc::new(ImageData::U16(&mut data), WIDTH, HEIGHT, 4).unwrap();
        cpu.apply(&mut img);
        let actual: Vec<[f32; 4]> = data
            .chunks(4)
            .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32])
            .collect();
        assert_same(name, "u16 RGBA", &actual, &expected);

        // 8 bit to f32 RGBA: converted into the destination rows.
        let codes8: Vec<u8> = codes.iter().flatten().map(|v| (v >> 8) as u8).collect();
        let normalized8: Vec<[f32; 4]> = codes8
            .chunks(4)
            .map(|p| [0, 1, 2, 3].map(|c| p[c] as f32 * (1.0 / 255.0)))
            .collect();
        let expected8 = reference(&cpu, &normalized8);
        let mut src = codes8.clone();
        let mut dst = vec![0.0f32; src.len()];
        let src_img = PackedImageDesc::new(ImageData::U8(&mut src), WIDTH, HEIGHT, 4).unwrap();
        let mut dst_img = PackedImageDesc::new(ImageData::F32(&mut dst), WIDTH, HEIGHT, 4).unwrap();
        cpu.apply_src_dst(&src_img, &mut dst_img).unwrap();
        assert_same(
            name,
            "u8 to f32",
            &unpack(&dst, 4, ChannelOrdering::Rgba),
            &expected8,
        );
    }
}

#[test]
fn single_pixel_wide_images_match_apply_rgba() {
    let input = pixels();
    for (name, cpu) in processors() {
        let expected = single_pixel_reference(&cpu, &input);
        let n = input.len();

        // In place, one pixel per row.
        let mut data = flatten(&input);
        let mut img = PackedImageDesc::new(ImageData::F32(&mut data), 1, n, 4).unwrap();
        cpu.apply(&mut img);
        assert_same(
            name,
            "1 x n RGBA",
            &unpack(&data, 4, ChannelOrdering::Rgba),
            &expected,
        );

        // Converted rows.
        let mut data = pack(&input, 4, ChannelOrdering::Bgra);
        let mut img =
            PackedImageDesc::with_ordering(ImageData::F32(&mut data), 1, n, ChannelOrdering::Bgra)
                .unwrap();
        cpu.apply(&mut img);
        assert_same(
            name,
            "1 x n BGRA",
            &unpack(&data, 4, ChannelOrdering::Bgra),
            &expected,
        );

        // Source / destination.
        let mut src = flatten(&input);
        let mut dst = vec![0.0f32; src.len()];
        let src_img = PackedImageDesc::new(ImageData::F32(&mut src), 1, n, 4).unwrap();
        let mut dst_img = PackedImageDesc::new(ImageData::F32(&mut dst), 1, n, 4).unwrap();
        cpu.apply_src_dst(&src_img, &mut dst_img).unwrap();
        assert_same(
            name,
            "1 x n src/dst",
            &unpack(&dst, 4, ChannelOrdering::Rgba),
            &expected,
        );
    }
}
