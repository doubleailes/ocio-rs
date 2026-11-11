// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Image descriptions used by `CpuProcessor::apply` (port of `ImageDesc`).
//!
//! Images are described by borrowed, typed buffers. Integer bit depths are
//! normalized by their maximum code value (e.g. 10ui values live in a `u16`
//! buffer and 1023 maps to 1.0).

use crate::error::{Error, Result};
use crate::math_utils::floor_non_negative;
use crate::ops::Pixel;
use crate::types::{BitDepth, ChannelOrdering};
use half::f16;

/// Typed pixel storage.
#[derive(Debug)]
pub enum ImageData<'a> {
    /// 8 bit integers.
    U8(&'a mut [u8]),
    /// 10, 12, 14 or 16 bit integers.
    U16(&'a mut [u16]),
    /// Half floats.
    F16(&'a mut [f16]),
    /// 32 bit floats.
    F32(&'a mut [f32]),
}

impl ImageData<'_> {
    fn len(&self) -> usize {
        match self {
            ImageData::U8(d) => d.len(),
            ImageData::U16(d) => d.len(),
            ImageData::F16(d) => d.len(),
            ImageData::F32(d) => d.len(),
        }
    }

    fn default_bit_depth(&self) -> BitDepth {
        match self {
            ImageData::U8(_) => BitDepth::UInt8,
            ImageData::U16(_) => BitDepth::UInt16,
            ImageData::F16(_) => BitDepth::F16,
            ImageData::F32(_) => BitDepth::F32,
        }
    }
}

#[inline]
fn quantize(v: f32, max: f32) -> f32 {
    let x = v * max + 0.5;
    if x.is_nan() || x < 0.0 {
        0.0
    } else if x > max {
        max
    } else {
        floor_non_negative(x)
    }
}

/// A channel value type of an image buffer, converted to and from the
/// normalized `f32` processing values.
trait Sample: Copy {
    /// The processing value of a stored value (`scale` is `1 / max` for
    /// integer types).
    fn load(self, scale: f32) -> f32;
    /// The stored value of a processing value (`max` is the maximum code
    /// value for integer types).
    fn store(v: f32, max: f32) -> Self;
}

impl Sample for u8 {
    #[inline]
    fn load(self, scale: f32) -> f32 {
        self as f32 * scale
    }
    #[inline]
    fn store(v: f32, max: f32) -> Self {
        quantize(v, max) as u8
    }
}

impl Sample for u16 {
    #[inline]
    fn load(self, scale: f32) -> f32 {
        self as f32 * scale
    }
    #[inline]
    fn store(v: f32, max: f32) -> Self {
        quantize(v, max) as u16
    }
}

impl Sample for f16 {
    #[inline]
    fn load(self, _scale: f32) -> f32 {
        self.to_f32()
    }
    #[inline]
    fn store(v: f32, _max: f32) -> Self {
        f16::from_f32(v)
    }
}

impl Sample for f32 {
    #[inline]
    fn load(self, _scale: f32) -> f32 {
        self
    }
    #[inline]
    fn store(v: f32, _max: f32) -> Self {
        v
    }
}

/// Call `$f` with the typed buffer of an [`ImageData`], so that the
/// element type is resolved once per row rather than once per value.
macro_rules! with_typed {
    ($data:expr, $d:ident => $f:expr) => {
        match $data {
            ImageData::U8($d) => $f,
            ImageData::U16($d) => $f,
            ImageData::F16($d) => $f,
            ImageData::F32($d) => $f,
        }
    };
}

/// Layout of the channels of a packed row.
#[derive(Clone, Copy)]
struct PackedLayout {
    /// Index of the first value of the row.
    base: usize,
    /// Distance between pixels, in elements.
    x_stride: usize,
    /// Offsets of R, G and B in a pixel.
    rgb: [usize; 3],
    /// Offset of A in a pixel, if any.
    alpha: Option<usize>,
}

#[inline]
fn read_packed<T: Sample>(d: &[T], l: PackedLayout, scale: f32, out: &mut [Pixel]) {
    for (x, px) in out.iter_mut().enumerate() {
        let i = l.base + x * l.x_stride;
        px[0] = d[i + l.rgb[0]].load(scale);
        px[1] = d[i + l.rgb[1]].load(scale);
        px[2] = d[i + l.rgb[2]].load(scale);
        px[3] = match l.alpha {
            Some(a) => d[i + a].load(scale),
            None => 1.0,
        };
    }
}

#[inline]
fn write_packed<T: Sample>(d: &mut [T], l: PackedLayout, max: f32, pixels: &[Pixel]) {
    for (x, px) in pixels.iter().enumerate() {
        let i = l.base + x * l.x_stride;
        d[i + l.rgb[0]] = T::store(px[0], max);
        d[i + l.rgb[1]] = T::store(px[1], max);
        d[i + l.rgb[2]] = T::store(px[2], max);
        if let Some(a) = l.alpha {
            d[i + a] = T::store(px[3], max);
        }
    }
}

#[inline]
fn read_plane<T: Sample>(d: &[T], base: usize, scale: f32, c: usize, out: &mut [Pixel]) {
    let n = out.len();
    for (px, v) in out.iter_mut().zip(&d[base..base + n]) {
        px[c] = v.load(scale);
    }
}

#[inline]
fn write_plane<T: Sample>(d: &mut [T], base: usize, max: f32, c: usize, pixels: &[Pixel]) {
    for (v, px) in d[base..base + pixels.len()].iter_mut().zip(pixels) {
        *v = T::store(px[c], max);
    }
}

fn check_bit_depth(data: &ImageData, bd: BitDepth) -> Result<()> {
    let ok = matches!(
        (data, bd),
        (ImageData::U8(_), BitDepth::UInt8)
            | (
                ImageData::U16(_),
                BitDepth::UInt10 | BitDepth::UInt12 | BitDepth::UInt14 | BitDepth::UInt16
            )
            | (ImageData::F16(_), BitDepth::F16)
            | (ImageData::F32(_), BitDepth::F32)
    );
    if ok {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "Bit depth '{bd}' does not match the image buffer type."
        )))
    }
}

/// Access to an image as a grid of RGBA pixels.
pub trait ImageDesc {
    /// Width in pixels.
    fn width(&self) -> usize;
    /// Height in pixels.
    fn height(&self) -> usize;
    /// Bit depth of the pixel values.
    fn bit_depth(&self) -> BitDepth;
    /// Read row `y` into `out` (length = width).
    fn read_row(&self, y: usize, out: &mut [Pixel]);
    /// Write row `y` from `pixels` (length = width).
    fn write_row(&mut self, y: usize, pixels: &[Pixel]);

    /// Row `y` as RGBA `f32` pixels, when the image stores it that way
    /// (32-bit float, RGBA order, adjacent pixels). The CPU processor then
    /// reads it in place instead of converting it with
    /// [`read_row`](Self::read_row). The default returns `None`.
    fn rgba_f32_row(&self, y: usize) -> Option<&[Pixel]> {
        let _ = y;
        None
    }

    /// Mutable version of [`rgba_f32_row`](Self::rgba_f32_row): the CPU
    /// processor then applies in place instead of using
    /// [`read_row`](Self::read_row) / [`write_row`](Self::write_row). The
    /// default returns `None`.
    fn rgba_f32_row_mut(&mut self, y: usize) -> Option<&mut [Pixel]> {
        let _ = y;
        None
    }
}

/// Interleaved image (RGBA, BGRA, ABGR, RGB or BGR).
#[derive(Debug)]
pub struct PackedImageDesc<'a> {
    data: ImageData<'a>,
    width: usize,
    height: usize,
    ordering: ChannelOrdering,
    bit_depth: BitDepth,
    /// Distance between pixels, in elements.
    x_stride: usize,
    /// Distance between rows, in elements.
    y_stride: usize,
}

impl<'a> PackedImageDesc<'a> {
    /// Tightly packed image with `num_channels` (3 = RGB, 4 = RGBA).
    pub fn new(
        data: ImageData<'a>,
        width: usize,
        height: usize,
        num_channels: usize,
    ) -> Result<Self> {
        let ordering = match num_channels {
            3 => ChannelOrdering::Rgb,
            4 => ChannelOrdering::Rgba,
            n => {
                return Err(Error::msg(format!(
                    "PackedImageDesc: unsupported number of channels {n}."
                )))
            }
        };
        Self::with_ordering(data, width, height, ordering)
    }

    /// Tightly packed image with a given channel ordering.
    pub fn with_ordering(
        data: ImageData<'a>,
        width: usize,
        height: usize,
        ordering: ChannelOrdering,
    ) -> Result<Self> {
        let nc = ordering.num_channels();
        let bd = data.default_bit_depth();
        Self::with_strides(data, width, height, ordering, bd, nc, nc * width)
    }

    /// Full control: bit depth (for 10/12/14 bit data in `u16` buffers) and
    /// strides in elements.
    pub fn with_strides(
        data: ImageData<'a>,
        width: usize,
        height: usize,
        ordering: ChannelOrdering,
        bit_depth: BitDepth,
        x_stride: usize,
        y_stride: usize,
    ) -> Result<Self> {
        check_bit_depth(&data, bit_depth)?;
        let nc = ordering.num_channels();
        if x_stride < nc {
            return Err(Error::msg(
                "PackedImageDesc: x stride is smaller than the number of channels.",
            ));
        }
        if height > 0 && width > 0 {
            let needed = (height - 1) * y_stride + (width - 1) * x_stride + nc;
            if data.len() < needed {
                return Err(Error::msg(format!(
                    "PackedImageDesc: buffer too small ({} elements, {} needed).",
                    data.len(),
                    needed
                )));
            }
        }
        Ok(Self {
            data,
            width,
            height,
            ordering,
            bit_depth,
            x_stride,
            y_stride,
        })
    }

    /// Channel ordering of the packed pixels.
    pub fn channel_ordering(&self) -> ChannelOrdering {
        self.ordering
    }

    /// Layout of row `y`.
    fn layout(&self, y: usize) -> PackedLayout {
        let (rgb, alpha) = self.offsets();
        PackedLayout {
            base: y * self.y_stride,
            x_stride: self.x_stride,
            rgb,
            alpha,
        }
    }

    /// The element range of row `y` when it holds adjacent RGBA `f32`
    /// pixels (see [`ImageDesc::rgba_f32_row`]).
    fn rgba_f32_range(&self, y: usize) -> Option<std::ops::Range<usize>> {
        if self.ordering != ChannelOrdering::Rgba || self.x_stride != 4 || y >= self.height {
            return None;
        }
        let base = y * self.y_stride;
        Some(base..base + 4 * self.width)
    }

    /// Channel offsets for R, G, B, A (A is `None` for 3-channel images).
    fn offsets(&self) -> ([usize; 3], Option<usize>) {
        match self.ordering {
            ChannelOrdering::Rgba => ([0, 1, 2], Some(3)),
            ChannelOrdering::Bgra => ([2, 1, 0], Some(3)),
            ChannelOrdering::Abgr => ([3, 2, 1], Some(0)),
            ChannelOrdering::Rgb => ([0, 1, 2], None),
            ChannelOrdering::Bgr => ([2, 1, 0], None),
        }
    }
}

impl ImageDesc for PackedImageDesc<'_> {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }
    fn read_row(&self, y: usize, out: &mut [Pixel]) {
        let layout = self.layout(y);
        let scale = 1.0 / self.bit_depth.max_value() as f32;
        let n = self.width.min(out.len());
        let out = &mut out[..n];
        with_typed!(&self.data, d => read_packed(d, layout, scale, out));
    }
    fn write_row(&mut self, y: usize, pixels: &[Pixel]) {
        let layout = self.layout(y);
        let max = self.bit_depth.max_value() as f32;
        let n = self.width.min(pixels.len());
        let pixels = &pixels[..n];
        with_typed!(&mut self.data, d => write_packed(d, layout, max, pixels));
    }
    fn rgba_f32_row(&self, y: usize) -> Option<&[Pixel]> {
        let range = self.rgba_f32_range(y)?;
        match &self.data {
            ImageData::F32(d) => Some(d[range].as_chunks::<4>().0),
            _ => None,
        }
    }
    fn rgba_f32_row_mut(&mut self, y: usize) -> Option<&mut [Pixel]> {
        let range = self.rgba_f32_range(y)?;
        match &mut self.data {
            ImageData::F32(d) => Some(d[range].as_chunks_mut::<4>().0),
            _ => None,
        }
    }
}

/// Planar image: one buffer per channel (alpha optional).
#[derive(Debug)]
pub struct PlanarImageDesc<'a> {
    r: ImageData<'a>,
    g: ImageData<'a>,
    b: ImageData<'a>,
    a: Option<ImageData<'a>>,
    width: usize,
    height: usize,
    bit_depth: BitDepth,
}

impl<'a> PlanarImageDesc<'a> {
    /// Planar image from one buffer per channel; fails if the buffers do not share the same type or are smaller than `width * height`.
    pub fn new(
        r: ImageData<'a>,
        g: ImageData<'a>,
        b: ImageData<'a>,
        a: Option<ImageData<'a>>,
        width: usize,
        height: usize,
    ) -> Result<Self> {
        let bd = r.default_bit_depth();
        for c in [&g, &b].into_iter().chain(a.iter()) {
            if c.default_bit_depth() != bd {
                return Err(Error::msg(
                    "PlanarImageDesc: all channels must share the same type.",
                ));
            }
        }
        let n = width * height;
        for c in [&r, &g, &b].into_iter().chain(a.iter()) {
            if c.len() < n {
                return Err(Error::msg("PlanarImageDesc: channel buffer too small."));
            }
        }
        Ok(Self {
            r,
            g,
            b,
            a,
            width,
            height,
            bit_depth: bd,
        })
    }
}

impl ImageDesc for PlanarImageDesc<'_> {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }
    fn read_row(&self, y: usize, out: &mut [Pixel]) {
        let scale = 1.0 / self.bit_depth.max_value() as f32;
        let base = y * self.width;
        let n = self.width.min(out.len());
        let out = &mut out[..n];
        with_typed!(&self.r, d => read_plane(d, base, scale, 0, out));
        with_typed!(&self.g, d => read_plane(d, base, scale, 1, out));
        with_typed!(&self.b, d => read_plane(d, base, scale, 2, out));
        match &self.a {
            Some(a) => with_typed!(a, d => read_plane(d, base, scale, 3, out)),
            None => out.iter_mut().for_each(|px| px[3] = 1.0),
        }
    }
    fn write_row(&mut self, y: usize, pixels: &[Pixel]) {
        let max = self.bit_depth.max_value() as f32;
        let base = y * self.width;
        let n = self.width.min(pixels.len());
        let pixels = &pixels[..n];
        with_typed!(&mut self.r, d => write_plane(d, base, max, 0, pixels));
        with_typed!(&mut self.g, d => write_plane(d, base, max, 1, pixels));
        with_typed!(&mut self.b, d => write_plane(d, base, max, 2, pixels));
        if let Some(a) = self.a.as_mut() {
            with_typed!(a, d => write_plane(d, base, max, 3, pixels));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_f32_rows_are_exposed_only_for_adjacent_rgba_f32_pixels() {
        let mut data = (0..2 * 3 * 5).map(|v| v as f32).collect::<Vec<_>>();
        // 2 x 3 RGBA, rows padded to 10 values: rows are exposed in place.
        let mut img = PackedImageDesc::with_strides(
            ImageData::F32(&mut data),
            2,
            3,
            ChannelOrdering::Rgba,
            BitDepth::F32,
            4,
            10,
        )
        .unwrap();
        assert_eq!(
            img.rgba_f32_row(1),
            Some(&[[10.0, 11.0, 12.0, 13.0], [14.0, 15.0, 16.0, 17.0]][..])
        );
        img.rgba_f32_row_mut(2).unwrap()[1][3] = -1.0;
        assert_eq!(img.rgba_f32_row(3), None);
        assert_eq!(data[27], -1.0);

        // Pixel padding or another channel order: converted.
        let mut data = vec![0.0f32; 30];
        let img = PackedImageDesc::with_strides(
            ImageData::F32(&mut data),
            2,
            3,
            ChannelOrdering::Rgba,
            BitDepth::F32,
            5,
            10,
        )
        .unwrap();
        assert_eq!(img.rgba_f32_row(0), None);
        let img =
            PackedImageDesc::with_ordering(ImageData::F32(&mut data), 2, 3, ChannelOrdering::Bgra)
                .unwrap();
        assert_eq!(img.rgba_f32_row(0), None);
        let mut data = vec![0u16; 24];
        let mut img = PackedImageDesc::new(ImageData::U16(&mut data), 2, 3, 4).unwrap();
        assert_eq!(img.rgba_f32_row_mut(0), None);
    }

    #[test]
    fn integer_values_are_normalized_and_quantized() {
        let mut data = [0u8, 51, 255, 128, 7, 8, 9, 10];
        let mut img = PackedImageDesc::new(ImageData::U8(&mut data), 2, 1, 4).unwrap();
        let mut row = [[0.0f32; 4]; 2];
        img.read_row(0, &mut row);
        // (Multiplied by the reciprocal of the maximum, as in OCIO.)
        let scale = 1.0f32 / 255.0;
        assert_eq!(row[0], [0.0, 51.0 * scale, 1.0, 128.0 * scale]);
        img.write_row(
            0,
            &[[-0.5, 0.5, 2.0, f32::NAN], [0.4999, 1.0 / 255.0, 0.0, 1.0]],
        );
        // Round half up and clamp; NaN gives 0.
        assert_eq!(data, [0, 128, 255, 0, 127, 1, 0, 255]);
    }

    #[test]
    fn planar_rows_without_alpha_read_an_opaque_alpha() {
        let (mut r, mut g, mut b) = ([0.5f32, 1.0], [0.25f32, 2.0], [0.0f32, 3.0]);
        let mut img = PlanarImageDesc::new(
            ImageData::F32(&mut r),
            ImageData::F32(&mut g),
            ImageData::F32(&mut b),
            None,
            2,
            1,
        )
        .unwrap();
        let mut row = [[9.0f32; 4]; 2];
        img.read_row(0, &mut row);
        assert_eq!(row, [[0.5, 0.25, 0.0, 1.0], [1.0, 2.0, 3.0, 1.0]]);
        img.write_row(0, &[[4.0, 5.0, 6.0, 7.0], [8.0, 9.0, 10.0, 11.0]]);
        assert_eq!((r, g, b), ([4.0, 8.0], [5.0, 9.0], [6.0, 10.0]));
    }
}
