//! GPU renderer of the 1D LUT op (port of `Lut1DOpGPU.cpp`).

use crate::error::{Error, Result};
use crate::gpu::shader_text::replace_all;
use crate::gpu::{GpuShaderCreator, GpuShaderText, TextureDimensions, TextureType};
use crate::nl;
use crate::ops::lut1d::{make_fast_lut1d_from_inverse, sanitize_float, Lut1DOp, Lut1DOpData};
use crate::types::{GpuLanguage, Lut1DHueAdjust, TransformDirection};

/// Largest finite half value.
const HALF_MAX: f32 = 65504.0;
/// Smallest normalized half value.
const HALF_NRM_MIN: f32 = 6.103_515_6e-05;

/// Pad the RGB LUT values to fill a `width` x `height` texture (port of
/// `CreatePaddedLutChannels`).
fn create_padded_lut_channels(width: usize, height: usize, channel: &[f32]) -> Vec<f32> {
    // The 1D LUT always contains 3 channels.
    let curr_width = channel.len() / 3;
    let mut padded = Vec::with_capacity(width * height * 3);
    let push_entry = |padded: &mut Vec<f32>, idx: usize| {
        for c in 0..3 {
            padded.push(sanitize_float(channel[3 * idx + c]));
        }
    };

    if height > 1 {
        // Fill the texture values.
        //
        // Make the last texel of a given row the same as the first texel of
        // its next row. This will preserve the continuity along row breaks as
        // long as the lookup position used by the sampler is based on
        // (width-1) to account for the 1 texel padding at the end of each row.
        let mut leftover = curr_width;

        let step = width - 1;
        let mut i = 0;
        while i < curr_width - step {
            for idx in i..i + step {
                push_entry(&mut padded, idx);
            }
            push_entry(&mut padded, i + step);
            leftover -= step;
            i += step;
        }

        // If there are still texels to fill, add them to the texture data.
        if leftover > 0 {
            for idx in (curr_width - leftover)..(curr_width - 1) {
                push_entry(&mut padded, idx);
            }
            push_entry(&mut padded, curr_width - 1);
        }
    } else {
        padded.extend(channel.iter().map(|v| sanitize_float(*v)));
    }

    // Pad the remaining of the texture with the last LUT entry.
    // Note: GPU Textures are expected a size of width*height.
    let missing = (width * height).saturating_sub(padded.len() / 3);
    for _ in 0..missing {
        push_entry(&mut padded, curr_width - 1);
    }
    padded
}

/// Pad the red values of the LUT to fill a `width` x `height` texture (port
/// of `CreatePaddedRedChannel`).
fn create_padded_red_channel(width: usize, height: usize, channel: &[f32]) -> Vec<f32> {
    // The 1D LUT always contains 3 channels.
    let curr_width = channel.len() / 3;
    let mut padded = Vec::with_capacity(width * height);
    let red = |idx: usize| sanitize_float(channel[3 * idx]);

    if height > 1 {
        // See create_padded_lut_channels.
        let mut leftover = curr_width;

        let step = width - 1;
        let mut i = 0;
        while i < curr_width - step {
            for idx in i..i + step {
                padded.push(red(idx));
            }
            padded.push(red(i + step));
            leftover -= step;
            i += step;
        }

        // If there are still texels to fill, add them to the texture data.
        if leftover > 0 {
            for idx in (curr_width - leftover)..(curr_width - 1) {
                padded.push(red(idx));
            }
            padded.push(red(curr_width - 1));
        }
    } else {
        for idx in 0..curr_width {
            padded.push(red(idx));
        }
    }

    // Pad the remaining of the texture with the last LUT entry.
    let missing = (width * height).saturating_sub(padded.len());
    for _ in 0..missing {
        padded.push(red(curr_width - 1));
    }
    padded
}

/// Port of `GetLut1DGPUShaderProgram`.
pub(crate) fn lut1d_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    lut_data: &Lut1DOpData,
) -> Result<()> {
    if shader_creator.language() == GpuLanguage::Osl1 {
        return Err(Error::msg(
            "The Lut1DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ));
    }

    let default_max_width = shader_creator.texture_max_width() as usize;
    if default_max_width < 2 {
        return Err(Error::msg(format!(
            "1D LUT texture maximum width is invalid: {default_max_width}."
        )));
    }

    let length = lut_data.array().length();
    let width = length.min(default_max_width);
    let height = (length / default_max_width) + 1;
    let num_channels = lut_data.array().num_color_components();

    // Note: The 1D LUT needs a GPU texture for the Look-up table
    // implementation. However, the texture type & content may vary based on
    // the number of channels i.e. when all channels are identical a F32 Red
    // GPU texture is enough.
    let single_channel = num_channels == 1;

    // Adjust LUT texture to allow for correct 2d linear interpolation, if
    // needed.
    let values = if single_channel {
        create_padded_red_channel(width, height, lut_data.array().values())
    } else {
        create_padded_lut_channels(width, height, lut_data.array().values())
    };

    // Register the RGB LUT.
    let index = shader_creator.next_resource_index();
    let res_name = format!("{}_lut1d_{}", shader_creator.resource_prefix(), index);
    // Note: Remove potentially problematic double underscores from GLSL
    // resource names.
    let name = replace_all(&res_name, "__", "_");

    let lang = shader_creator.language();
    let dimensions = if height > 1
        || lut_data.is_input_half_domain()
        || lang == GpuLanguage::GlslEs1_0
        || lang == GpuLanguage::GlslEs3_0
        || !shader_creator.allow_texture_1d()
    {
        TextureDimensions::Texture2D
    } else {
        TextureDimensions::Texture1D
    };

    // Copy the LUT into the shader creator as a texture object.
    let texture_shader_binding_index = shader_creator.add_texture(
        &name,
        &GpuShaderText::sampler_name(&name),
        width as u32,
        height as u32,
        if single_channel {
            TextureType::RedChannel
        } else {
            TextureType::RgbChannel
        },
        dimensions,
        lut_data.concrete_interpolation(),
        &values,
    )?;

    // Add the LUT code to the OCIO shader program.

    if dimensions == TextureDimensions::Texture2D {
        // In case the 1D LUT length exceeds the 1D texture maximum length,
        // or the language doesn't support 1D textures, a 2D texture is used.

        // Create a 2D texture declaration.
        {
            let mut ss = GpuShaderText::new(lang);
            ss.declare_tex2d(
                &name,
                shader_creator.descriptor_set_index(),
                texture_shader_binding_index,
            )?;
            shader_creator.add_to_texture_declare_shader_code(ss.as_str());
        }
        // Create the helper function to deal with 2D array lookups.
        {
            let mut ss = GpuShaderText::new(lang);

            nl!(ss, ss.float2_keyword(), " ", name, "_computePos(float f)");
            nl!(ss, "{");
            ss.indent();

            if lut_data.is_input_half_domain() {
                const NEG_MIN_EXP: f32 = 15.0;
                const EXP_SCALE: f32 = 1024.0;
                const INV_DENRM_STEP: f32 = 16777216.0; // 1 / 2^-24

                nl!(ss, "float dep;");
                nl!(ss, "float abs_f = abs(f);");
                nl!(ss, "if (abs_f > ", HALF_NRM_MIN, ")");
                nl!(ss, "{");
                ss.indent();
                ss.declare_float3("fComp", NEG_MIN_EXP, NEG_MIN_EXP, NEG_MIN_EXP)?;
                nl!(ss, "float absarr = min( abs_f, ", HALF_MAX, ");");
                // Compute the exponent, scaled [-14,15].
                nl!(ss, "fComp.x = floor( log2( absarr ) );");
                // Lower is the greatest power of 2 <= f.
                nl!(ss, "float lower = pow( 2.0, fComp.x );");
                // Compute the mantissa (scaled [0-1]).
                nl!(ss, "fComp.y = ( absarr - lower ) / lower;");
                // The dot product recombines the parts into a raw half without
                // the sign component:
                //   dep = [ exponent + mantissa + NEG_MIN_EXP] * scale
                ss.declare_float3("scale", EXP_SCALE, EXP_SCALE, EXP_SCALE)?;
                nl!(ss, "dep = dot( fComp, scale );");
                ss.dedent();
                nl!(ss, "}");
                nl!(ss, "else");
                nl!(ss, "{");
                ss.indent();
                // Extract bits from denormalized values.
                nl!(ss, "dep = abs_f * ", INV_DENRM_STEP, ";");
                ss.dedent();
                nl!(ss, "}");

                // Adjust position for negative values.
                nl!(ss, "dep += (f < 0.) ? 32768.0 : 0.0;");

                // At this point 'dep' contains the raw half.
                // Note: Raw halfs for NaN floats cannot be computed using
                //       floating-point operations.
            } else {
                // Need clamp() to protect against f outside [0,1] causing a
                // bogus x value.
                // clamp( f, 0., 1.) * (dim - 1)
                nl!(
                    ss,
                    "float dep = clamp(f, 0.0, 1.0) * ",
                    (length - 1) as f32,
                    ";"
                );
            }

            nl!(ss, ss.float2_decl("retVal")?, ";");

            if height > 1 {
                // floor( dep / (width-1) ))
                nl!(ss, "retVal.y = floor(dep / ", (width - 1) as f32, ");");
                // dep - retVal.y * (width-1)
                nl!(ss, "retVal.x = dep - retVal.y * ", (width - 1) as f32, ";");

                // (retVal.x + 0.5) / width;
                nl!(ss, "retVal.x = (retVal.x + 0.5) / ", width as f32, ";");
                // (retVal.x + 0.5) / height;
                nl!(ss, "retVal.y = (retVal.y + 0.5) / ", height as f32, ";");
            } else {
                // (dep + 0.5) / width;
                nl!(ss, "retVal.x = (dep + 0.5) / ", width as f32, ";");
                nl!(ss, "retVal.y = 0.5;");
            }

            nl!(ss, "return retVal;");
            ss.dedent();
            nl!(ss, "}");

            shader_creator.add_to_helper_shader_code(ss.as_str());
        }
    } else {
        // Create a 1D texture declaration.
        let mut ss = GpuShaderText::new(lang);
        ss.declare_tex1d(
            &name,
            shader_creator.descriptor_set_index(),
            texture_shader_binding_index,
        )?;
        shader_creator.add_to_texture_declare_shader_code(ss.as_str());
    }

    let pix = shader_creator.pixel_name().to_string();

    let mut ss = GpuShaderText::new(lang);
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add LUT 1D processing for ", name);
    nl!(ss, "");

    nl!(ss, "{");
    ss.indent();

    let hue_dw3 = lut_data.hue_adjust() == Lut1DHueAdjust::Dw3;

    if hue_dw3 {
        nl!(ss, "// Add the pre hue adjustment");
        nl!(
            ss,
            ss.float3_decl("maxval")?,
            " = max(",
            pix,
            ".rgb, max(",
            pix,
            ".gbr, ",
            pix,
            ".brg));"
        );
        nl!(
            ss,
            ss.float3_decl("minval")?,
            " = min(",
            pix,
            ".rgb, min(",
            pix,
            ".gbr, ",
            pix,
            ".brg));"
        );
        nl!(ss, "float oldChroma = max(1e-8, maxval.r - minval.r);");
        nl!(ss, ss.float3_decl("delta")?, " = ", pix, ".rgb - minval;");
        nl!(ss, "");
    }

    let g_suffix = if single_channel { ".r;" } else { ".g;" };
    let b_suffix = if single_channel { ".r;" } else { ".b;" };

    if dimensions == TextureDimensions::Texture2D {
        let s = format!("{name}_computePos({pix}");

        nl!(
            ss,
            pix,
            ".r = ",
            ss.sample_tex2d(&name, &format!("{s}.r)"))?,
            ".r;"
        );
        nl!(
            ss,
            pix,
            ".g = ",
            ss.sample_tex2d(&name, &format!("{s}.g)"))?,
            g_suffix
        );
        nl!(
            ss,
            pix,
            ".b = ",
            ss.sample_tex2d(&name, &format!("{s}.b)"))?,
            b_suffix
        );
    } else {
        let dim = length as f32;

        nl!(
            ss,
            ss.float3_decl(&format!("{name}_coords"))?,
            " = (",
            pix,
            ".rgb * ",
            ss.float3_const1(dim - 1.0),
            " + ",
            ss.float3_const1(0.5f32),
            " ) / ",
            ss.float3_const1(dim),
            ";"
        );

        nl!(
            ss,
            pix,
            ".r = ",
            ss.sample_tex1d(&name, &format!("{name}_coords.r"))?,
            ".r;"
        );
        nl!(
            ss,
            pix,
            ".g = ",
            ss.sample_tex1d(&name, &format!("{name}_coords.g"))?,
            g_suffix
        );
        nl!(
            ss,
            pix,
            ".b = ",
            ss.sample_tex1d(&name, &format!("{name}_coords.b"))?,
            b_suffix
        );
    }

    if hue_dw3 {
        nl!(ss, "");
        nl!(ss, "// Add the post hue adjustment");
        nl!(
            ss,
            ss.float3_decl("maxval2")?,
            " = max(",
            pix,
            ".rgb, max(",
            pix,
            ".gbr, ",
            pix,
            ".brg));"
        );
        nl!(
            ss,
            ss.float3_decl("minval2")?,
            " = min(",
            pix,
            ".rgb, min(",
            pix,
            ".gbr, ",
            pix,
            ".brg));"
        );
        nl!(ss, "float newChroma = maxval2.r - minval2.r;");
        nl!(ss, pix, ".rgb = minval2.r + delta * newChroma / oldChroma;");
    }

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `Lut1DOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &Lut1DOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    let lut_data = op.data();
    if lut_data.direction() == TransformDirection::Inverse {
        // Note: Even if the optim flags specify exact inversion, only fast
        // inversion is supported on the GPU.
        let tmp = make_fast_lut1d_from_inverse(lut_data)?;
        return lut1d_shader_program(shader_creator, &tmp);
    }
    lut1d_shader_program(shader_creator, lut_data)
}
