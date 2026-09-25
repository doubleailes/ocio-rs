//! GPU renderer of the 3D LUT op (port of `Lut3DOpGPU.cpp`).

use crate::error::{Error, Result};
use crate::gpu::shader_text::replace_all;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::lut3d::{make_fast_lut3d_from_inverse, Lut3DOp, Lut3DOpData};
use crate::types::{GpuLanguage, Interpolation, TransformDirection};

/// One tetrahedral case: the two lookup offsets (in BGR order) and the
/// weights of the four corners.
struct Tetra<'a> {
    condition: &'a str,
    offset2: [f32; 3],
    offset3: [f32; 3],
    f1: &'a str,
    f4: &'a str,
    f2: &'a str,
    f3: &'a str,
}

fn add_tetra_case(ss: &mut GpuShaderText, name: &str, pix: &str, t: &Tetra<'_>) -> Result<()> {
    nl!(ss, t.condition);
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        "nextInd = baseInd + ",
        ss.float3_const(t.offset2[0], t.offset2[1], t.offset2[2]),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("v2")?,
        " = ",
        ss.sample_tex3d(name, "nextInd")?,
        ".rgb;"
    );

    nl!(
        ss,
        "nextInd = baseInd + ",
        ss.float3_const(t.offset3[0], t.offset3[1], t.offset3[2]),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("v3")?,
        " = ",
        ss.sample_tex3d(name, "nextInd")?,
        ".rgb;"
    );

    nl!(ss, "f1 = ", ss.float3_const1(t.f1), ";");
    nl!(ss, "f4 = ", ss.float3_const1(t.f4), ";");
    nl!(
        ss,
        ss.float3_decl("f2")?,
        " = ",
        ss.float3_const1(t.f2),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("f3")?,
        " = ",
        ss.float3_const1(t.f3),
        ";"
    );

    nl!(ss, pix, ".rgb = (f2 * v2) + (f3 * v3);");
    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

/// Port of `GetLut3DGPUShaderProgram`.
pub(crate) fn lut3d_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    lut_data: &Lut3DOpData,
) -> Result<()> {
    if shader_creator.language() == GpuLanguage::Osl1 {
        return Err(Error::msg(
            "The Lut3DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ));
    }

    let index = shader_creator.next_resource_index();
    let res_name = format!("{}_lut3d_{}", shader_creator.resource_prefix(), index);
    // Note: Remove potentially problematic double underscores from GLSL
    // resource names.
    let name = replace_all(&res_name, "__", "_");

    let mut sampler_interpolation = lut_data.concrete_interpolation();
    // Enforce GL_NEAREST with shader-generated tetrahedral interpolation.
    if sampler_interpolation == Interpolation::Tetrahedral {
        sampler_interpolation = Interpolation::Nearest;
    }

    let grid_size = lut_data.grid_size();

    // Copy the LUT into the shader creator as a texture object.
    let texture_shader_binding_index = shader_creator.add_3d_texture(
        &name,
        &GpuShaderText::sampler_name(&name),
        grid_size as u32,
        sampler_interpolation,
        lut_data.array().values(),
    )?;

    let lang = shader_creator.language();

    // Create the texture declaration.
    {
        let mut ss = GpuShaderText::new(lang);
        ss.declare_tex3d(
            &name,
            shader_creator.descriptor_set_index(),
            texture_shader_binding_index,
        )?;
        shader_creator.add_to_texture_declare_shader_code(ss.as_str());
    }

    let dim = grid_size as f32;

    // incr = 1/dim (amount needed to increment one index in the grid)
    let incr = 1.0f32 / dim;

    let pix = shader_creator.pixel_name().to_string();

    let mut ss = GpuShaderText::new(lang);
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add LUT 3D processing for ", name);
    nl!(ss, "");

    // Tetrahedral interpolation
    // The strategy is to use texture3d lookups with GL_NEAREST to fetch the 4
    // corners of the cube (v1,v2,v3,v4), compute the 4 barycentric weights
    // (f1,f2,f3,f4), and then perform the interpolation manually. One side
    // benefit of this is that we are not subject to the 8-bit quantization of
    // the fractional weights that happens using GL_LINEAR.
    if lut_data.concrete_interpolation() == Interpolation::Tetrahedral {
        nl!(ss, "{");
        ss.indent();

        nl!(
            ss,
            ss.float3_decl("coords")?,
            " = ",
            pix,
            ".rgb * ",
            ss.float3_const1(dim - 1.0),
            "; "
        );

        // baseInd is on [0,dim-1]
        nl!(ss, ss.float3_decl("baseInd")?, " = floor(coords);");

        // frac is on [0,1]
        nl!(ss, ss.float3_decl("frac")?, " = coords - baseInd;");

        // Scale/offset baseInd onto [0,1] as usual for doing texture lookups
        // we use zyx to flip the order since blue varies most rapidly in the
        // grid array ordering.
        nl!(ss, ss.float3_decl("f1, f4")?, ";");

        nl!(
            ss,
            "baseInd = ( baseInd.zyx + ",
            ss.float3_const1(0.5f32),
            " ) / ",
            ss.float3_const1(dim),
            ";"
        );
        nl!(
            ss,
            ss.float3_decl("v1")?,
            " = ",
            ss.sample_tex3d(&name, "baseInd")?,
            ".rgb;"
        );

        nl!(
            ss,
            ss.float3_decl("nextInd")?,
            " = baseInd + ",
            ss.float3_const1(incr),
            ";"
        );
        nl!(
            ss,
            ss.float3_decl("v4")?,
            " = ",
            ss.sample_tex3d(&name, "nextInd")?,
            ".rgb;"
        );

        nl!(ss, "if (frac.r >= frac.g)");
        nl!(ss, "{");
        ss.indent();
        // Note that compared to the CPU version of the algorithm, we increment
        // in inverted order since baseInd & nextInd are essentially BGR rather
        // than RGB.
        // R > G > B
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "if (frac.g >= frac.b)",
                offset2: [0.0, 0.0, incr],
                offset3: [0.0, incr, incr],
                f1: "1. - frac.r",
                f4: "frac.b",
                f2: "frac.r - frac.g",
                f3: "frac.g - frac.b",
            },
        )?;
        // R > B > G
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "else if (frac.r >= frac.b)",
                offset2: [0.0, 0.0, incr],
                offset3: [incr, 0.0, incr],
                f1: "1. - frac.r",
                f4: "frac.g",
                f2: "frac.r - frac.b",
                f3: "frac.b - frac.g",
            },
        )?;
        // B > R > G
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "else",
                offset2: [incr, 0.0, 0.0],
                offset3: [incr, 0.0, incr],
                f1: "1. - frac.b",
                f4: "frac.g",
                f2: "frac.b - frac.r",
                f3: "frac.r - frac.g",
            },
        )?;
        ss.dedent();
        nl!(ss, "}");
        nl!(ss, "else");
        nl!(ss, "{");
        ss.indent();
        // B > G > R
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "if (frac.g <= frac.b)",
                offset2: [incr, 0.0, 0.0],
                offset3: [incr, incr, 0.0],
                f1: "1. - frac.b",
                f4: "frac.r",
                f2: "frac.b - frac.g",
                f3: "frac.g - frac.r",
            },
        )?;
        // G > R > B
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "else if (frac.r >= frac.b)",
                offset2: [0.0, incr, 0.0],
                offset3: [0.0, incr, incr],
                f1: "1. - frac.g",
                f4: "frac.b",
                f2: "frac.g - frac.r",
                f3: "frac.r - frac.b",
            },
        )?;
        // G > B > R
        add_tetra_case(
            &mut ss,
            &name,
            &pix,
            &Tetra {
                condition: "else",
                offset2: [0.0, incr, 0.0],
                offset3: [incr, incr, 0.0],
                f1: "1. - frac.g",
                f4: "frac.r",
                f2: "frac.g - frac.b",
                f3: "frac.b - frac.r",
            },
        )?;
        ss.dedent();
        nl!(ss, "}");

        nl!(ss, pix, ".rgb = ", pix, ".rgb + (f1 * v1) + (f4 * v4);");

        ss.dedent();
        nl!(ss, "}");
    } else {
        // Trilinear interpolation
        // Use texture3d and GL_LINEAR and the GPU's built-in trilinear
        // algorithm. Note that the fractional components are quantized to
        // 8-bits on some hardware, which introduces significant error with
        // small grid sizes.
        nl!(
            ss,
            ss.float3_decl(&format!("{name}_coords"))?,
            " = (",
            pix,
            ".zyx * ",
            ss.float3_const1(dim - 1.0),
            " + ",
            ss.float3_const1(0.5f32),
            ") / ",
            ss.float3_const1(dim),
            ";"
        );

        nl!(
            ss,
            pix,
            ".rgb = ",
            ss.sample_tex3d(&name, &format!("{name}_coords"))?,
            ".rgb;"
        );
    }

    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `Lut3DOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &Lut3DOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    let lut_data = op.data();
    if lut_data.direction() == TransformDirection::Inverse {
        let tmp = make_fast_lut3d_from_inverse(lut_data)?;
        return lut3d_shader_program(shader_creator, &tmp);
    }
    lut3d_shader_program(shader_creator, lut_data)
}
