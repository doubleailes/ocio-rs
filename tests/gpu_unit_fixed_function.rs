//! Port of the OpenColorIO GPU unit tests `tests/gpu/FixedFunctionOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).
//!
//! Where upstream picks a threshold with `OCIO_USE_SSE2`, the SSE2 value is
//! used: the ocio-rs CPU renderers reproduce OCIO's SSE2 builds (see
//! `PORTING.md`).

// The upstream float literals are kept verbatim (they round to the same f32).
#![allow(clippy::excessive_precision)]

mod wgsl_common;

use ocio::gpu::GpuShaderCreator;
use ocio::{
    BuiltinTransform, FixedFunctionStyle, FixedFunctionTransform, GroupTransform, Transform,
    TransformDirection,
};
use wgsl_common::gpu_unit::gpu_test;

/// `FixedFunctionTransform::Create(style, params)` with `setDirection(direction)`.
fn fixed_function(
    style: FixedFunctionStyle,
    params: &[f64],
    direction: TransformDirection,
) -> Transform {
    let mut func = FixedFunctionTransform::new(style, params);
    func.direction = direction;
    Transform::FixedFunction(func)
}

#[rustfmt::skip]
const RED_MOD_VALUES: [f32; 16] = [
    0.90, 0.05, 0.22, 0.50, //
    0.97, 0.097, 0.0097, 1.0, //
    0.89, 0.15, 0.56, 0.0, //
    -1.0, -0.001, 1.2, 0.0,
];

#[rustfmt::skip]
const GLOW_VALUES: [f32; 12] = [
    0.11, 0.02, 0.0, 0.5, // YC = 0.10
    0.01, 0.02, 0.03, 1.0, // YC = 0.03
    0.11, 0.91, 0.01, 0.0, // YC = 0.84
];

#[test]
fn style_aces_redmod03_fwd() {
    gpu_test("FixedFunction/style_aces_redmod03_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRedMod03,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(RED_MOD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces_redmod03_inv() {
    gpu_test("FixedFunction/style_aces_redmod03_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRedMod03,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(RED_MOD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces_redmod10_fwd() {
    gpu_test("FixedFunction/style_aces_redmod10_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRedMod10,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(RED_MOD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces_redmod10_inv() {
    gpu_test("FixedFunction/style_aces_redmod10_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRedMod10,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(RED_MOD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces_glow03_fwd() {
    gpu_test("FixedFunction/style_aces_glow03_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGlow03,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(GLOW_VALUES.to_vec());
        test.set_error_threshold(1e-7);
    });
}

#[test]
fn style_aces_glow03_inv() {
    gpu_test("FixedFunction/style_aces_glow03_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGlow03,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(GLOW_VALUES.to_vec());
        test.set_error_threshold(1e-7);
    });
}

#[test]
fn style_aces_glow10_fwd() {
    gpu_test("FixedFunction/style_aces_glow10_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGlow10,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(GLOW_VALUES.to_vec());
        test.set_error_threshold(1e-7);
    });
}

#[test]
fn style_aces_glow10_inv() {
    gpu_test("FixedFunction/style_aces_glow10_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGlow10,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(GLOW_VALUES.to_vec());
        test.set_error_threshold(1e-7);
    });
}

const GAMUT_COMP13_PARAMS: [f64; 7] = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

// See FixedFunctionOpCPU_tests.cpp for informations on the dataset
#[rustfmt::skip]
const GAMUT_COMP13_VALUES: [f32; 156] = [
    // ALEXA Wide Gamut
    0.96663409472, 0.04819045216, 0.00719300006, 0.0, //
    0.11554181576, 1.18493819237, -0.06659350544, 0.0, //
    -0.08217582852, -0.23312863708, 1.05940067768, 0.0, //
    // BMD Wide Gamut
    0.92980366945, 0.03025679290, -0.02240031771, 0.0, //
    0.12437260151, 1.19238424301, -0.08014731854, 0.0, //
    -0.05417707562, -0.22264070809, 1.10254764557, 0.0, //
    // Cinema Gamut
    1.10869872570, -0.05317572504, -0.00306261564, 0.0, //
    0.00142395718, 1.31239914894, -0.22332298756, 0.0, //
    -0.11012268066, -0.25922337174, 1.22638559341, 0.0, //
    // REDWideGamutRGB
    1.14983725548, -0.02548932098, -0.06720325351, 0.0, //
    -0.06796986610, 1.30455482006, -0.31973674893, 0.0, //
    -0.08186896890, -0.27906489372, 1.38694024086, 0.0, //
    // S-Gamut3
    1.08979821205, -0.03117186762, -0.00326358480, 0.0, //
    -0.03276504576, 1.18293666840, -0.00156985107, 0.0, //
    -0.05703317001, -0.15176482499, 1.00483345985, 0.0, //
    // Venice S-Gamut3
    1.15183949471, -0.04052511975, -0.01231821068, 0.0, //
    -0.11769985408, 1.20661473274, 0.00725125661, 0.0, //
    -0.03413961083, -0.16608965397, 1.00506699085, 0.0, //
    // V-Gamut
    1.04839742184, -0.02998665348, -0.00313943392, 0.0, //
    0.01196120959, 1.14840388298, -0.00963746291, 0.0, //
    -0.06036021933, -0.11841656268, 1.01277709007, 0.0, //
    // CC24 hue selective patch
    0.13911968470, 0.08746965975, 0.05927771702, 0.0, //
    0.45410454273, 0.32112336159, 0.23821924627, 0.0, //
    0.15262818336, 0.19457373023, 0.31270095706, 0.0, //
    0.11231111735, 0.14410330355, 0.06487321854, 0.0, //
    0.24113640189, 0.22817260027, 0.40912008286, 0.0, //
    0.27200737596, 0.47832396626, 0.40502992272, 0.0, //
    0.49412208796, 0.23219805956, 0.05947655812, 0.0, //
    0.09734666348, 0.10917002708, 0.33662334085, 0.0, //
    0.37841814756, 0.12591768801, 0.12897071242, 0.0, //
    0.09104857594, 0.05404697359, 0.13533248007, 0.0, //
    0.38014721870, 0.47619381547, 0.10615456849, 0.0, //
    0.60210841894, 0.38621774316, 0.08225912601, 0.0, //
    0.05051656812, 0.05367648974, 0.27239432931, 0.0, //
    0.14276765287, 0.28139206767, 0.09023084491, 0.0, //
    0.28782477975, 0.06140174344, 0.05256444961, 0.0, //
    0.70791155100, 0.58026152849, 0.09300658852, 0.0, //
    0.35456034541, 0.12329842150, 0.27530980110, 0.0, //
    0.08374430984, 0.22774916887, 0.35839819908, 0.0,
];

#[test]
fn style_aces_gamutcomp13_fwd() {
    gpu_test("FixedFunction/style_aces_gamutcomp13_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGamutComp13,
            &GAMUT_COMP13_PARAMS,
            TransformDirection::Forward,
        ));
        test.set_custom_values(GAMUT_COMP13_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces_gamutcomp13_inv() {
    gpu_test("FixedFunction/style_aces_gamutcomp13_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGamutComp13,
            &GAMUT_COMP13_PARAMS,
            TransformDirection::Inverse,
        ));
        test.set_custom_values(GAMUT_COMP13_VALUES.to_vec());

        // 3e-5 accommodates GPU pow() precision at large output values (~3.08) on DirectX.
        test.set_error_threshold(3e-5);
    });
}

#[rustfmt::skip]
const P3D65_1000NIT_PARAMS: [f64; 9] = [
    // Peak luminance
    1000.0, //
    // P3D65 gamut
    0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290,
];

#[test]
fn style_aces2_output_transform_fwd() {
    gpu_test("FixedFunction/style_aces2_output_transform_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesOutputTransform20,
            &P3D65_1000NIT_PARAMS,
            TransformDirection::Forward,
        ));

        #[rustfmt::skip]
        let values = vec![
            // ACEScg primaries and secondaries scaled by 4
            2.781808965, 0.179178253, -0.022103530, 1.0, //
            3.344523751, 3.617862727, -0.006002689, 1.0, //
            0.562714786, 3.438684474, 0.016100841, 1.0, //
            1.218191035, 3.820821747, 4.022103530, 1.0, //
            0.655476249, 0.382137273, 4.006002689, 1.0, //
            3.437285214, 0.561315526, 3.983899159, 1.0, //
            // OCIO test values
            0.110000000, 0.020000000, 0.040000000, 0.5, //
            0.710000000, 0.510000000, 0.810000000, 1.0, //
            0.430000000, 0.820000000, 0.710000000, 0.0, //
            // ColorChecker24 (SMPTE 2065-1 2021)
            0.118770000, 0.087090000, 0.058950000, 1.0, //
            0.400020000, 0.319160000, 0.237360000, 1.0, //
            0.184760000, 0.203980000, 0.313110000, 1.0, //
            0.109010000, 0.135110000, 0.064930000, 1.0, //
            0.266840000, 0.246040000, 0.409320000, 1.0, //
            0.322830000, 0.462080000, 0.406060000, 1.0, //
            0.386050000, 0.227430000, 0.057770000, 1.0, //
            0.138220000, 0.130370000, 0.337030000, 1.0, //
            0.302020000, 0.137520000, 0.127580000, 1.0, //
            0.093100000, 0.063470000, 0.135250000, 1.0, //
            0.348760000, 0.436540000, 0.106130000, 1.0, //
            0.486550000, 0.366850000, 0.080610000, 1.0, //
            0.087320000, 0.074430000, 0.272740000, 1.0, //
            0.153660000, 0.256920000, 0.090710000, 1.0, //
            0.217420000, 0.070700000, 0.051300000, 1.0, //
            0.589190000, 0.539430000, 0.091570000, 1.0, //
            0.309040000, 0.148180000, 0.274260000, 1.0, //
            0.149010000, 0.233780000, 0.359390000, 1.0, //
            0.866530000, 0.867920000, 0.858180000, 1.0, //
            0.573560000, 0.572560000, 0.571690000, 1.0, //
            0.353460000, 0.353370000, 0.353910000, 1.0, //
            0.202530000, 0.202430000, 0.202870000, 1.0, //
            0.094670000, 0.095200000, 0.096370000, 1.0, //
            0.037450000, 0.037660000, 0.038950000, 1.0, //
            // Spectrally non-selective 18 % reflecting diffuser
            0.180000000, 0.180000000, 0.180000000, 1.0, //
            // Perfect reflecting diffuser
            0.977840000, 0.977840000, 0.977840000, 1.0,
        ];
        test.set_custom_values(values);

        test.set_error_threshold(2e-5);
    });
}

#[test]
fn style_aces2_output_transform_inv() {
    gpu_test("FixedFunction/style_aces2_output_transform_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesOutputTransform20,
            &P3D65_1000NIT_PARAMS,
            TransformDirection::Inverse,
        ));

        #[rustfmt::skip]
        let values = vec![
            // ACEScg primaries and secondaries scaled by 4
            4.965774059, -0.032864563, 0.041625995, 1.0, //
            3.969441891, 3.825784922, -0.056133576, 1.0, //
            -0.075329021, 3.688980103, 0.270296901, 1.0, //
            -0.095423937, 3.650517225, 3.459972620, 1.0, //
            -0.028930068, 0.196428135, 2.796343565, 1.0, //
            4.900805950, -0.064376131, 3.838256121, 1.0, //
            // OCIO test values
            0.096890204, -0.001135312, 0.018971510, 0.5, //
            0.809614301, 0.479856580, 0.814239502, 1.0, //
            0.107420206, 0.920529068, 0.726378500, 0.0, //
            // ColorChecker24 (SMPTE 2065-1 2021)
            0.115475260, 0.050812904, 0.030212952, 1.0, //
            0.484879673, 0.301042974, 0.226768956, 1.0, //
            0.098463766, 0.160814658, 0.277010560, 1.0, //
            0.071130365, 0.107334383, 0.035097566, 1.0, //
            0.207111493, 0.198474824, 0.375326216, 1.0, //
            0.195447892, 0.481111974, 0.393299013, 1.0, //
            0.571913838, 0.196872935, 0.041634772, 1.0, //
            0.045791931, 0.069875360, 0.291233480, 1.0, //
            0.424848706, 0.083199009, 0.102153838, 1.0, //
            0.059589427, 0.022219172, 0.091246888, 1.0, //
            0.360365510, 0.478741467, 0.086726837, 1.0, //
            0.695662081, 0.371994525, 0.068298168, 1.0, //
            0.011806309, 0.021665439, 0.199594811, 1.0, //
            0.076526314, 0.256237417, 0.060564656, 1.0, //
            0.300064564, 0.023416257, 0.030360471, 1.0, //
            0.805484772, 0.596903503, 0.082996152, 1.0, //
            0.388385952, 0.079899102, 0.245819211, 1.0, //
            0.010952532, 0.196105912, 0.307181358, 1.0, //
            0.921019495, 0.921707213, 0.912856042, 1.0, //
            0.590192318, 0.588423848, 0.587825358, 1.0, //
            0.337743521, 0.337685764, 0.338155121, 1.0, //
            0.169265985, 0.169178501, 0.169557109, 1.0, //
            0.058346048, 0.059387825, 0.060296260, 1.0, //
            0.012581184, 0.012947139, 0.013654195, 1.0, //
            // Spectrally non-selective 18 % reflecting diffuser
            0.145115465, 0.145115525, 0.145115510, 1.0, //
            // Perfect reflecting diffuser
            1.041565657, 1.041566014, 1.041565657, 1.0,
        ];
        test.set_custom_values(values);

        test.set_error_threshold(1e-4);
    });
}

#[test]
fn style_aces2_output_transform_invfwd() {
    gpu_test(
        "FixedFunction/style_aces2_output_transform_invfwd",
        |test| {
            // Test that there are no shader resource (textures, functions, etc) conflicts between
            // the 2 different inverse and forward transforms.  This is not a round-trip test,
            // it tests that the forward and inverse may exist in the same shader with no issue.

            let data_inv: [f64; 9] = [
                // Peak luminance
                100.0, //
                // REC709 gamut
                0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329,
            ];
            let func_inv = fixed_function(
                FixedFunctionStyle::AcesOutputTransform20,
                &data_inv,
                TransformDirection::Inverse,
            );

            let func_fwd = fixed_function(
                FixedFunctionStyle::AcesOutputTransform20,
                &P3D65_1000NIT_PARAMS,
                TransformDirection::Forward,
            );

            let grp = GroupTransform::from_transforms(vec![func_inv, func_fwd]);

            // TODO: Partially running the output transform without the clamp so do not test Nan
            // or Inf values
            test.set_test_nan(false);
            test.set_test_infinity(false);

            test.set_processor_transform(Transform::Group(grp));

            // This is not expected to be an identity, but it should be the same between CPU & GPU.
            test.set_error_threshold(7e-4);
        },
    );
}

/// Port of upstream's `BuildDisplayViewTransform`.
fn build_display_view_transform(
    display_style: &str,
    view_style: &str,
    do_round_trip: bool,
) -> Transform {
    // Built-in transform for the display.
    let display_builtin = Transform::Builtin(BuiltinTransform::new(display_style));
    display_builtin.validate().unwrap();
    let display_builtin_inv = display_builtin.inverted();

    // Built-in transform for the view.
    let view_builtin = Transform::Builtin(BuiltinTransform::new(view_style));
    view_builtin.validate().unwrap();
    let view_builtin_inv = view_builtin.inverted();

    // Assemble inverse and forward transform into a group transform that goes from
    // display code values to ACES2065-1 and (optionally) back to display code values.
    let mut transforms = vec![display_builtin_inv, view_builtin_inv];
    if do_round_trip {
        transforms.push(view_builtin);
        transforms.push(display_builtin);
    }
    Transform::Group(GroupTransform::from_transforms(transforms))
}

/// Port of upstream's `GenerateIdentityLut3D`.
fn generate_identity_lut3d(edge_len: usize, scale: f32) -> Vec<f32> {
    const NUM_CHANNELS: usize = 4;
    let num_samples = edge_len * edge_len * edge_len;
    let mut img = vec![0.0f32; num_samples * NUM_CHANNELS];

    let c = 1.0f32 / (edge_len as f32 - 1.0);
    for i in 0..num_samples {
        img[NUM_CHANNELS * i] = scale * (i % edge_len) as f32 * c;
        img[NUM_CHANNELS * i + 1] = scale * ((i / edge_len) % edge_len) as f32 * c;
        img[NUM_CHANNELS * i + 2] = scale * ((i / edge_len / edge_len) % edge_len) as f32 * c;
    }
    img
}

// NOTE: Some of the following tests compare the round-trip from display code value to ACES2065-1
// and back to display code value. The round-trip is not perfect (see BuiltinTransform_tests.cpp)
// but the tests here simply check if the CPU and GPU are giving the same result.

#[test]
fn style_aces2_rec709_rndtrip() {
    gpu_test("FixedFunction/style_aces2_rec709_rndtrip", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0";
        let do_roundtrip = true;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);

        // The test harness gets a processor from the transform with the default optimization
        // level. However, the forward/inverse does not optimize out due to the clamp to AP1
        // in-between the FixedFunctions.
        test.set_processor_transform(group);

        // Set up a grid of RGBA custom values.
        let lut_size = 17;
        test.set_custom_values(generate_identity_lut3d(lut_size, 1.0));

        test.set_error_threshold(0.004);
    });
}

#[test]
fn style_aces2_rec709_inv() {
    gpu_test("FixedFunction/style_aces2_rec709_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        test.set_custom_values(generate_identity_lut3d(lut_size, 1.0));

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_displayp3_rndtrip() {
    gpu_test("FixedFunction/style_aces2_displayp3_rndtrip", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_DisplayP3";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0";
        let do_roundtrip = true;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 1.0f32;
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_displayp3_inv() {
    gpu_test("FixedFunction/style_aces2_displayp3_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_DisplayP3";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 1.0f32;
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_1000nit_p3_rndtrip() {
    gpu_test("FixedFunction/style_aces2_1000nit_p3_rndtrip", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0";
        let do_roundtrip = true;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.75183f32; // scale to 1000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        // TODO: Investigate why this is not closer.
        // Setting the CPUProcessor to OPTIMIZATION_NONE helps slightly, but is not the main
        // cause of the error. 0.014 accommodates DirectX floating point differences.
        test.set_error_threshold(0.014);
    });
}

#[test]
fn style_aces2_1000nit_p3_inv() {
    gpu_test("FixedFunction/style_aces2_1000nit_p3_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.75183f32; // scale to 1000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_4000nit_p3_rndtrip() {
    gpu_test("FixedFunction/style_aces2_4000nit_p3_rndtrip", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0";
        let do_roundtrip = true;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.90257f32; // scale to 4000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        // TODO: Investigate why this is not closer.
        test.set_error_threshold(0.019);
    });
}

#[test]
fn style_aces2_4000nit_p3_inv() {
    gpu_test("FixedFunction/style_aces2_4000nit_p3_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.90257f32; // scale to 4000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_4000nit_rec2020_rndtrip() {
    gpu_test(
        "FixedFunction/style_aces2_4000nit_rec2020_rndtrip",
        |test| {
            let display_style = "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ";
            let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0";
            let do_roundtrip = true;
            let group = build_display_view_transform(display_style, view_style, do_roundtrip);
            test.set_processor_transform(group);

            let lut_size = 17;
            let lum_scale = 0.90257f32; // scale to 4000 nits
            test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

            // TODO: Investigate why this is not closer. 0.034 accommodates DirectX floating point
            // differences.
            test.set_error_threshold(0.034);
        },
    );
}

#[test]
fn style_aces2_4000nit_rec2020_inv() {
    gpu_test("FixedFunction/style_aces2_4000nit_rec2020_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ";
        let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.90257f32; // scale to 4000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.001);
    });
}

#[test]
fn style_aces2_d60_4000nit_p3_rndtrip() {
    gpu_test("FixedFunction/style_aces2_d60_4000nit_p3_rndtrip", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style =
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0";
        let do_roundtrip = true;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.90257f32; // scale to 4000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        // TODO: Investigate why this is not closer.
        test.set_error_threshold(0.03);
    });
}

#[test]
fn style_aces2_d60_4000nit_p3_inv() {
    gpu_test("FixedFunction/style_aces2_d60_4000nit_p3_inv", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style =
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        let lut_size = 17;
        let lum_scale = 0.90257f32; // scale to 4000 nits
        test.set_custom_values(generate_identity_lut3d(lut_size, lum_scale));

        // Difference is on equal RGB, above about 3600, peaking around 3684, and stopping at 3696
        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.005);
    });
}

#[test]
fn style_aces2_nan_bug() {
    gpu_test("FixedFunction/style_aces2_nan_bug", |test| {
        let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
        let view_style =
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0";
        let do_roundtrip = false;
        let group = build_display_view_transform(display_style, view_style, do_roundtrip);
        test.set_processor_transform(group);

        #[rustfmt::skip]
        let values = vec![
            0.89942779, 0.89942779, 0.89942779, 1.0,
            // This second value became NaN on the GPU before the Aab_to_RGB fix.
            // FIXME: The GPU is no longer NaN, but it is still hugely different from the CPU.
            // 0.89944305, 0.89944305, 0.89944305, 1.0
        ];
        test.set_custom_values(values);

        test.set_relative_comparison(true);
        test.set_expected_minimal_value(1.0);
        test.set_error_threshold(0.01);
    });
}

// Shared test data for the FIXED_FUNCTION_ACES_RGB_TO_JMH_20 / FIXED_FUNCTION_ACES_RGB_TO_HMJ_20
// GPU tests below (ACEScg primaries and secondaries scaled by 4, some OCIO test values, and
// ColorChecker24 SMPTE 2065-1 2021 values).
#[rustfmt::skip]
const ACES2_RGB_VALUES: [f32; 108] = [
    2.781808965, 0.179178253, -0.022103530, 1.0, //
    3.344523751, 3.617862727, -0.006002689, 1.0, //
    0.562714786, 3.438684474, 0.016100841, 1.0, //
    1.218191035, 3.820821747, 4.022103530, 1.0, //
    0.655476249, 0.382137273, 4.006002689, 1.0, //
    3.437285214, 0.561315526, 3.983899159, 1.0, //
    0.110000000, 0.020000000, 0.040000000, 0.5, //
    0.710000000, 0.510000000, 0.810000000, 1.0, //
    0.430000000, 0.820000000, 0.710000000, 0.0, //
    0.118770000, 0.087090000, 0.058950000, 1.0, //
    0.400020000, 0.319160000, 0.237360000, 1.0, //
    0.184760000, 0.203980000, 0.313110000, 1.0, //
    0.109010000, 0.135110000, 0.064930000, 1.0, //
    0.266840000, 0.246040000, 0.409320000, 1.0, //
    0.322830000, 0.462080000, 0.406060000, 1.0, //
    0.386050000, 0.227430000, 0.057770000, 1.0, //
    0.138220000, 0.130370000, 0.337030000, 1.0, //
    0.302020000, 0.137520000, 0.127580000, 1.0, //
    0.093100000, 0.063470000, 0.135250000, 1.0, //
    0.348760000, 0.436540000, 0.106130000, 1.0, //
    0.486550000, 0.366850000, 0.080610000, 1.0, //
    0.087320000, 0.074430000, 0.272740000, 1.0, //
    0.153660000, 0.256920000, 0.090710000, 1.0, //
    0.217420000, 0.070700000, 0.051300000, 1.0, //
    0.589190000, 0.539430000, 0.091570000, 1.0, //
    0.309040000, 0.148180000, 0.274260000, 1.0, //
    0.149010000, 0.233780000, 0.359390000, 1.0,
];

// The JMh values corresponding to ACES2_RGB_VALUES above, run through RGB_TO_JMH_20.
#[rustfmt::skip]
const ACES2_JMH_VALUES: [f32; 108] = [
    107.480636597, 206.827301025, 25.025110245, 1.0, //
    173.194076538, 133.330886841, 106.183448792, 1.0, //
    139.210220337, 191.922363281, 147.056488037, 1.0, //
    157.905166626, 111.975311279, 192.204727173, 1.0, //
    79.229278564, 100.424659729, 268.442108154, 1.0, //
    132.888137817, 173.358779907, 341.715240479, 1.0, //
    26.112514496, 42.523605347, 4.173158169, 0.5, //
    79.190460205, 25.002300262, 332.159759521, 1.0, //
    81.912559509, 39.754810333, 182.925750732, 0.0, //
    33.924663544, 12.254567146, 38.146659851, 1.0, //
    61.332393646, 15.169423103, 39.841842651, 1.0, //
    47.191543579, 11.839941978, 249.107116699, 1.0, //
    37.328300476, 13.224150658, 128.878036499, 1.0, //
    53.465549469, 13.121579170, 285.658966064, 1.0, //
    65.414512634, 19.172147751, 179.324264526, 1.0, //
    55.711513519, 37.182041168, 50.924011230, 1.0, //
    40.020961761, 20.762512207, 271.008331299, 1.0, //
    47.704769135, 35.791145325, 13.975610733, 1.0, //
    30.385913849, 14.544739723, 317.544281006, 1.0, //
    64.222846985, 33.487697601, 119.145133972, 1.0, //
    65.570358276, 35.864013672, 70.842193604, 1.0, //
    31.800464630, 23.920211792, 273.228973389, 1.0, //
    47.950405121, 28.027387619, 144.154159546, 1.0, //
    38.440967560, 42.604164124, 17.892261505, 1.0, //
    75.117736816, 40.952045441, 90.752044678, 1.0, //
    49.311210632, 33.812240601, 348.832092285, 1.0, //
    47.441757202, 22.915655136, 218.454376221, 1.0,
];

// Reorders/scales a JMh values vector into the HMJ layout used by FIXED_FUNCTION_ACES_RGB_TO_HMJ_20:
// [ h/360, M/200, J/100 ].
fn jmh_to_hmj(jmh: &[f32]) -> Vec<f32> {
    jmh.chunks_exact(4)
        .flat_map(|p| {
            let (j, m, h) = (p[0], p[1], p[2]);
            [h / 360.0, m / 200.0, j / 100.0, p[3]]
        })
        .collect()
}

// ACES AP0
const AP0_PARAMS: [f64; 8] = [
    0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
];

#[test]
fn style_aces2_rgb_to_jmh_fwd() {
    gpu_test("FixedFunction/style_aces2_rgb_to_jmh_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRgbToJmh20,
            &AP0_PARAMS,
            TransformDirection::Forward,
        ));

        test.set_custom_values(ACES2_RGB_VALUES.to_vec());

        // Large because JMh is scaled in the hundreds.
        test.set_error_threshold(2e-4);
    });
}

#[test]
fn style_aces2_rgb_to_jmh_inv() {
    gpu_test("FixedFunction/style_aces2_rgb_to_jmh_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRgbToJmh20,
            &AP0_PARAMS,
            TransformDirection::Inverse,
        ));

        test.set_custom_values(ACES2_JMH_VALUES.to_vec());

        test.set_error_threshold(2e-5);
    });
}

#[test]
fn style_aces2_rgb_to_hmj_fwd() {
    gpu_test("FixedFunction/style_aces2_rgb_to_hmj_fwd", |test| {
        // Same RGB values as style_aces2_rgb_to_jmh_fwd above -- FIXED_FUNCTION_ACES_RGB_TO_HMJ_20
        // computes the same JMh internally, just repacked as [ h/360, M/200, J/100 ].

        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRgbToHmj20,
            &AP0_PARAMS,
            TransformDirection::Forward,
        ));

        test.set_custom_values(ACES2_RGB_VALUES.to_vec());

        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_aces2_rgb_to_hmj_inv() {
    gpu_test("FixedFunction/style_aces2_rgb_to_hmj_inv", |test| {
        // Same JMh values as style_aces2_rgb_to_jmh_inv above, reordered/scaled into HMJ.

        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesRgbToHmj20,
            &AP0_PARAMS,
            TransformDirection::Inverse,
        ));

        test.set_custom_values(jmh_to_hmj(&ACES2_JMH_VALUES));

        test.set_error_threshold(2e-5);
    });
}

#[test]
fn style_aces2_tonescale_compress_fwd() {
    gpu_test("FixedFunction/style_aces2_tonescale_compress_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesTonescaleCompress20,
            &[1000.0],
            TransformDirection::Forward,
        ));

        #[rustfmt::skip]
        let values = vec![
            // ACEScg primaries and secondaries scaled by 4
            107.480636597, 206.827301025, 25.025110245, 1.0, //
            173.194076538, 133.330886841, 106.183448792, 1.0, //
            139.210220337, 191.922363281, 147.056488037, 1.0, //
            157.905166626, 111.975311279, 192.204727173, 1.0, //
            79.229278564, 100.424659729, 268.442108154, 1.0, //
            132.888137817, 173.358779907, 341.715240479, 1.0, //
            // OCIO test values
            26.112514496, 42.523605347, 4.173158169, 0.5, //
            79.190460205, 25.002300262, 332.159759521, 1.0, //
            81.912559509, 39.754810333, 182.925750732, 0.0, //
            // ColorChecker24 (SMPTE 2065-1 2021)
            33.924663544, 12.254567146, 38.146659851, 1.0, //
            61.332393646, 15.169423103, 39.841842651, 1.0, //
            47.191543579, 11.839941978, 249.107116699, 1.0, //
            37.328300476, 13.224150658, 128.878036499, 1.0, //
            53.465549469, 13.121579170, 285.658966064, 1.0, //
            65.414512634, 19.172147751, 179.324264526, 1.0, //
            55.711513519, 37.182041168, 50.924011230, 1.0, //
            40.020961761, 20.762512207, 271.008331299, 1.0, //
            47.704769135, 35.791145325, 13.975610733, 1.0, //
            30.385913849, 14.544739723, 317.544281006, 1.0, //
            64.222846985, 33.487697601, 119.145133972, 1.0, //
            65.570358276, 35.864013672, 70.842193604, 1.0, //
            31.800464630, 23.920211792, 273.228973389, 1.0, //
            47.950405121, 28.027387619, 144.154159546, 1.0, //
            38.440967560, 42.604164124, 17.892261505, 1.0, //
            75.117736816, 40.952045441, 90.752044678, 1.0, //
            49.311210632, 33.812240601, 348.832092285, 1.0, //
            47.441757202, 22.915655136, 218.454376221, 1.0, //
            93.610260010, 0.439610571, 108.271926880, 1.0, //
            77.237663269, 0.131636351, 33.296318054, 1.0, //
            61.655914307, 0.041985143, 291.004058838, 1.0, //
            47.493667603, 0.048908804, 297.386047363, 1.0, //
            33.264842987, 0.283808023, 234.276382446, 1.0, //
            21.467216492, 0.409062684, 255.025634766, 1.0, //
            // Spectrally non-selective 18 % reflecting diffuser
            44.938602448, 0.000004705, 299.357757568, 1.0, //
            // Perfect reflecting diffuser
            98.969635010, 0.000083445, 5.640549183, 1.0,
        ];
        test.set_custom_values(values);

        test.set_error_threshold(1e-4);
    });
}

/// The tonescale compressed JMh values: input values of
/// `style_aces2_tonescale_compress_inv`, `style_aces2_gamut_compress_fwd` and
/// `style_aces2_gamut_compress_no1dlut`.
#[rustfmt::skip]
const TONESCALE_COMPRESSED_VALUES: [f32; 140] = [
    // ACEScg primaries and secondaries scaled by 4
    110.702453613, 211.251770020, 25.025110245, 1.0, //
    168.016815186, 129.796249390, 106.183448792, 1.0, //
    140.814849854, 193.459213257, 147.056488037, 1.0, //
    156.429519653, 110.938514709, 192.204727173, 1.0, //
    80.456542969, 98.490524292, 268.442108154, 1.0, //
    135.172195435, 175.559280396, 341.715240479, 1.0, //
    // OCIO test values
    18.187314987, 33.819175720, 4.173158169, 0.5, //
    80.413116455, 21.309329987, 332.159759521, 1.0, //
    83.447891235, 37.852291107, 182.925750732, 0.0, //
    // ColorChecker24 (SMPTE 2065-1 2021)
    27.411964417, 13.382769585, 38.146659851, 1.0, //
    59.987670898, 14.391894341, 39.841842651, 1.0, //
    43.298923492, 12.199877739, 249.107116699, 1.0, //
    31.489658356, 14.075142860, 128.878036499, 1.0, //
    50.749198914, 12.731814384, 285.658966064, 1.0, //
    64.728637695, 18.593795776, 179.324264526, 1.0, //
    53.399448395, 37.394428253, 50.924011230, 1.0, //
    34.719596863, 21.616765976, 271.008331299, 1.0, //
    43.910713196, 36.788166046, 13.975610733, 1.0, //
    23.196525574, 15.118354797, 317.544281006, 1.0, //
    63.348674774, 33.283493042, 119.145133972, 1.0, //
    64.908889771, 35.371044159, 70.842193604, 1.0, //
    24.876911163, 23.143159866, 273.228973389, 1.0, //
    44.203376770, 28.918329239, 144.154159546, 1.0, //
    32.824356079, 43.447875977, 17.892261505, 1.0, //
    75.830871582, 39.872474670, 90.752044678, 1.0, //
    45.823116302, 34.652069092, 348.832092285, 1.0, //
    43.597240448, 23.079078674, 218.454376221, 1.0, //
    96.212783813, 0.322624743, 108.271926880, 1.0, //
    78.222122192, 0.094044082, 33.296318054, 1.0, //
    60.364795685, 0.031291425, 291.004058838, 1.0, //
    43.659111023, 0.038717352, 297.386047363, 1.0, //
    26.623359680, 0.269155562, 234.276382446, 1.0, //
    12.961384773, 0.366550505, 255.025634766, 1.0, //
    // Spectrally non-selective 18 % reflecting diffuser
    40.609165192, 0.000000000, 299.357757568, 1.0, //
    // Perfect reflecting diffuser
    101.899215698, 0.000068110, 5.640549183, 1.0,
];

#[test]
fn style_aces2_tonescale_compress_inv() {
    gpu_test("FixedFunction/style_aces2_tonescale_compress_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesTonescaleCompress20,
            &[1000.0],
            TransformDirection::Inverse,
        ));

        test.set_custom_values(TONESCALE_COMPRESSED_VALUES.to_vec());

        test.set_error_threshold(1e-4);
    });
}

#[test]
fn style_aces2_gamut_compress_fwd() {
    gpu_test("FixedFunction/style_aces2_gamut_compress_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGamutCompress20,
            &P3D65_1000NIT_PARAMS,
            TransformDirection::Forward,
        ));

        test.set_custom_values(TONESCALE_COMPRESSED_VALUES.to_vec());

        test.set_error_threshold(1e-4);
    });
}

#[test]
fn style_aces2_gamut_compress_inv() {
    gpu_test("FixedFunction/style_aces2_gamut_compress_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGamutCompress20,
            &P3D65_1000NIT_PARAMS,
            TransformDirection::Inverse,
        ));

        #[rustfmt::skip]
        let values = vec![
            // ACEScg primaries and secondaries scaled by 4
            107.831291199, 174.252944946, 25.025119781, 1.0, //
            168.028198242, 118.224960327, 106.183464050, 1.0, //
            140.030105591, 127.177192688, 147.056488037, 1.0, //
            156.512435913, 73.218856812, 192.204727173, 1.0, //
            79.378631592, 72.613555908, 268.442108154, 1.0, //
            133.827835083, 149.929809570, 341.715240479, 1.0, //
            // OCIO test values
            18.194000244, 33.312938690, 4.173166752, 0.5, //
            80.413116455, 21.309329987, 332.159759521, 1.0, //
            83.467437744, 37.305160522, 182.925750732, 0.0, //
            // ColorChecker24 (SMPTE 2065-1 2021)
            27.411962509, 13.382793427, 38.146591187, 1.0, //
            59.987670898, 14.391893387, 39.841842651, 1.0, //
            43.298923492, 12.199877739, 249.107116699, 1.0, //
            31.489658356, 14.075142860, 128.878036499, 1.0, //
            50.749198914, 12.731814384, 285.658966064, 1.0, //
            64.728637695, 18.593795776, 179.324264526, 1.0, //
            53.399448395, 37.394428253, 50.924011230, 1.0, //
            34.719596863, 21.616765976, 271.008331299, 1.0, //
            43.910709381, 36.788166046, 13.975610733, 1.0, //
            23.196525574, 15.118361473, 317.544250488, 1.0, //
            63.348674774, 33.283493042, 119.145133972, 1.0, //
            64.908889771, 35.371044159, 70.842193604, 1.0, //
            24.876916885, 23.143167496, 273.229034424, 1.0, //
            44.203376770, 28.918329239, 144.154159546, 1.0, //
            32.824352264, 43.447864532, 17.892255783, 1.0, //
            75.830871582, 39.872474670, 90.752044678, 1.0, //
            45.823104858, 34.652038574, 348.832092285, 1.0, //
            43.635551453, 21.629474640, 218.454376221, 1.0,
        ];
        test.set_custom_values(values);

        // TODO: Improve inversion match?
        test.set_error_threshold(4e-4);
    });
}

#[test]
fn style_aces2_gamut_compress_no1dlut() {
    gpu_test("FixedFunction/style_aces2_gamut_compress_no1dlut", |test| {
        // Test the 2D TEXTURE path used for Reach and Cusp table sampling
        // OpenGL tests would otherwise not use it and prefer 1D TEXTURE

        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesGamutCompress20,
            &P3D65_1000NIT_PARAMS,
            TransformDirection::Inverse,
        ));

        test.set_custom_values(TONESCALE_COMPRESSED_VALUES.to_vec());

        test.set_error_threshold(1e-4);

        test.shader_desc_setup = Some(Box::new(|desc| desc.set_allow_texture_1d(false)));
    });
}

// The next four tests run into a problem on some graphics cards where 0.0 * Inf = 0.0,
// rather than the correct value of NaN.  Therefore turning off TestInfinity for these tests.

#[test]
fn style_aces_darktodim10_fwd() {
    gpu_test("FixedFunction/style_aces_darktodim10_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesDarkToDim10,
            &[],
            TransformDirection::Forward,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
    });
}

#[test]
fn style_aces_darktodim10_inv() {
    gpu_test("FixedFunction/style_aces_darktodim10_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::AcesDarkToDim10,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
    });
}

#[test]
fn style_rec2100_surround_fwd() {
    gpu_test("FixedFunction/style_rec2100_surround_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::Rec2100Surround,
            &[0.7],
            TransformDirection::Forward,
        ));
        test.set_error_threshold(2e-6);
        test.set_test_infinity(false);
    });
}

#[test]
fn style_rec2100_surround_inv() {
    gpu_test("FixedFunction/style_rec2100_surround_inv", |test| {
        // (Since we're not calling inverse() here, set the param to be the inverse of prev test.)
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::Rec2100Surround,
            &[1. / 0.7],
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(2e-6);
        test.set_test_infinity(false);
    });
}

#[test]
fn style_rgb_to_hsv_fwd() {
    gpu_test("FixedFunction/style_RGB_TO_HSV_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsv,
            &[],
            TransformDirection::Forward,
        ));

        test.set_error_threshold(1e-6);

        test.set_test_nan(false);
        if cfg!(target_os = "macos") {
            test.set_test_infinity(false);
        }
    });
}

#[test]
fn style_rgb_to_hsv_fwd_custom() {
    gpu_test("FixedFunction/style_RGB_TO_HSV_fwd_custom", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsv,
            &[],
            TransformDirection::Forward,
        ));

        #[rustfmt::skip]
        let values = vec![
            1.500, 2.500, 0.500, 0.50, //
            3.125, -0.625, 1.250, 1.00, //
            -5.0 / 3.0, -4.0 / 3.0, -1.0 / 3.0, 0.25, //
            0.100, -0.800, 0.400, 0.25,
        ];
        test.set_custom_values(values);

        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_rgb_to_hsv_inv() {
    gpu_test("FixedFunction/style_RGB_TO_HSV_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsv,
            &[],
            TransformDirection::Inverse,
        ));

        // Mostly passes at 1e-6 but there are some values where the result is large in one chan
        // and very small in another and so doing relative comparison per channel is not fair.
        test.set_error_threshold(5e-4);
        test.set_expected_minimal_value(5e-3);
        test.set_relative_comparison(true);
    });
}

#[test]
fn style_rgb_to_hsv_inv_custom() {
    gpu_test("FixedFunction/style_RGB_TO_HSV_inv_custom", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsv,
            &[],
            TransformDirection::Inverse,
        ));

        #[rustfmt::skip]
        let values = vec![
            3.0 / 12.0, 0.80, 2.50, 0.50, // val > 1
            11.0 / 12.0, 1.20, 2.50, 1.00, // sat > 1
            15.0 / 24.0, 0.80, -2.00, 0.25, // val < 0
            19.0 / 24.0, 1.50, -0.40, 0.25, // sat > 1, val < 0
            -89.0 / 24.0, 0.50, 0.40, 2.00, // under-range hue
            81.0 / 24.0, 1.50, -0.40, -0.25, // over-range hue, sat > 1, val < 0
            81.0 / 24.0, -0.50, 0.40, 0.00, // sat < 0
            0.5000, 2.50, 0.04, 0.00, // sat > 2
        ];
        test.set_custom_values(values);

        test.set_error_threshold(1e-6);
    });
}

#[rustfmt::skip]
const HSY_FWD_VALUES: [f32; 32] = [
    -0.075290, 0.078996, -0.108397, 0.0, //
    0.3, 0.4, 0.5, 0.0, //
    0.05, 0.03, 0.04, 0.0, //
    0.01, 0.01, -0.05, 1.0, //
    0.05, -0.005, -0.05, 1.0, //
    -0.048, 0.01, 0.05, 0.0, //
    0.3, -0.4, 0.5, 0.0, //
    -0.055, 0.01, 0.05, 0.0,
];

#[rustfmt::skip]
const HSY_INV_VALUES: [f32; 32] = [
    0.470554752, 9.12594033, 0.0326650218, 0.0, // hsy alpha == 1
    0.75, 0.22196741, 0.38596, 0.0, //
    0.08333333, 0.12976444, 0.034974, 0.0, //
    0.333333333333, 0.606036032, 0.0056680, 1.0, // hsy mid alpha
    0.241666666667, 0.8372990325, 0.0034440, 1.0, //
    0.734693877551, 0.752099600, 0.0005572, 0.0, // hsy alpha == 0
    0.96296296, 9.7034, -0.1862, 0.0, //
    0.730158730159, 0.811517000, -0.0009310, 0.0,
];

#[test]
fn style_rgb_to_hsy_lin_fwd() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_LIN_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyLin,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(HSY_FWD_VALUES.to_vec());
        test.set_error_threshold(5e-6);
    });
}

#[test]
fn style_rgb_to_hsy_lin_inv() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_LIN_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyLin,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(HSY_INV_VALUES.to_vec());
        test.set_error_threshold(5e-6);
    });
}

#[test]
fn style_rgb_to_hsy_log_fwd() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_LOG_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyLog,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(HSY_FWD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_rgb_to_hsy_log_inv() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_LOG_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyLog,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(HSY_INV_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_rgb_to_hsy_vid_fwd() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_VID_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyVid,
            &[],
            TransformDirection::Forward,
        ));
        test.set_custom_values(HSY_FWD_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_rgb_to_hsy_vid_inv() {
    gpu_test("FixedFunction/style_RGB_TO_HSY_VID_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::RgbToHsyVid,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_custom_values(HSY_INV_VALUES.to_vec());
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_xyz_to_xyy_fwd() {
    gpu_test("FixedFunction/style_XYZ_TO_xyY_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToXyy,
            &[],
            TransformDirection::Forward,
        ));
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_xyz_to_xyy_inv() {
    gpu_test("FixedFunction/style_XYZ_TO_xyY_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToXyy,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_xyz_to_uvy_fwd() {
    gpu_test("FixedFunction/style_XYZ_TO_uvY_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToUvy,
            &[],
            TransformDirection::Forward,
        ));
        test.set_error_threshold(1e-6);
    });
}

#[test]
fn style_xyz_to_uvy_inv() {
    gpu_test("FixedFunction/style_XYZ_TO_uvY_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToUvy,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(1e-5);
    });
}

#[test]
fn style_xyz_to_luv_fwd() {
    gpu_test("FixedFunction/style_XYZ_TO_LUV_fwd", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToLuv,
            &[],
            TransformDirection::Forward,
        ));
        test.set_test_infinity(false);
        test.set_error_threshold(5e-5);
    });
}

#[test]
fn style_xyz_to_luv_inv() {
    gpu_test("FixedFunction/style_XYZ_TO_LUV_inv", |test| {
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::XyzToLuv,
            &[],
            TransformDirection::Inverse,
        ));
        test.set_test_infinity(false);
        test.set_error_threshold(1e-5);
    });
}

#[test]
fn style_lin_to_pq_fwd() {
    gpu_test("FixedFunction/style_LIN_TO_PQ_fwd", |test| {
        test.set_wide_range_interval(-0.1, 100.1);
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToPq,
            &[],
            TransformDirection::Forward,
        ));

        test.set_test_infinity(false);
        test.set_test_nan(false);

        // Using large threshold for SSE2 as that will enable usage of fast but
        // approximate power function ssePower.
        test.set_error_threshold(0.0008); // 2e-5 without OCIO_USE_SSE2.
    });
}

#[test]
fn style_lin_to_pq_inv() {
    gpu_test("FixedFunction/style_LIN_TO_PQ_inv", |test| {
        // Picking a tight epsilon is tricky with this function due to nested power
        // operations and [0,100] output range for [0,1] input range.

        // MaxDiff in range [-0.1, 1.1] against...
        //      scalar double precision         : 0.000094506
        //      scalar single precision         : 0.000144501
        //      SSE2 (intrinsic pow)            : 0.000144441
        //      SSE2 (fastPower)                : 0.002207260
        test.set_wide_range_interval(-0.1, 1.1);
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToPq,
            &[],
            TransformDirection::Inverse,
        ));
        // Since the output range will be 0..100, we set the relative epsilon.
        test.set_relative_comparison(true);
        test.set_error_threshold(0.0023); // 1.5e-4 without OCIO_USE_SSE2.
        test.set_test_infinity(false);
        test.set_test_nan(false);
    });
}

// Parameters for the Rec.2100 HLG curve.
#[rustfmt::skip]
const HLG_PARAMS: [f64; 10] = [
    0.0,            // mirror point
    0.25,           // break point

    // Gamma segment.
    0.5,            // gamma power
    1.0,            // post-power scale
    0.0,            // pre-power offset

    // Log segment.
    std::f64::consts::E, // log base (e)
    0.17883277,     // log-side slope
    0.807825590164, // log-side offset
    1.0,            // lin-side slope
    -0.07116723,    // lin-side offset
];

#[test]
fn style_lin_to_gamma_log_fwd() {
    gpu_test("FixedFunction/style_LIN_TO_GAMMA_LOG_fwd", |test| {
        test.set_wide_range_interval(-0.1, 3.35); // Output ~[-0.3, 1.02]
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToGammaLog,
            &HLG_PARAMS,
            TransformDirection::Forward,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
        test.set_test_nan(false);
    });
}

#[test]
fn style_lin_to_gamma_log_inv() {
    gpu_test("FixedFunction/style_LIN_TO_GAMMA_LOG_inv", |test| {
        test.set_wide_range_interval(-0.3, 1.02); // Output ~[-0.1, 3.35]
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToGammaLog,
            &HLG_PARAMS,
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
    });
}

#[rustfmt::skip]
const DOUBLE_LOG_PARAMS: [f64; 13] = [
    10.0,                  // Base for the log
    0.1,                   // Break point between Log1 and Linear segments
    0.5,                   // Break point between Linear and Log2 segments
    -1.0, -1.0, -1.0, 0.2, // Log curve 1: LogSideSlope, LogSideOffset, LinSideSlope, LinSideOffset
    1.0, 1.0, 1.0, 0.5,    // Log curve 2: LogSideSlope, LogSideOffset, LinSideSlope, LinSideOffset
    1.0, 0.0,              // Linear segment slope and offset
];

#[test]
fn style_lin_to_double_log_fwd() {
    gpu_test("FixedFunction/style_LIN_TO_DOUBLE_LOG_fwd", |test| {
        test.set_wide_range_interval(-1.0, 2.0); // Output ~[-1.08, 1.4]
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToDoubleLog,
            &DOUBLE_LOG_PARAMS,
            TransformDirection::Forward,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
        test.set_test_nan(false);
    });
}

#[test]
fn style_lin_to_double_log_inv() {
    gpu_test("FixedFunction/style_LIN_TO_DOUBLE_LOG_inv", |test| {
        test.set_wide_range_interval(-1.1, 1.4); // Output ~[-1.0, 2.0]
        test.set_processor_transform(fixed_function(
            FixedFunctionStyle::LinToDoubleLog,
            &DOUBLE_LOG_PARAMS,
            TransformDirection::Inverse,
        ));
        test.set_error_threshold(1e-6);
        test.set_test_infinity(false);
    });
}
