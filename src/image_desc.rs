//! Image descriptions used by `CpuProcessor::apply` (port of `ImageDesc`).
//!
//! Images are described by borrowed, typed buffers. Integer bit depths are
//! normalized by their maximum code value (e.g. 10ui values live in a `u16`
//! buffer and 1023 maps to 1.0).

use crate::error::{Error, Result};
use crate::ops::Pixel;
use crate::types::{BitDepth, ChannelOrdering};
use half::f16;

/// Typed pixel storage.
#[derive(Debug)]
pub enum ImageData<'a> {
    U8(&'a mut [u8]),
    /// 10, 12, 14 or 16 bit integers.
    U16(&'a mut [u16]),
    F16(&'a mut [f16]),
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

    #[inline]
    fn get(&self, i: usize, scale: f32) -> f32 {
        match self {
            ImageData::U8(d) => d[i] as f32 * scale,
            ImageData::U16(d) => d[i] as f32 * scale,
            ImageData::F16(d) => d[i].to_f32(),
            ImageData::F32(d) => d[i],
        }
    }

    #[inline]
    fn set(&mut self, i: usize, v: f32, max: f32) {
        match self {
            ImageData::U8(d) => d[i] = quantize(v, max) as u8,
            ImageData::U16(d) => d[i] = quantize(v, max) as u16,
            ImageData::F16(d) => d[i] = f16::from_f32(v),
            ImageData::F32(d) => d[i] = v,
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
        x.floor()
    }
}

fn check_bit_depth(data: &ImageData, bd: BitDepth) -> Result<()> {
    let ok = matches!(
        (data, bd),
        (ImageData::U8(_), BitDepth::UInt8)
            | (ImageData::U16(_), BitDepth::UInt10 | BitDepth::UInt12 | BitDepth::UInt14 | BitDepth::UInt16)
            | (ImageData::F16(_), BitDepth::F16)
            | (ImageData::F32(_), BitDepth::F32)
    );
    if ok {
        Ok(())
    } else {
        Err(Error::msg(format!("Bit depth '{bd}' does not match the image buffer type.")))
    }
}

/// Access to an image as a grid of RGBA pixels.
pub trait ImageDesc {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn bit_depth(&self) -> BitDepth;
    /// Read row `y` into `out` (length = width).
    fn read_row(&self, y: usize, out: &mut [Pixel]);
    /// Write row `y` from `pixels` (length = width).
    fn write_row(&mut self, y: usize, pixels: &[Pixel]);
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
    pub fn new(data: ImageData<'a>, width: usize, height: usize, num_channels: usize) -> Result<Self> {
        let ordering = match num_channels {
            3 => ChannelOrdering::Rgb,
            4 => ChannelOrdering::Rgba,
            n => return Err(Error::msg(format!("PackedImageDesc: unsupported number of channels {n}."))),
        };
        Self::with_ordering(data, width, height, ordering)
    }

    /// Tightly packed image with a given channel ordering.
    pub fn with_ordering(data: ImageData<'a>, width: usize, height: usize, ordering: ChannelOrdering) -> Result<Self> {
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
            return Err(Error::msg("PackedImageDesc: x stride is smaller than the number of channels."));
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
        Ok(Self { data, width, height, ordering, bit_depth, x_stride, y_stride })
    }

    pub fn channel_ordering(&self) -> ChannelOrdering {
        self.ordering
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
        let (rgb, a) = self.offsets();
        let scale = 1.0 / self.bit_depth.max_value() as f32;
        let base = y * self.y_stride;
        for (x, px) in out.iter_mut().enumerate().take(self.width) {
            let i = base + x * self.x_stride;
            px[0] = self.data.get(i + rgb[0], scale);
            px[1] = self.data.get(i + rgb[1], scale);
            px[2] = self.data.get(i + rgb[2], scale);
            px[3] = match a {
                Some(ao) => self.data.get(i + ao, scale),
                None => 1.0,
            };
        }
    }
    fn write_row(&mut self, y: usize, pixels: &[Pixel]) {
        let (rgb, a) = self.offsets();
        let max = self.bit_depth.max_value() as f32;
        let base = y * self.y_stride;
        for (x, px) in pixels.iter().enumerate().take(self.width) {
            let i = base + x * self.x_stride;
            self.data.set(i + rgb[0], px[0], max);
            self.data.set(i + rgb[1], px[1], max);
            self.data.set(i + rgb[2], px[2], max);
            if let Some(ao) = a {
                self.data.set(i + ao, px[3], max);
            }
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
                return Err(Error::msg("PlanarImageDesc: all channels must share the same type."));
            }
        }
        let n = width * height;
        for c in [&r, &g, &b].into_iter().chain(a.iter()) {
            if c.len() < n {
                return Err(Error::msg("PlanarImageDesc: channel buffer too small."));
            }
        }
        Ok(Self { r, g, b, a, width, height, bit_depth: bd })
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
        for (x, px) in out.iter_mut().enumerate().take(self.width) {
            let i = base + x;
            px[0] = self.r.get(i, scale);
            px[1] = self.g.get(i, scale);
            px[2] = self.b.get(i, scale);
            px[3] = self.a.as_ref().map(|a| a.get(i, scale)).unwrap_or(1.0);
        }
    }
    fn write_row(&mut self, y: usize, pixels: &[Pixel]) {
        let max = self.bit_depth.max_value() as f32;
        let base = y * self.width;
        for (x, px) in pixels.iter().enumerate().take(self.width) {
            let i = base + x;
            self.r.set(i, px[0], max);
            self.g.set(i, px[1], max);
            self.b.set(i, px[2], max);
            if let Some(a) = self.a.as_mut() {
                a.set(i, px[3], max);
            }
        }
    }
}
