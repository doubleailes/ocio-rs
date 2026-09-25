//! Basic image input and output for the command line tools (port of the OCIO
//! `libutils/imageioapphelpers` `ImageIO` class).
//!
//! The C++ class relies on OpenImageIO or OpenEXR; this port uses the pure
//! Rust `exr` crate for OpenEXR files and the `image` crate for PNG, TIFF and
//! JPEG files. Images are kept in their native bit depth (8-bit, 16-bit,
//! half or float) with 3 (RGB) or 4 (RGBA) interleaved channels: gray images
//! are expanded to RGB and gray + alpha images to RGBA when read.
//!
//! Metadata attributes are written to the header of OpenEXR files; the
//! other formats ignore them.

use half::f16;
use ocio::{BitDepth, ChannelOrdering, Error, ImageData, PackedImageDesc, Result};

/// The pixel buffer, in the native bit depth of the image.
#[derive(Debug, Clone, PartialEq)]
pub enum PixelData {
    /// 8-bit unsigned integer samples.
    U8(Vec<u8>),
    /// 16-bit unsigned integer samples.
    U16(Vec<u16>),
    /// Half float samples.
    F16(Vec<f16>),
    /// Float samples.
    F32(Vec<f32>),
}

impl PixelData {
    fn bit_depth(&self) -> BitDepth {
        match self {
            PixelData::U8(_) => BitDepth::UInt8,
            PixelData::U16(_) => BitDepth::UInt16,
            PixelData::F16(_) => BitDepth::F16,
            PixelData::F32(_) => BitDepth::F32,
        }
    }

    fn zeros(bit_depth: BitDepth, len: usize) -> Result<Self> {
        Ok(match bit_depth {
            BitDepth::UInt8 => PixelData::U8(vec![0; len]),
            BitDepth::UInt16 => PixelData::U16(vec![0; len]),
            BitDepth::F16 => PixelData::F16(vec![f16::ZERO; len]),
            BitDepth::F32 => PixelData::F32(vec![0.0; len]),
            other => return Err(unsupported_bit_depth(other)),
        })
    }

    /// All the samples as normalized floats.
    fn to_f32(&self) -> Vec<f32> {
        match self {
            PixelData::U8(v) => v.iter().map(|x| f32::from(*x) / 255.0).collect(),
            PixelData::U16(v) => v.iter().map(|x| f32::from(*x) / 65535.0).collect(),
            PixelData::F16(v) => v.iter().map(|x| x.to_f32()).collect(),
            PixelData::F32(v) => v.clone(),
        }
    }

    /// Convert to another bit depth (integer conversions are rounded and
    /// clamped, as OpenImageIO does).
    fn convert(&self, bit_depth: BitDepth) -> Result<PixelData> {
        if bit_depth == self.bit_depth() {
            return Ok(self.clone());
        }
        let values = self.to_f32();
        Ok(match bit_depth {
            BitDepth::UInt8 => {
                PixelData::U8(values.iter().map(|v| quantize(*v, 255.0) as u8).collect())
            }
            BitDepth::UInt16 => PixelData::U16(
                values
                    .iter()
                    .map(|v| quantize(*v, 65535.0) as u16)
                    .collect(),
            ),
            BitDepth::F16 => PixelData::F16(values.iter().map(|v| f16::from_f32(*v)).collect()),
            BitDepth::F32 => PixelData::F32(values),
            other => return Err(unsupported_bit_depth(other)),
        })
    }
}

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

fn unsupported_bit_depth(bd: BitDepth) -> Error {
    Error::msg(format!("Error: Unsupported bitdepth: {}", bd.as_str()))
}

/// A metadata attribute value.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// A string attribute.
    Str(String),
    /// A float attribute.
    Float(f32),
    /// An integer attribute.
    Int(i32),
}

/// An image loaded in memory (port of `OCIO::ImageIO`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageIO {
    width: usize,
    height: usize,
    num_channels: usize,
    data: PixelData,
    attributes: Vec<(String, AttributeValue)>,
}

impl Default for ImageIO {
    fn default() -> Self {
        ImageIO {
            width: 0,
            height: 0,
            num_channels: 3,
            data: PixelData::F32(Vec::new()),
            attributes: Vec::new(),
        }
    }
}

fn num_channels_of(order: ChannelOrdering) -> Result<usize> {
    match order {
        ChannelOrdering::Rgba => Ok(4),
        ChannelOrdering::Rgb => Ok(3),
        other => Err(Error::msg(format!(
            "Error: Unsupported channel ordering: {other:?}"
        ))),
    }
}

fn extension_of(filename: &str) -> String {
    std::path::Path::new(filename)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_exr_file(filename: &str) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    match std::fs::File::open(filename) {
        Ok(mut f) => f.read_exact(&mut magic).is_ok() && magic == [0x76, 0x2f, 0x31, 0x01],
        Err(_) => extension_of(filename) == "exr",
    }
}

impl ImageIO {
    /// Description of the image I/O backend.
    pub fn version() -> String {
        "ImageIO: pure Rust 'exr' and 'image' crates (OpenEXR, PNG, TIFF, JPEG)".to_string()
    }

    /// Allocate an image buffer filled with zeros.
    pub fn new(
        width: usize,
        height: usize,
        order: ChannelOrdering,
        bit_depth: BitDepth,
    ) -> Result<Self> {
        let mut img = ImageIO::default();
        img.init(width, height, order, bit_depth)?;
        Ok(img)
    }

    /// Load an image in its native bit depth.
    pub fn open(filename: &str) -> Result<Self> {
        let mut img = ImageIO::default();
        img.read(filename, BitDepth::Unknown)?;
        Ok(img)
    }

    /// Initialize to an empty image buffer.
    pub fn init(
        &mut self,
        width: usize,
        height: usize,
        order: ChannelOrdering,
        bit_depth: BitDepth,
    ) -> Result<()> {
        let nc = num_channels_of(order)?;
        self.data = PixelData::zeros(bit_depth, width * height * nc)?;
        self.width = width;
        self.height = height;
        self.num_channels = nc;
        self.attributes.clear();
        Ok(())
    }

    /// Initialize to an empty image buffer having the size, channels and
    /// attributes of `img` and the bit depth `bit_depth` (the one of `img`
    /// when unknown). The color interop id is not propagated.
    pub fn init_from(&mut self, img: &ImageIO, bit_depth: BitDepth) -> Result<()> {
        let bd = if bit_depth == BitDepth::Unknown {
            img.bit_depth()
        } else {
            bit_depth
        };
        self.data = PixelData::zeros(bd, img.width * img.height * img.num_channels)?;
        self.width = img.width;
        self.height = img.height;
        self.num_channels = img.num_channels;
        self.attributes = img.attributes.clone();
        self.attribute("colorInteropID", AttributeValue::Str("unknown".to_string()));
        Ok(())
    }

    /// Printable information about the image.
    pub fn image_desc_str(&self) -> String {
        let names = self.channel_names();
        let chans: Vec<&str> = (0..self.num_channels)
            .map(|i| names.get(i).copied().unwrap_or("Unknown"))
            .collect();
        format!(
            "\nImage: [{}x{}] {} {}\n",
            self.width,
            self.height,
            self.bit_depth().as_str(),
            chans.join(", ")
        )
    }

    /// Image width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Image height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Bit depth of the pixel buffer.
    pub fn bit_depth(&self) -> BitDepth {
        self.data.bit_depth()
    }

    /// Number of channels (3 or 4).
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    /// Channel ordering (RGBA for 4 channels, RGB otherwise).
    pub fn channel_order(&self) -> ChannelOrdering {
        if self.num_channels == 4 {
            ChannelOrdering::Rgba
        } else {
            ChannelOrdering::Rgb
        }
    }

    /// Channel names.
    pub fn channel_names(&self) -> Vec<&'static str> {
        if self.num_channels == 4 {
            vec!["R", "G", "B", "A"]
        } else {
            vec!["R", "G", "B"]
        }
    }

    /// Size in bytes of one channel.
    pub fn chan_stride_bytes(&self) -> usize {
        match self.bit_depth() {
            BitDepth::UInt8 => 1,
            BitDepth::UInt16 | BitDepth::F16 => 2,
            _ => 4,
        }
    }

    /// Size in bytes of one pixel.
    pub fn x_stride_bytes(&self) -> usize {
        self.num_channels * self.chan_stride_bytes()
    }

    /// Size in bytes of one line.
    pub fn y_stride_bytes(&self) -> usize {
        self.width * self.x_stride_bytes()
    }

    /// Size in bytes of the image.
    pub fn image_bytes(&self) -> usize {
        self.y_stride_bytes() * self.height
    }

    /// The pixel buffer.
    pub fn data(&self) -> &PixelData {
        &self.data
    }

    /// The pixel buffer (mutable).
    pub fn data_mut(&mut self) -> &mut PixelData {
        &mut self.data
    }

    /// The float pixels, if the buffer holds floats.
    pub fn data_f32(&self) -> Option<&[f32]> {
        match &self.data {
            PixelData::F32(v) => Some(v),
            _ => None,
        }
    }

    /// The float pixels, if the buffer holds floats (mutable).
    pub fn data_f32_mut(&mut self) -> Option<&mut [f32]> {
        match &mut self.data {
            PixelData::F32(v) => Some(v),
            _ => None,
        }
    }

    /// Metadata attributes.
    pub fn attributes(&self) -> &[(String, AttributeValue)] {
        &self.attributes
    }

    /// Set a metadata attribute (replacing any attribute of the same name).
    pub fn attribute(&mut self, name: &str, value: AttributeValue) {
        if let Some(a) = self.attributes.iter_mut().find(|(n, _)| n == name) {
            a.1 = value;
        } else {
            self.attributes.push((name.to_string(), value));
        }
    }

    /// Image description to process the image with a CPU processor.
    pub fn image_desc(&mut self) -> Result<PackedImageDesc<'_>> {
        let (w, h, nc) = (self.width, self.height, self.num_channels);
        let data = match &mut self.data {
            PixelData::U8(v) => ImageData::U8(v),
            PixelData::U16(v) => ImageData::U16(v),
            PixelData::F16(v) => ImageData::F16(v),
            PixelData::F32(v) => ImageData::F32(v),
        };
        PackedImageDesc::new(data, w, h, nc)
    }

    /// Read an image, converting it to `bit_depth` unless unknown.
    pub fn read(&mut self, filename: &str, bit_depth: BitDepth) -> Result<()> {
        if !matches!(
            bit_depth,
            BitDepth::Unknown | BitDepth::UInt8 | BitDepth::UInt16 | BitDepth::F16 | BitDepth::F32
        ) {
            return Err(unsupported_bit_depth(bit_depth));
        }
        let img = if is_exr_file(filename) {
            read_exr(filename)
        } else {
            read_other(filename)
        }
        .map_err(|e| Error::msg(format!("Error: Could not read image: {e}")))?;
        *self = img;
        if bit_depth != BitDepth::Unknown {
            self.data = self.data.convert(bit_depth)?;
        }
        Ok(())
    }

    /// Write the image, converting it to `bit_depth` unless unknown. As with
    /// OpenImageIO, the bit depth is adjusted to what the file format
    /// supports.
    pub fn write(&self, filename: &str, bit_depth: BitDepth) -> Result<()> {
        let requested = if bit_depth == BitDepth::Unknown {
            self.bit_depth()
        } else {
            bit_depth
        };
        if !matches!(
            requested,
            BitDepth::UInt8 | BitDepth::UInt16 | BitDepth::F16 | BitDepth::F32
        ) {
            return Err(unsupported_bit_depth(requested));
        }
        let ext = extension_of(filename);
        let res = match ext.as_str() {
            "exr" => self.write_exr(filename, requested),
            "png" | "tif" | "tiff" | "jpg" | "jpeg" => self.write_other(filename, &ext, requested),
            _ => Err(format!("unsupported file format '{filename}'")),
        };
        res.map_err(|e| Error::msg(format!("Error: Could not write image: {e}")))
    }

    fn write_exr(&self, filename: &str, bit_depth: BitDepth) -> std::result::Result<(), String> {
        use exr::prelude::*;
        let n = self.width * self.height;
        let values = self.data.to_f32();
        let names = self.channel_names();
        let mut channels = Vec::new();
        for (c, name) in names.iter().enumerate().take(self.num_channels) {
            let samples: Vec<f32> = (0..n).map(|i| values[i * self.num_channels + c]).collect();
            let samples = if bit_depth == BitDepth::F32 {
                FlatSamples::F32(samples)
            } else {
                FlatSamples::F16(samples.into_iter().map(f16::from_f32).collect())
            };
            channels.push(AnyChannel::new(*name, samples));
        }
        let mut attributes = LayerAttributes::default();
        for (name, value) in &self.attributes {
            let key = match Text::new_or_none(name) {
                Some(k) => k,
                None => continue,
            };
            let v = match value {
                self::AttributeValue::Str(s) => match Text::new_or_none(s) {
                    Some(t) => exr::meta::attribute::AttributeValue::Text(t),
                    None => continue,
                },
                self::AttributeValue::Float(f) => exr::meta::attribute::AttributeValue::F32(*f),
                self::AttributeValue::Int(i) => exr::meta::attribute::AttributeValue::I32(*i),
            };
            attributes.other.insert(key, v);
        }
        let layer = Layer::new(
            (self.width, self.height),
            attributes,
            Encoding::FAST_LOSSLESS,
            AnyChannels::sort(SmallVec::from_vec(channels)),
        );
        let image = Image::from_layer(layer);
        image.write().to_file(filename).map_err(|e| e.to_string())
    }

    fn write_other(
        &self,
        filename: &str,
        ext: &str,
        bit_depth: BitDepth,
    ) -> std::result::Result<(), String> {
        use image::DynamicImage;
        let (w, h) = (self.width as u32, self.height as u32);
        let rgba = self.num_channels == 4;
        let bad = || "invalid image buffer".to_string();
        // Pick the bit depth supported by the file format.
        let target = match ext {
            "jpg" | "jpeg" => BitDepth::UInt8,
            "png" => {
                if bit_depth == BitDepth::UInt8 {
                    BitDepth::UInt8
                } else {
                    BitDepth::UInt16
                }
            }
            _ => match bit_depth {
                BitDepth::UInt8 => BitDepth::UInt8,
                BitDepth::UInt16 => BitDepth::UInt16,
                _ => BitDepth::F32,
            },
        };
        let data = self.data.convert(target).map_err(|e| e.to_string())?;
        let img = match (data, rgba) {
            (PixelData::U8(v), false) => {
                DynamicImage::ImageRgb8(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::U8(v), true) => {
                DynamicImage::ImageRgba8(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::U16(v), false) => {
                DynamicImage::ImageRgb16(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::U16(v), true) => {
                DynamicImage::ImageRgba16(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::F32(v), false) => {
                DynamicImage::ImageRgb32F(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::F32(v), true) => {
                DynamicImage::ImageRgba32F(image::ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
            }
            (PixelData::F16(_), _) => return Err(bad()),
        };
        // JPEG does not support an alpha channel.
        let img = if matches!(ext, "jpg" | "jpeg") && rgba {
            DynamicImage::ImageRgb8(img.to_rgb8())
        } else {
            img
        };
        img.save(filename).map_err(|e| e.to_string())
    }
}

fn read_exr(filename: &str) -> std::result::Result<ImageIO, String> {
    use exr::prelude::*;
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_file(filename)
        .map_err(|e| e.to_string())?;
    let layer = image.layer_data;
    let (width, height) = (layer.size.width(), layer.size.height());
    let list = &layer.channel_data.list;
    // As OCIO's EXR reader (`imageio_exr.cpp`): the R, G and B channels are
    // read (zero filled if missing) and A only if present; no other channel
    // is preserved. The image is half float unless one of these channels is
    // float.
    let find = |name: &str| list.iter().position(|c| c.name.to_string() == name);
    let rgb = [find("R"), find("G"), find("B")];
    let alpha = find("A");
    let selected: Vec<Option<usize>> = match alpha {
        Some(a) => vec![rgb[0], rgb[1], rgb[2], Some(a)],
        None => rgb.to_vec(),
    };
    let any_float = selected
        .iter()
        .flatten()
        .any(|i| matches!(list[*i].sample_data, FlatSamples::F32(_)));
    let nc = selected.len();
    let n = width * height;
    let data = if any_float {
        let mut out = vec![0.0f32; n * nc];
        for (c, idx) in selected.iter().enumerate() {
            let Some(idx) = idx else { continue };
            for (i, s) in list[*idx].sample_data.values_as_f32().enumerate().take(n) {
                out[i * nc + c] = s;
            }
        }
        PixelData::F32(out)
    } else {
        let mut out = vec![f16::ZERO; n * nc];
        for (c, idx) in selected.iter().enumerate() {
            let Some(idx) = idx else { continue };
            match &list[*idx].sample_data {
                FlatSamples::F16(v) => {
                    for (i, s) in v.iter().enumerate().take(n) {
                        out[i * nc + c] = *s;
                    }
                }
                other => {
                    for (i, s) in other.values_as_f32().enumerate().take(n) {
                        out[i * nc + c] = f16::from_f32(s);
                    }
                }
            }
        }
        PixelData::F16(out)
    };
    let mut attributes = Vec::new();
    for (k, v) in &layer.attributes.other {
        let value = match v {
            exr::meta::attribute::AttributeValue::Text(t) => {
                self::AttributeValue::Str(t.to_string())
            }
            exr::meta::attribute::AttributeValue::F32(f) => self::AttributeValue::Float(*f),
            exr::meta::attribute::AttributeValue::I32(i) => self::AttributeValue::Int(*i),
            _ => continue,
        };
        attributes.push((k.to_string(), value));
    }
    attributes.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(ImageIO {
        width,
        height,
        num_channels: nc,
        data,
        attributes,
    })
}

fn read_other(filename: &str) -> std::result::Result<ImageIO, String> {
    use image::DynamicImage;
    let img = image::open(filename).map_err(|e| e.to_string())?;
    let (width, height) = (img.width() as usize, img.height() as usize);
    let (nc, data) = match img {
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_) => {
            (3, PixelData::U8(img.to_rgb8().into_raw()))
        }
        DynamicImage::ImageLumaA8(_) | DynamicImage::ImageRgba8(_) => {
            (4, PixelData::U8(img.to_rgba8().into_raw()))
        }
        DynamicImage::ImageLuma16(_) | DynamicImage::ImageRgb16(_) => {
            (3, PixelData::U16(img.to_rgb16().into_raw()))
        }
        DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgba16(_) => {
            (4, PixelData::U16(img.to_rgba16().into_raw()))
        }
        DynamicImage::ImageRgb32F(_) => (3, PixelData::F32(img.to_rgb32f().into_raw())),
        _ => (4, PixelData::F32(img.to_rgba32f().into_raw())),
    };
    Ok(ImageIO {
        width,
        height,
        num_channels: nc,
        data,
        attributes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("ocio_tools_imageio_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().into_owned()
    }

    fn gradient(nc: usize) -> ImageIO {
        let order = if nc == 4 {
            ChannelOrdering::Rgba
        } else {
            ChannelOrdering::Rgb
        };
        let mut img = ImageIO::new(4, 2, order, BitDepth::F32).unwrap();
        for (i, v) in img.data_f32_mut().unwrap().iter_mut().enumerate() {
            *v = i as f32 / 32.0;
        }
        img
    }

    #[test]
    fn exr_channels_as_ocio() {
        use exr::prelude::*;
        // Luminance + alpha: as in OCIO, RGB are zero filled and A is kept.
        let path = temp_path("ya.exr");
        let (w, h) = (2usize, 1usize);
        let y = FlatSamples::F16(vec![f16::from_f32(0.5), f16::from_f32(0.25)]);
        let a = FlatSamples::F16(vec![f16::from_f32(1.0), f16::from_f32(0.75)]);
        let channels = AnyChannels::sort(SmallVec::from_vec(vec![
            AnyChannel::new("Y", y),
            AnyChannel::new("A", a),
        ]));
        Image::from_channels((w, h), channels)
            .write()
            .to_file(&path)
            .unwrap();
        let img = ImageIO::open(&path).unwrap();
        assert_eq!(img.num_channels(), 4);
        match &img.data {
            PixelData::F16(v) => {
                let v: Vec<f32> = v.iter().map(|x| x.to_f32()).collect();
                assert_eq!(v, vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.75]);
            }
            _ => panic!("expected a half float image"),
        }

        // Missing G channel, float R: float image, G zero filled, no alpha.
        let path = temp_path("rb.exr");
        let channels = AnyChannels::sort(SmallVec::from_vec(vec![
            AnyChannel::new("R", FlatSamples::F32(vec![0.5, 0.25])),
            AnyChannel::new("B", FlatSamples::F16(vec![f16::from_f32(1.0); 2])),
        ]));
        Image::from_channels((w, h), channels)
            .write()
            .to_file(&path)
            .unwrap();
        let img = ImageIO::open(&path).unwrap();
        assert_eq!(img.num_channels(), 3);
        match &img.data {
            PixelData::F32(v) => assert_eq!(v, &vec![0.5, 0.0, 1.0, 0.25, 0.0, 1.0]),
            _ => panic!("expected a float image"),
        }
    }

    #[test]
    fn exr_round_trip() {
        let img = gradient(4);
        let path = temp_path("rt.exr");
        img.write(&path, BitDepth::Unknown).unwrap();
        let back = ImageIO::open(&path).unwrap();
        assert_eq!(back.width(), 4);
        assert_eq!(back.height(), 2);
        assert_eq!(back.num_channels(), 4);
        assert_eq!(back.bit_depth(), BitDepth::F32);
        assert_eq!(back.data_f32(), img.data_f32());

        img.write(&path, BitDepth::F16).unwrap();
        let back = ImageIO::open(&path).unwrap();
        assert_eq!(back.bit_depth(), BitDepth::F16);
    }

    #[test]
    fn exr_attributes() {
        let mut img = gradient(3);
        img.attribute("oiio:ColorSpace", AttributeValue::Str("lin".into()));
        img.attribute("myfloat", AttributeValue::Float(1.5));
        img.attribute("myint", AttributeValue::Int(3));
        let path = temp_path("attr.exr");
        img.write(&path, BitDepth::Unknown).unwrap();
        let back = ImageIO::open(&path).unwrap();
        let attrs = back.attributes();
        assert!(attrs.contains(&("myfloat".to_string(), AttributeValue::Float(1.5))));
        assert!(attrs.contains(&("myint".to_string(), AttributeValue::Int(3))));
        assert!(attrs.contains(&(
            "oiio:ColorSpace".to_string(),
            AttributeValue::Str("lin".into())
        )));
    }

    #[test]
    fn png_tiff_round_trip() {
        let img = gradient(3);
        let path = temp_path("rt.png");
        img.write(&path, BitDepth::UInt8).unwrap();
        let back = ImageIO::open(&path).unwrap();
        assert_eq!(back.bit_depth(), BitDepth::UInt8);
        assert_eq!(back.num_channels(), 3);
        if let PixelData::U8(v) = back.data() {
            assert_eq!(v[1], (255.0f32 / 32.0 + 0.5).floor() as u8);
        }

        let path = temp_path("rt.tif");
        img.write(&path, BitDepth::Unknown).unwrap();
        let back = ImageIO::open(&path).unwrap();
        assert_eq!(back.bit_depth(), BitDepth::F32);
        assert_eq!(back.data_f32(), img.data_f32());

        let path = temp_path("rt.jpg");
        gradient(4).write(&path, BitDepth::Unknown).unwrap();
        let back = ImageIO::open(&path).unwrap();
        assert_eq!(back.num_channels(), 3);
    }

    #[test]
    fn errors() {
        let img = gradient(3);
        let err = img
            .write(&temp_path("x.bmpx"), BitDepth::Unknown)
            .unwrap_err();
        assert!(err.message().starts_with("Error: Could not write image: "));
        let err = ImageIO::open(&temp_path("missing.exr")).unwrap_err();
        assert!(err.message().starts_with("Error: Could not read image: "));
        assert_eq!(
            ImageIO::new(1, 1, ChannelOrdering::Rgb, BitDepth::UInt10)
                .unwrap_err()
                .message(),
            "Error: Unsupported bitdepth: 10ui"
        );
        assert!(img.image_desc_str().contains("Image: [4x2] 32f R, G, B"));
    }
}
