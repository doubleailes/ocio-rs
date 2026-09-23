//! Human readable description of transforms, mirroring the C++
//! `operator<<` of every transform (used by the `Display` implementations of
//! the config objects).

use super::utils::{fmt_f32, fmt_f64, format_g};
use crate::transforms::*;
use crate::types::Lut1DHueAdjust;

fn list(v: &[f64]) -> String {
    v.iter().map(|x| fmt_f64(*x)).collect::<Vec<_>>().join(", ")
}

fn b(v: bool) -> u8 {
    u8::from(v)
}

/// Describe a transform like OCIO's `operator<<`.
pub fn format_transform(t: &Transform) -> String {
    match t {
        Transform::Allocation(t) => {
            let mut s = format!("<AllocationTransform direction={}", t.direction.as_str());
            if !t.vars.is_empty() {
                s.push_str(&format!(", allocation={}, vars=", t.allocation.as_str()));
                s.push_str(
                    &t.vars
                        .iter()
                        .map(|v| fmt_f32(*v as f32))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            s.push('>');
            s
        }
        Transform::Builtin(t) => {
            format!(
                "<BuiltinTransform direction = {}, style = {}>",
                t.direction.as_str(),
                t.style
            )
        }
        Transform::Cdl(t) => format!(
            "<CDLTransform direction={}, slope=[{}], offset=[{}], power=[{}], sat={}, style={}>",
            t.direction.as_str(),
            list(&t.slope),
            list(&t.offset),
            list(&t.power),
            fmt_f64(t.sat),
            t.style.as_str()
        ),
        Transform::ColorSpace(t) => {
            let mut s = format!(
                "<ColorSpaceTransform direction={}, src={}, dst={}",
                t.direction.as_str(),
                t.src,
                t.dst
            );
            if !t.data_bypass {
                s.push_str("dataBypass=0");
            }
            s.push('>');
            s
        }
        Transform::DisplayView(t) => {
            let mut s = format!(
                "<DisplayViewTransform direction={}, src={}, display={}, view={}, ",
                t.direction.as_str(),
                t.src,
                t.display,
                t.view
            );
            if t.looks_bypass {
                s.push_str(", looksBypass=1");
            }
            if !t.data_bypass {
                s.push_str(", dataBypass=0");
            }
            s.push('>');
            s
        }
        Transform::Exponent(t) => format!(
            "<ExponentTransform direction={}, value=[{}], style={}>",
            t.direction.as_str(),
            list(&t.value),
            t.negative_style.as_str()
        ),
        Transform::ExponentWithLinear(t) => format!(
            "<ExponentWithLinearTransform direction={}, gamma=[{}], offset=[{}], style={}>",
            t.direction.as_str(),
            list(&t.gamma),
            list(&t.offset),
            t.negative_style.as_str()
        ),
        Transform::ExposureContrast(t) => {
            let mut s = format!(
                "<ExposureContrast direction={}, style={}, exposure={}, contrast={}, gamma={}, pivot={}, \
                 logExposureStep={}, logMidGray={}",
                t.direction.as_str(),
                t.style.as_str(),
                fmt_f64(t.exposure),
                fmt_f64(t.contrast),
                fmt_f64(t.gamma),
                fmt_f64(t.pivot),
                fmt_f64(t.log_exposure_step),
                fmt_f64(t.log_mid_gray)
            );
            if t.exposure_dynamic {
                s.push_str(", exposureDynamic");
            }
            if t.contrast_dynamic {
                s.push_str(", contrastDynamic");
            }
            if t.gamma_dynamic {
                s.push_str(", gammaDynamic");
            }
            s.push('>');
            s
        }
        Transform::File(t) => {
            let mut s = format!(
                "<FileTransform direction={}, interpolation={}, src={}",
                t.direction.as_str(),
                t.interpolation.as_str(),
                t.src
            );
            if !t.ccc_id.is_empty() {
                s.push_str(&format!(", cccid={}", t.ccc_id));
            }
            if t.cdl_style != crate::types::CdlStyle::NoClamp {
                s.push_str(&format!(", cdl_style={}", t.cdl_style.as_str()));
            }
            s.push('>');
            s
        }
        Transform::FixedFunction(t) => {
            let mut s = format!(
                "<FixedFunction direction={}, style={}",
                t.direction.as_str(),
                t.style.as_str()
            );
            if !t.params.is_empty() {
                s.push_str(&format!(", params=[{}]", list(&t.params)));
            }
            s.push('>');
            s
        }
        Transform::GradingHueCurve(t) => {
            let mut s = format!(
                "<GradingHueCurveTransform direction={}, style={}, values={:?}",
                t.direction.as_str(),
                t.style.as_str(),
                t.value
            );
            if t.rgb_to_hsy == crate::types::HsyTransformStyle::None {
                s.push_str(", hsy_transform=none");
            }
            if t.dynamic {
                s.push_str(", dynamic");
            }
            s.push('>');
            s
        }
        Transform::GradingPrimary(t) => {
            let mut s = format!(
                "<GradingPrimaryTransform direction={}, style={}, values={:?}",
                t.direction.as_str(),
                t.style.as_str(),
                t.value
            );
            if t.dynamic {
                s.push_str(", dynamic");
            }
            s.push('>');
            s
        }
        Transform::GradingRgbCurve(t) => {
            let mut s = format!(
                "<GradingRGBCurveTransform direction={}, style={}, values={:?}",
                t.direction.as_str(),
                t.style.as_str(),
                t.value
            );
            if t.bypass_lin_to_log {
                s.push_str(", bypass_lintolog");
            }
            if t.dynamic {
                s.push_str(", dynamic");
            }
            s.push('>');
            s
        }
        Transform::GradingTone(t) => {
            let mut s = format!(
                "<GradingToneTransform direction={}, style={}, values={:?}",
                t.direction.as_str(),
                t.style.as_str(),
                t.value
            );
            if t.dynamic {
                s.push_str(", dynamic");
            }
            s.push('>');
            s
        }
        Transform::Group(g) => {
            let mut s = format!(
                "<GroupTransform direction={}, transforms=",
                g.direction.as_str()
            );
            for c in &g.transforms {
                s.push_str("\n        ");
                s.push_str(&format_transform(c));
            }
            s.push('>');
            s
        }
        Transform::LogAffine(t) => format!(
            "<LogAffineTransform direction={}, base={}, logSideSlope=[{}], logSideOffset=[{}], \
             linSideSlope=[{}], linSideOffset=[{}]>",
            t.direction.as_str(),
            fmt_f64(t.base),
            list(&t.log_side_slope),
            list(&t.log_side_offset),
            list(&t.lin_side_slope),
            list(&t.lin_side_offset)
        ),
        Transform::LogCamera(t) => {
            let mut s = format!(
                "<LogCameraTransform direction={}, base={}, logSideSlope=[{}], logSideOffset=[{}], \
                 linSideSlope=[{}], linSideOffset=[{}], linSideBreak=[{}]",
                t.direction.as_str(),
                fmt_f64(t.base),
                list(&t.log_side_slope),
                list(&t.log_side_offset),
                list(&t.lin_side_slope),
                list(&t.lin_side_offset),
                list(&t.lin_side_break)
            );
            if let Some(ls) = &t.linear_slope {
                s.push_str(&format!(", linearSlope=[{}]", list(ls)));
            }
            s.push('>');
            s
        }
        Transform::Log(t) => format!(
            "<LogTransform direction={}, base={}>",
            t.direction.as_str(),
            fmt_f64(t.base)
        ),
        Transform::Look(t) => {
            let mut s = format!(
                "<LookTransform direction={}, src={}, dst={}, looks={}",
                t.direction.as_str(),
                t.src,
                t.dst,
                t.looks
            );
            if t.skip_color_space_conversion {
                s.push_str(", skipCSConversion");
            }
            s.push('>');
            s
        }
        Transform::Lut1D(t) => {
            let hue = match t.hue_adjust {
                Lut1DHueAdjust::None => 0,
                Lut1DHueAdjust::Dw3 => 1,
                Lut1DHueAdjust::Wypn => 2,
            };
            let l = t.length();
            let mut s = format!(
                "<Lut1DTransform direction={}, fileoutdepth={}, interpolation={}, inputhalf={}, \
                 outputrawhalf={}, hueadjust={}, length={}, ",
                t.direction.as_str(),
                t.file_output_bit_depth.as_str(),
                t.interpolation.as_str(),
                b(t.input_half_domain),
                b(t.output_raw_halfs),
                hue,
                l
            );
            if l > 0 {
                s.push_str(&min_max(&t.values));
            }
            s.push('>');
            s
        }
        Transform::Lut3D(t) => {
            let mut s = format!(
                "<Lut3DTransform direction={}, fileoutdepth={}, interpolation={}, gridSize={}, ",
                t.direction.as_str(),
                t.file_output_bit_depth.as_str(),
                t.interpolation.as_str(),
                t.grid_size
            );
            if t.grid_size > 0 {
                s.push_str(&min_max(&t.values));
            }
            s.push('>');
            s
        }
        Transform::Matrix(t) => {
            let m = t
                .matrix
                .iter()
                .map(|x| format_g(*x, 16))
                .collect::<Vec<_>>()
                .join(", ");
            let o = t
                .offset
                .iter()
                .map(|x| format_g(*x, 16))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "<MatrixTransform direction={}, fileindepth={}, fileoutdepth={}, matrix=[{}], offset=[{}]>",
                t.direction.as_str(),
                t.file_input_bit_depth.as_str(),
                t.file_output_bit_depth.as_str(),
                m,
                o
            )
        }
        Transform::Range(t) => {
            let mut s = format!(
                "<RangeTransform direction={}, fileindepth={}, fileoutdepth={}",
                t.direction.as_str(),
                t.file_input_bit_depth.as_str(),
                t.file_output_bit_depth.as_str()
            );
            if t.style != crate::types::RangeStyle::Clamp {
                s.push_str(&format!(", style={}", t.style.as_str()));
            }
            if let Some(v) = t.min_in {
                s.push_str(&format!(", minInValue={}", fmt_f64(v)));
            }
            if let Some(v) = t.max_in {
                s.push_str(&format!(", maxInValue={}", fmt_f64(v)));
            }
            if let Some(v) = t.min_out {
                s.push_str(&format!(", minOutValue={}", fmt_f64(v)));
            }
            if let Some(v) = t.max_out {
                s.push_str(&format!(", maxOutValue={}", fmt_f64(v)));
            }
            s.push('>');
            s
        }
    }
}

fn min_max(values: &[f32]) -> String {
    let mut mn = [f32::MAX; 3];
    let mut mx = [-f32::MAX; 3];
    for px in values.chunks_exact(3) {
        for c in 0..3 {
            mn[c] = mn[c].min(px[c]);
            mx[c] = mx[c].max(px[c]);
        }
    }
    format!(
        "minrgb=[{}, {}, {}], maxrgb=[{}, {}, {}]",
        fmt_f32(mn[0]),
        fmt_f32(mn[1]),
        fmt_f32(mn[2]),
        fmt_f32(mx[0]),
        fmt_f32(mx[1]),
        fmt_f32(mx[2])
    )
}
