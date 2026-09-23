//! Reader tests (port of the reader part of `FileFormatCTF_tests.cpp`).

use super::opdata::*;
use super::reader::parse_ctf;
use super::transform::*;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::transforms::*;
use crate::types::*;

pub(super) const TEST_FILES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files/");

pub(super) fn test_path(name: &str) -> String {
    format!("{TEST_FILES}{name}")
}

/// Port of `LoadCLFFile`.
pub(super) fn load(name: &str) -> Result<CtfReaderTransform> {
    let p = test_path(name);
    let data = std::fs::read(&p).map_err(|_| Error::msg("Error opening test file."))?;
    parse_ctf(&data, &p).map(|r| r.transform)
}

/// Load a file and also return the collected warnings.
pub(super) fn load_with_warnings(name: &str) -> Result<(CtfReaderTransform, Vec<String>)> {
    let p = test_path(name);
    let data = std::fs::read(&p).map_err(|_| Error::msg("Error opening test file."))?;
    parse_ctf(&data, &p).map(|r| (r.transform, r.warnings))
}

/// Port of `ParseString`.
pub(super) fn parse(s: &str) -> Result<CtfReaderTransform> {
    parse_ctf(s.as_bytes(), "").map(|r| r.transform)
}

pub(super) fn parse_with_warnings(s: &str) -> Result<(CtfReaderTransform, Vec<String>)> {
    parse_ctf(s.as_bytes(), "").map(|r| (r.transform, r.warnings))
}

#[track_caller]
pub(super) fn check_err<T: std::fmt::Debug>(r: Result<T>, what: &str) {
    match r {
        Ok(v) => panic!("expected an error containing {what:?}, got {v:?}"),
        Err(e) => assert!(
            e.message().contains(what),
            "error {:?} does not contain {:?}",
            e.message(),
            what
        ),
    }
}

#[track_caller]
pub(super) fn check_load_err(name: &str, what: &str) {
    check_err(load(name), what);
}

pub(super) fn descs(meta: &FormatMetadata, name: &str) -> Vec<String> {
    meta.children
        .iter()
        .filter(|c| c.element_name.eq_ignore_ascii_case(name))
        .map(|c| c.element_value.clone())
        .collect()
}

pub(super) fn op_name(op: &OpData) -> &str {
    op.metadata().attribute_value(METADATA_NAME)
}

pub(super) fn op_id(op: &OpData) -> &str {
    op.metadata().attribute_value(METADATA_ID)
}

pub(super) fn op_descs(op: &OpData) -> Vec<String> {
    descs(op.metadata(), METADATA_DESCRIPTION)
}

macro_rules! accessor {
    ($fn:ident, $variant:ident, $ty:ty) => {
        #[track_caller]
        pub(super) fn $fn(op: &OpData) -> &$ty {
            match op {
                OpData::$variant(d) => d,
                _ => panic!("unexpected op type {}", op.type_name()),
            }
        }
    };
}

accessor!(as_matrix, Matrix, MatrixData);
accessor!(as_range, Range, RangeData);
accessor!(as_lut1d, Lut1D, Lut1DData);
accessor!(as_lut3d, Lut3D, Lut3DData);
accessor!(as_gamma, Gamma, GammaData);
accessor!(as_cdl, Cdl, CdlData);
accessor!(as_log, Log, LogData);
accessor!(as_ec, ExposureContrast, EcData);
accessor!(as_ff, FixedFunction, FfData);
accessor!(as_reference, Reference, ReferenceData);
accessor!(as_primary, GradingPrimary, GradingPrimaryData);
accessor!(as_rgb_curve, GradingRgbCurve, GradingRgbCurveData);
accessor!(as_hue_curve, GradingHueCurve, GradingHueCurveData);
accessor!(as_tone, GradingTone, GradingToneData);

/// Port of `MatrixOpData::isIdentity`.
pub(super) fn matrix_is_identity(m: &MatrixData) -> bool {
    if m.has_offsets() || m.has_alpha() {
        return false;
    }
    for r in 0..4 {
        for c in 0..4 {
            if r != c && m.matrix[r * 4 + c] != 0.0 {
                return false;
            }
        }
    }
    (0..4).all(|i| (m.matrix[i * 5] - 1.0).abs() <= 1e-6)
}

pub(super) fn half_bits_to_f32(bits: u16) -> f32 {
    half::f16::from_bits(bits).to_f32()
}

#[test]
fn missing_file() {
    check_err(load("xxxxxxxxxxxxxxxxx.xxxxx"), "Error opening test file.");
}

#[test]
fn clf_examples() {
    {
        let t = load("clf/lut1d_example.clf").unwrap();
        assert_eq!(t.name(), "transform example lut1d");
        assert_eq!(t.id(), "exlut1");
        let d = descs(&t.metadata, METADATA_DESCRIPTION);
        assert_eq!(d, vec!["1D LUT with legal out of range values"]);
        assert_eq!(t.ops.len(), 1);
        assert_eq!(op_name(&t.ops[0]), "65valueLut");
        assert_eq!(op_id(&t.ops[0]), "lut-23");
        let lut = as_lut1d(&t.ops[0]);
        assert_eq!(lut.file_output_bd, BitDepth::UInt12);
        let d = op_descs(&t.ops[0]);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0],
            "Note that the bit-depth does not constrain the legal range of values."
        );
        assert_eq!(d[1], "Formula: flipud(1.25 - 1.5 * x^2.2)");
    }
    {
        let t = load("clf/lut3d_identity_12i_16f.clf").unwrap();
        assert_eq!(t.name(), "transform example lut3d");
        assert_eq!(t.id(), "exlut2");
        assert_eq!(
            descs(&t.metadata, METADATA_DESCRIPTION),
            vec![" 3D LUT example "]
        );
        assert_eq!(t.ops.len(), 1);
        assert_eq!(op_name(&t.ops[0]), "identity");
        assert_eq!(op_id(&t.ops[0]), "lut-24");
        let lut = as_lut3d(&t.ops[0]);
        assert_eq!(lut.interpolation, Interpolation::Tetrahedral);
        assert_eq!(lut.file_output_bd, BitDepth::F16);
        assert_eq!(op_descs(&t.ops[0]), vec![" 3D LUT "]);
    }
    {
        let t = load("clf/matrix_3x4_example.clf").unwrap();
        assert_eq!(t.name(), "transform example matrix");
        assert_eq!(t.id(), "exmat1");
        let md = &t.metadata;
        assert_eq!(md.children.len(), 2);
        assert_eq!(md.children[0].element_name, "Description");
        assert_eq!(md.children[0].element_value, " Matrix example ");
        assert_eq!(md.children[1].element_name, "Description");
        assert_eq!(md.children[1].element_value, " Used by unit tests ");

        assert_eq!(t.ops.len(), 1);
        assert_eq!(op_name(&t.ops[0]), "colorspace conversion");
        assert_eq!(op_id(&t.ops[0]), "mat-25");
        let mat = as_matrix(&t.ops[0]);
        assert_eq!(mat.file_in_bd, BitDepth::UInt10);
        assert_eq!(mat.file_out_bd, BitDepth::UInt12);
        assert_eq!(
            op_descs(&t.ops[0]),
            vec![" 3x4 Matrix , 4th column is offset "]
        );

        let oscale = BitDepth::UInt12.max_value();
        let scale = oscale / BitDepth::UInt10.max_value();
        let v = &mat.matrix;
        assert_eq!(v[0] * scale, 3.60);
        assert_eq!(v[1] * scale, 0.10);
        assert_eq!(v[2] * scale, -0.20);
        assert_eq!(v[3], 0.0);
        assert_eq!(v[4] * scale, 0.20);
        assert_eq!(v[5] * scale, 3.50);
        assert_eq!(v[6] * scale, 0.10);
        assert_eq!(v[7], 0.0);
        assert_eq!(v[8] * scale, 0.10);
        assert_eq!(v[9] * scale, -0.30);
        assert_eq!(v[10] * scale, 3.40);
        assert_eq!(v[11], 0.0);
        assert_eq!(v[12], 0.0);
        assert_eq!(v[13], 0.0);
        assert_eq!(v[14], 0.0);
        assert_eq!(v[15], 1.0);
        let o = &mat.offsets;
        assert_eq!(o[0] * oscale, 0.30);
        assert_eq!(o[1] * oscale, -0.05);
        assert_eq!(o[2] * oscale, -0.40);
        assert_eq!(o[3], 0.0);
    }
    {
        // Test two-entries IndexMap support.
        let t = load("indexMap_test_clfv2.clf").unwrap();
        assert_eq!(t.name(), "transform example lut IndexMap");
        assert_eq!(t.id(), "exlut3");
        let md = &t.metadata;
        assert_eq!(md.children.len(), 1);
        assert_eq!(md.children[0].element_name, "Description");
        assert_eq!(
            md.children[0].element_value,
            " IndexMap LUT example from spec "
        );

        assert_eq!(t.ops.len(), 2);
        let r = as_range(&t.ops[0]);
        assert_eq!(r.file_in_bd, BitDepth::UInt10);
        assert_eq!(r.file_out_bd, BitDepth::UInt10);
        assert_eq!(r.min_in, 64. / 1023.);
        assert_eq!(r.max_in, 940. / 1023.);
        assert_eq!(r.min_out, 0. / 1023.);
        assert_eq!(r.max_out, 1023. / 1023.);

        assert_eq!(op_name(&t.ops[1]), "IndexMap LUT");
        assert_eq!(op_id(&t.ops[1]), "lut-26");
        let lut = as_lut1d(&t.ops[1]);
        assert_eq!(lut.file_output_bd, BitDepth::F16);
        assert_eq!(op_descs(&t.ops[1]), vec![" 1D LUT with IndexMap "]);
    }
}

#[track_caller]
fn check_matrix(m: &MatrixData, v: [f64; 16], o: [f64; 4]) {
    assert_eq!(m.matrix, v);
    assert_eq!(m.offsets, o);
}

const M_3X3: [f64; 16] = [
    3.24, -1.537, -0.49850, 0.0, -0.96930, 1.876, 0.04156, 0.0, 0.05560, -0.204, 1.0573, 0.0, 0.0,
    0.0, 0.0, 1.0,
];

#[test]
fn matrix4x4() {
    let t = load("matrix_example4x4.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_2);
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(descs(&t.metadata, METADATA_INPUT_DESCRIPTOR), vec!["XYZ"]);
    assert_eq!(descs(&t.metadata, METADATA_OUTPUT_DESCRIPTOR), vec!["RGB"]);
    assert_eq!(m.file_in_bd, BitDepth::F32);
    assert_eq!(m.file_out_bd, BitDepth::F32);
    let mut v = M_3X3;
    // Validate double precision can be read both matrix and offsets.
    v[10] = 1.123456789012;
    check_matrix(m, v, [0.987654321098, 0.2, 0.3, 0.0]);
}

#[test]
fn matrix_with_offset() {
    let t = load("matrix_offsets_example.ctf").unwrap();
    // The ProcessList does not have a version attribute and therefore
    // defaults to 1.2.
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_2);
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(m.matrix, M_3X3);
    assert_eq!(m.offsets[0], 1.0);
    assert_eq!(m.offsets[1], 2.0);
    assert_eq!(m.offsets[2], 3.0);
}

#[test]
fn matrix_with_offset_1_3() {
    // Matrix 4 4 3 only valid up to version 1.2.
    check_load_err(
        "matrix_offsets_example_1_3.ctf",
        "Illegal array dimensions 4 4 3",
    );
}

#[test]
fn matrix_1_3_3x3() {
    let t = load("matrix_example_1_3_3x3.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_3);
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(descs(&t.metadata, METADATA_INPUT_DESCRIPTOR), vec!["XYZ"]);
    assert_eq!(descs(&t.metadata, METADATA_OUTPUT_DESCRIPTOR), vec!["RGB"]);
    assert_eq!(m.file_in_bd, BitDepth::UInt10);
    assert_eq!(m.file_out_bd, BitDepth::UInt10);
    // 3x3 array gets extended to 4x4.
    check_matrix(m, M_3X3, [0.0; 4]);
}

#[test]
fn matrix_1_3_4x4() {
    let t = load("matrix_example_1_3_4x4.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_3);
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    check_matrix(
        m,
        [
            3.24, -1.537, -0.49850, -0.1, -0.96930, 1.876, 0.04156, -0.2, 0.05560, -0.204, 1.0573,
            -0.3, 0.11, 0.22, 0.33, 0.4,
        ],
        [0.0; 4],
    );
}

#[test]
fn matrix_1_3_offsets() {
    let t = load("matrix_example_1_3_offsets.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_3);
    assert_eq!(t.ops.len(), 1);
    check_matrix(as_matrix(&t.ops[0]), M_3X3, [0.1, 0.2, 0.3, 0.0]);
}

#[test]
fn matrix_1_3_alpha_offsets() {
    let t = load("matrix_example_1_3_alpha_offsets.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_3);
    assert_eq!(t.ops.len(), 1);
    check_matrix(
        as_matrix(&t.ops[0]),
        [
            3.24, -1.537, -0.49850, 0.6, -0.96930, 1.876, 0.04156, 0.7, 0.05560, -0.204, 1.0573,
            0.8, 1.2, 1.3, 1.4, 1.5,
        ],
        [0.1, 0.2, 0.3, 0.4],
    );
}

#[track_caller]
fn check_identity(s: &str) {
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 1);
    assert!(matrix_is_identity(as_matrix(&t.ops[0])));
}

#[test]
fn matrix_identity() {
    // Pre version 1.3 matrix parsing.
    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none">
    <Description>RGB matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 3 3">
4.0029325513196481 0 0
0 4.0029325513196481 0
0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.2">
    <Description>RGB matrix + offset Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="4 4 3">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
0 0                  0 0
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
    // Version 1.3 and onward matrix parsing.
    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGB matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 3 3">
4.0029325513196481 0 0
0 4.0029325513196481 0
0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGBA matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="4 4 4">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
0 0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGB matrix + offset Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 4 3">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
}

#[test]
fn lut_1d() {
    {
        let t = load("lut1d_32_10i_10i.ctf").unwrap();
        assert_eq!(t.name(), "1d-lut example");
        assert_eq!(t.id(), "9843a859-e41e-40a8-a51c-840889c3774e");
        let md = &t.metadata;
        assert_eq!(md.children.len(), 3);
        assert_eq!(md.children[0].element_name, "Description");
        assert_eq!(md.children[0].element_value, "Apply a 1/2.2 gamma.");
        assert_eq!(md.children[1].element_name, "InputDescriptor");
        assert_eq!(md.children[1].element_value, "RGB");
        assert_eq!(md.children[2].element_name, "OutputDescriptor");
        assert_eq!(md.children[2].element_value, "RGB");
        assert_eq!(descs(md, METADATA_INPUT_DESCRIPTOR), vec!["RGB"]);
        assert_eq!(descs(md, METADATA_OUTPUT_DESCRIPTOR), vec!["RGB"]);

        assert_eq!(t.ops.len(), 1);
        let lut = as_lut1d(&t.ops[0]);
        assert_eq!(op_descs(&t.ops[0]).len(), 1);
        assert!(!lut.half_domain);
        assert!(!lut.raw_halfs);
        assert_eq!(lut.hue_adjust, Lut1DHueAdjust::None);
        assert_eq!(lut.file_output_bd, BitDepth::UInt10);
        assert_eq!(op_name(&t.ops[0]), "1d-lut example op");

        // LUT is defined with a 32x1 array, extended to 32x3.
        assert_eq!(lut.length, 32);
        assert_eq!(lut.num_components, 1);
        let v = &lut.values;
        assert_eq!(v.len(), 96);
        assert_eq!(v[0], 0.0);
        assert_eq!(v[1], 0.0);
        assert_eq!(v[2], 0.0);
        assert_eq!(v[3], 215.0f32 / 1023.0f32);
        assert_eq!(v[4], 215.0f32 / 1023.0f32);
        assert_eq!(v[5], 215.0f32 / 1023.0f32);
        assert_eq!(v[6], 294.0f32 / 1023.0f32);
        assert_eq!(v[92], 1008.0f32 / 1023.0f32);
        assert_eq!(v[93], 1023.0f32 / 1023.0f32);
        assert_eq!(v[94], 1023.0f32 / 1023.0f32);
        assert_eq!(v[95], 1023.0f32 / 1023.0f32);
    }
    {
        let t = load("lut1d_hue_adjust_test.ctf").unwrap();
        assert_eq!(t.ops.len(), 1);
        assert_eq!(as_lut1d(&t.ops[0]).hue_adjust, Lut1DHueAdjust::Dw3);
    }
}

#[test]
fn lut1d_hue_adjust_invalid_style() {
    check_load_err(
        "lut1d_hue_adjust_invalid_style.ctf",
        "Illegal 'hueAdjust' attribute",
    );
}

#[test]
fn lut_3by1d_with_nan_infinity() {
    let t = load("lut3by1d_nan_infinity_example.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let v = &as_lut1d(&t.ops[0]).values;
    for x in &v[0..5] {
        assert!(x.is_nan());
    }
    assert_eq!(v[5], f32::INFINITY);
    assert_eq!(v[6], f32::INFINITY);
    assert_eq!(v[7], f32::INFINITY);
    assert_eq!(v[8], f32::NEG_INFINITY);
    assert_eq!(v[9], f32::NEG_INFINITY);
}

#[test]
fn lut1d_half_domain_set_false() {
    check_load_err(
        "clf/illegal/lut1d_half_domain_set_false.clf",
        "Illegal 'halfDomain' attribute",
    );
}

#[test]
fn lut1d_raw_half_set_false() {
    check_load_err(
        "clf/illegal/lut1d_raw_half_set_false.clf",
        "Illegal 'rawHalfs' attribute",
    );
}

#[test]
fn lut1d_half_domain_raw_half_set() {
    let t = load("clf/lut1d_half_domain_raw_half_set.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut1d(&t.ops[0]);
    assert!(lut.half_domain);
    assert!(lut.raw_halfs);
    assert_eq!(lut.values[0], half_bits_to_f32(44646));
    assert_eq!(lut.values[3], half_bits_to_f32(44637));
    assert_eq!(lut.values[6], half_bits_to_f32(44634));
    assert_eq!(lut.values[9], half_bits_to_f32(44631));
    assert_eq!(lut.values[12], half_bits_to_f32(44629));
}

#[test]
fn lut1d_half_domain_missing_values() {
    check_load_err(
        "clf/illegal/lut1d_half_domain_missing_values.clf",
        "65536 required for halfDomain",
    );
}

#[test]
fn lut_3by1d() {
    let t = load("clf/xyz_to_rgb.clf").unwrap();
    assert_eq!(t.ops.len(), 3);
    check_matrix(as_matrix(&t.ops[0]), M_3X3, [0.0; 4]);
    as_range(&t.ops[1]);
    let lut = as_lut1d(&t.ops[2]);
    assert_eq!(lut.dir, TransformDirection::Forward);
    assert_eq!(lut.file_output_bd, BitDepth::F32);
    assert_eq!(lut.length, 128);
    assert_eq!(lut.num_components, 3);
    let v = &lut.values;
    assert_eq!(v.len(), 384);
    assert_eq!(v[0], 0.0);
    assert_eq!(v[1], 0.0);
    assert_eq!(v[2], 0.0);
    assert_eq!(v[3], 0.06780);
    assert_eq!(v[21], 0.19986);
    assert_eq!(v[22], 0.18986);
    assert_eq!(v[23], 0.17987);
    assert_eq!(v[48], 0.31636);
    assert_eq!(v[49], 0.30054);
    assert_eq!(v[50], 0.28472);
}

#[test]
fn lut1d_long_lut() {
    let t = load("clf/lut1d_long.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut1d(&t.ops[0]);
    assert_eq!(lut.length, 131072);
    assert_eq!(lut.num_components, 1);
    assert_eq!(lut.values.len(), 131072 * 3);
    assert_eq!(lut.values[393215], 1.293);
}

#[track_caller]
fn check_close(a: f32, b: f32, tol: f32) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tolerance {tol})");
}

#[test]
fn lut1d_inv() {
    let t = load("lut1d_inv.ctf").unwrap();
    assert_eq!(t.ops.len(), 2);
    check_matrix(as_matrix(&t.ops[0]), M_3X3, [0.0; 4]);
    let lut = as_lut1d(&t.ops[1]);
    assert_eq!(lut.file_output_bd, BitDepth::F32);
    assert_eq!(lut.dir, TransformDirection::Inverse);
    assert_eq!(lut.num_components, 3);
    assert_eq!(lut.length, 17);
    let v = &lut.values;
    assert_eq!(v.len(), 51);
    let e = 1e-6;
    check_close(v[0], 0.0, e);
    check_close(v[1], 0.0, e);
    check_close(v[2], 0.0, e);
    check_close(v[3], 0.28358, e);
    check_close(v[21], 0.68677, e);
    check_close(v[22], 0.68677, e);
    check_close(v[23], 0.68677, e);
    check_close(v[48], 1.0, e);
    check_close(v[49], 1.0, e);
    check_close(v[50], 1.0, e);
}

#[test]
fn lut1d_inv_scaling() {
    // Validate that the InverseLUT1D array values are scaled based on inBitDepth.
    let t = load("lut1d_inverse_halfdom_slog_fclut.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut1d(&t.ops[0]);
    // For an InverseLUT1D, the file "out" depth is actually taken from inBitDepth.
    assert_eq!(lut.file_output_bd, BitDepth::UInt16);
    assert_eq!(lut.dir, TransformDirection::Inverse);
    assert_eq!(lut.num_components, 1);
    assert_eq!(lut.length, 65536);
    assert_eq!(lut.values.len(), 65536 * 3);
    let e = 1e-6;
    // Input value 17830 scaled by 65535.
    check_close(lut.values[0], 0.27206836, e);
    // Input value 55070 scaled by 65535.
    check_close(lut.values[31743 * 3], 0.84031434, e);
}

#[test]
fn invlut1d_clf() {
    let clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UIDLUT42">
    <InverseLUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="16 3">
   0    1    2
   3    4    5
   6    7    8
   9   10   11
  12   13   14
  15   16   17
  18   19   20
  21   22   23
  24   25   26
  27   28   29
  30   31   32
  33   34   35
  36   37   38
  39   40   41
  42   43   44
  45   46   47
        </Array>
    </InverseLUT1D>
</ProcessList>
"#;
    check_err(
        parse(clf),
        "CLF file version '3' does not support operator 'InverseLUT1D'",
    );
}

#[test]
fn lut3d() {
    let t = load("clf/lut3d_17x17x17_10i_12i.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut3d(&t.ops[0]);
    assert_eq!(lut.dir, TransformDirection::Forward);
    assert_eq!(lut.file_output_bd, BitDepth::UInt12);
    // Interpolation is not defined in the file.
    assert_eq!(lut.interpolation, Interpolation::Default);
    assert_eq!(lut.grid_size, 17);
    assert_eq!(lut.num_components, 3);
    let v = &lut.values;
    assert_eq!(v.len(), 17 * 17 * 17 * 3);
    let tol = 2e-8;
    check_close(v[0], 0.0 / 4095.0, tol);
    check_close(v[1], 12.0 / 4095.0, tol);
    check_close(v[2], 13.0 / 4095.0, tol);
    check_close(v[18], 0.0 / 4095.0, tol);
    check_close(v[19], 203.0 / 4095.0, tol);
    check_close(v[20], 399.0 / 4095.0, tol);
    check_close(v[30], 54.0 / 4095.0, tol);
    check_close(v[31], 490.0 / 4095.0, tol);
    check_close(v[32], 987.0 / 4095.0, tol);
}

#[test]
fn lut3d_inv() {
    let t = load("lut3d_example_Inv.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut3d(&t.ops[0]);
    // For an InverseLUT3D, the file "out" depth is set by the inBitDepth of the file.
    assert_eq!(lut.file_output_bd, BitDepth::UInt12);
    assert_eq!(lut.interpolation, Interpolation::Tetrahedral);
    assert_eq!(lut.dir, TransformDirection::Inverse);
    assert_eq!(lut.num_components, 3);
    assert_eq!(lut.grid_size, 17);
    let v = &lut.values;
    assert_eq!(v.len(), 17 * 17 * 17 * 3);
    check_close(v[0], 25.0 / 4095.0, 1e-8);
    check_close(v[1], 30.0 / 4095.0, 1e-8);
    assert_eq!(v[2], 33.0f32 / 4095.0);
    check_close(v[18], 26.0 / 4095.0, 1e-8);
    assert_eq!(v[19], 308.0f32 / 4095.0);
    assert_eq!(v[20], 580.0f32 / 4095.0);
    assert_eq!(v[30], 0.0);
    assert_eq!(v[31], 586.0f32 / 4095.0);
    assert_eq!(v[32], 1350.0f32 / 4095.0);
}

#[test]
fn lut3d_unequal_size() {
    check_load_err(
        "clf/illegal/lut3d_unequal_size.clf",
        "Illegal array dimensions 2 2 3 3",
    );
}

#[test]
fn tabluation_support() {
    // This clf file contains tabulations used as delimiters for a series of numbers.
    let t = load("clf/tabulation_support.clf").unwrap();
    assert_eq!(t.id(), "e0a0ae4b-adc2-4c25-ad70-fa6f31ba219d");
    assert_eq!(t.ops.len(), 1);
    let lut = as_lut3d(&t.ops[0]);
    assert_eq!(lut.file_output_bd, BitDepth::UInt10);
    assert_eq!(lut.interpolation, Interpolation::Tetrahedral);
    assert_eq!(lut.grid_size, 3);
    assert_eq!(lut.num_components, 3);
    let v = &lut.values;
    assert_eq!(v.len(), 81);
    let scale = BitDepth::UInt10.max_value() as f32;
    assert_eq!(v[0] * scale, -60.0);
    assert_eq!(v[1] * scale, 5.0);
    assert_eq!(v[2] * scale, 75.0);
    assert_eq!(v[3] * scale, -10.0);
    check_close(v[4] * scale, 50.0, 1e-5);
    check_close(v[5] * scale, 400.0, 1e-4);
    assert_eq!(v[6] * scale, 0.0);
    check_close(v[7] * scale, 100.0, 1e-4);
    assert_eq!(v[8] * scale, 1200.0);
    assert_eq!(v[9] * scale, -40.0);
    assert_eq!(v[10] * scale, 500.0);
    assert_eq!(v[11] * scale, -30.0);
    assert_eq!(v[3 * 26] * scale, 1110.0);
    assert_eq!(v[3 * 26 + 1] * scale, 900.0);
    assert_eq!(v[3 * 26 + 2] * scale, 1200.0);
}

#[test]
fn matrix_windows_eol() {
    // This file uses windows end of line character and does not start with
    // the ?xml header.
    let t = load("clf/matrix_windows.clf").unwrap();
    assert_eq!(t.id(), "42");
    assert_eq!(t.ops.len(), 1);
    as_matrix(&t.ops[0]);
    assert_eq!(op_id(&t.ops[0]), "");
    assert_eq!(op_name(&t.ops[0]), "identity matrix");
}

#[test]
fn matrix_no_newlines() {
    let t = load("clf/matrix_no_newlines.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    let v = &m.matrix;
    let scale = 4095.0f32 / 1023.0;
    let c = |x: f64, e: f32| check_close(x as f32 * scale, e, 1e-6);
    c(v[0], 3.6);
    c(v[1], 0.1);
    c(v[2], -0.2);
    c(v[3], 0.0);
    c(v[4], 0.2);
    c(v[5], 3.5);
    c(v[6], 0.1);
    c(v[7], 0.0);
    c(v[8], 0.1);
    c(v[9], -0.3);
    c(v[10], 3.4);
    c(v[11], 0.0);
    let o = &m.offsets;
    let oscale = 4095.0f32;
    check_close(o[0] as f32 * oscale, 0.3, 1e-6);
    check_close(o[1] as f32 * oscale, -0.05, 1e-6);
    check_close(o[2] as f32 * oscale, -0.4, 1e-6);
    check_close(o[3] as f32 * oscale, 0.0, 1e-6);
}

#[test]
fn check_utf8() {
    let t = load("clf/matrix_example_utf8.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let d = op_descs(&t.ops[0]);
    assert_eq!(d.len(), 1);
    let utf8_test = "\u{6a19}\u{6e96}\u{842c}\u{570b}\u{78bc}";
    assert_eq!(
        d[0].as_bytes(),
        b"\xE6\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC"
    );
    assert_eq!(d[0], utf8_test);
    assert_ne!(
        d[0].as_bytes(),
        b"\xE5\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC"
    );
}

#[test]
fn smpte_id_element() {
    let t = load("clf/bit_depth_identity.clf").unwrap();
    assert_eq!(t.id(), "urn:uuid:9d768121-0cf9-40a3-a8e3-7b49f79858a7");
    let md = &t.metadata;
    assert_eq!(md.children.len(), 3);
    assert_eq!(md.children[0].element_name, "Id");
    assert_eq!(
        md.children[0].element_value,
        "urn:uuid:9d768121-0cf9-40a3-a8e3-7b49f79858a7"
    );
    assert_eq!(md.children[1].element_name, "Description");
    assert_eq!(
        md.children[1].element_value,
        "Identity transform illustrating Array bit depth scaling"
    );
    assert_eq!(md.children[2].element_name, "Description");
    assert_eq!(
        md.children[2].element_value,
        "Can be loaded by either SMPTE or CLF v3 parsers"
    );

    assert_eq!(t.ops.len(), 3);
    let m1 = as_matrix(&t.ops[0]);
    assert_eq!(m1.file_in_bd, BitDepth::UInt8);
    assert_eq!(m1.file_out_bd, BitDepth::UInt16);
    assert_eq!(as_lut1d(&t.ops[1]).file_output_bd, BitDepth::UInt16);
    let m2 = as_matrix(&t.ops[2]);
    assert_eq!(m2.file_in_bd, BitDepth::UInt16);
    assert_eq!(m2.file_out_bd, BitDepth::UInt16);
}

#[test]
#[ignore = "needs-merge"]
fn smpte_id_element_processor() {
    let config = crate::Config::create_raw();
    let ft = FileTransform {
        src: test_path("clf/bit_depth_identity.clf"),
        ..Default::default()
    };
    let processor = config
        .get_processor_for_transform(&Transform::File(ft), TransformDirection::Forward)
        .unwrap();
    let opt = processor.optimized_cpu_processor_with_bit_depths(
        BitDepth::UInt10,
        BitDepth::UInt12,
        crate::types::OptimizationFlags::DEFAULT,
    );
    assert!(opt.is_identity(), "{:?}", opt.ops());
    let meta = processor.format_metadata();
    assert_eq!(meta.children.len(), 3);
    assert_eq!(meta.children[0].element_name, "Id");
    assert_eq!(
        meta.children[0].element_value,
        "urn:uuid:9d768121-0cf9-40a3-a8e3-7b49f79858a7"
    );
}

#[test]
fn smpte_id_bad_value() {
    let (_t, warnings) = load_with_warnings("clf/smpte_only/illegal/id_bad_value.clf").unwrap();
    let expected = "id_bad_value.clf(3): '3bae2da8' is not a SMPTE ST 2136-1 compliant Id value.";
    assert!(
        warnings.iter().any(|w| w.contains(expected)),
        "{warnings:?}"
    );
}

#[test]
fn info_example() {
    let t = load("clf/info_example.clf").unwrap();
    let md = &t.metadata;
    assert_eq!(md.children.len(), 4);
    assert_eq!(md.children[0].element_name, "Description");
    assert_eq!(
        md.children[0].element_value,
        "Example of using the Info element"
    );
    assert_eq!(md.children[1].element_name, "Description");
    assert_eq!(md.children[1].element_value, "A second description");
    assert_eq!(md.children[2].element_name, "InputDescriptor");
    assert_eq!(md.children[2].element_value, "input desc");
    assert_eq!(md.children[3].element_name, "OutputDescriptor");
    assert_eq!(md.children[3].element_value, "output desc");

    // Ensure ops were not affected by metadata parsing.
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(op_name(&t.ops[0]), "identity");
    assert_eq!(m.file_in_bd, BitDepth::F32);
    assert_eq!(m.file_out_bd, BitDepth::UInt12);

    let info = &t.info_metadata;
    assert_eq!(info.element_name, METADATA_INFO);
    let items = &info.children;
    assert_eq!(items.len(), 6);
    assert_eq!(items[0].element_name, "Copyright");
    assert_eq!(
        items[0].element_value,
        "Copyright Contributors to the OpenColorIO Project."
    );
    assert_eq!(items[1].element_name, "AppRelease");
    assert_eq!(items[1].element_value, "2020.0.63");
    assert_eq!(items[2].element_name, "Revision");
    assert_eq!(items[2].element_value, "1");

    assert_eq!(items[3].element_name, "Category");
    assert_eq!(items[3].element_value, "");
    let cat = &items[3].children;
    assert_eq!(cat.len(), 1);
    assert_eq!(cat[0].element_name, "Tags");
    let tags = &cat[0].children;
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].element_name, "SceneLinearWorkingSpace");
    assert_eq!(tags[0].element_value, "");
    assert_eq!(tags[1].element_name, "Input");
    assert_eq!(tags[1].element_value, "");

    assert_eq!(items[4].element_name, "InputColorSpace");
    assert_eq!(items[4].element_value, "");
    let ic = &items[4].children;
    assert_eq!(ic.len(), 4);
    assert_eq!(ic[0].element_name, METADATA_DESCRIPTION);
    assert_eq!(ic[0].element_value, "Input color space description");
    assert_eq!(ic[1].element_name, "ImageState");
    assert_eq!(ic[1].element_value, "video");
    assert_eq!(ic[2].element_name, "ShortName");
    assert_eq!(ic[2].element_value, "no_version");
    assert_eq!(ic[3].element_name, "ID");
    assert_eq!(ic[3].element_value, "387b23d1-f1ce-3f69-8544-e5601f45f78b");

    assert_eq!(items[5].element_name, "OutputColorSpace");
    assert_eq!(items[5].element_value, "");
    let oc = &items[5].children;
    assert_eq!(oc.len(), 3);
    let attribs = &items[5].attributes;
    assert_eq!(attribs.len(), 2);
    assert_eq!(attribs[0], ("att1".to_string(), "test1".to_string()));
    assert_eq!(attribs[1], ("att2".to_string(), "test2".to_string()));
    assert_eq!(oc[0].element_name, "ImageState");
    assert_eq!(oc[0].element_value, "scene");
    assert_eq!(oc[1].element_name, "ShortName");
    assert_eq!(oc[1].element_value, "ACES");
    assert_eq!(oc[2].element_name, "ID");
    assert_eq!(oc[2].element_value, "1");
}

fn check_common_smpte_metadata(md: &FormatMetadata) {
    let items = &md.children;
    assert_eq!(items.len(), 9);
    let check = |i: usize, name: &str, value: &str, lang: Option<&str>| {
        assert_eq!(items[i].element_name, name);
        assert_eq!(items[i].element_value, value);
        if let Some(l) = lang {
            assert_eq!(items[i].attribute_value("language"), l);
        }
    };
    check(
        0,
        "Id",
        "urn:uuid:a8f91bfa-b79f-5d4d-b750-a411c476bb47",
        None,
    );
    check(
        1,
        "Description",
        "Demo Advanced LUT with dummy values",
        Some("en"),
    );
    check(
        2,
        "Description",
        "Démonstration d'une LUT avancée avec des valeurs factices",
        Some("fr"),
    );
    check(
        3,
        "Description",
        "Demo Erweiterte LUT mit Dummy-Werten",
        Some("de"),
    );
    check(4, "InputDescriptor", "ITU-R BT.709", Some("en-GB"));
    check(5, "OutputDescriptor", "Same as Input", Some("en-US"));
    check(6, "OutputDescriptor", "Identique à l'entrée", Some("fr"));
    check(7, "OutputDescriptor", "Gleiches wie Eingabe", Some("de"));
    check(8, "Info", "", None);

    // Info block (name spaces are retained).
    let info = &items[8].children;
    assert_eq!(info.len(), 9);
    let ci = |i: usize, name: &str, value: &str| {
        assert_eq!(info[i].element_name, name);
        assert_eq!(info[i].element_value, value);
    };
    ci(
        0,
        "Profile",
        "http://www.smpte-ra.org/ns/2136-10/2026#Live_Broadcast_LUT33",
    );
    ci(1, "AppRelease", "SMPTE_2136-10_Example");
    ci(2, "Copyright", "OCIO contributors");
    ci(3, "Revision", "1.0");
    ci(4, "clfbp:InputCharacteristics", "");
    ci(5, "clfbp:OutputCharacteristics", "");
    ci(6, "clfbp:OutputVideoSignalClipping", "sdiClip");
    ci(7, "Keywords", "Test, Display-light");
    ci(8, "clfbp:ContactEmail", "fake-email@ocio.org");

    let input = &info[4].children;
    assert_eq!(input.len(), 3);
    assert_eq!(input[0].element_name, "clfbp:ColorPrimaries");
    assert_eq!(input[0].element_value, "ColorPrimaries_ITU709");
    assert_eq!(input[1].element_name, "clfbp:TransferCharacteristic");
    assert_eq!(input[1].element_value, "TransferCharacteristic_ITU709");
    assert_eq!(input[2].element_name, "clfbp:CodingEquations");
    assert_eq!(input[2].element_value, "CodingEquations_ITU709");

    let output = &info[5].children;
    assert_eq!(output.len(), 3);
    assert_eq!(output[0].element_name, "clfbp:ColorPrimaries");
    assert_eq!(output[0].element_value, "ColorPrimaries_ITU2020");
    assert_eq!(output[1].element_name, "clfbp:TransferCharacteristic");
    assert_eq!(
        output[1].element_value,
        "TransferCharacteristic_SMPTEST2084"
    );
    assert_eq!(output[2].element_name, "clfbp:CodingEquations");
    assert_eq!(output[2].element_value, "CodingEquations_ITU2100_ICtCp");
}

/// Read a file with the CLF/CTF file format (like the group transform a
/// processor of a `FileTransform` would create from a CLF/CTF file).
pub(super) fn read_file_group(name: &str) -> GroupTransform {
    let p = test_path(name);
    let data = std::fs::read(&p).unwrap();
    super::create()
        .read(&data, &p, Interpolation::Default)
        .unwrap()
        .group
}

#[test]
fn smpte_all_metadata() {
    let group = read_file_group("clf/smpte_only/broadcast_profile_lut33.clf");
    {
        let md1 = &group.metadata;
        assert_eq!(md1.attributes.len(), 2);
        assert_eq!(md1.attributes[0].0, "name");
        assert_eq!(
            md1.attributes[0].1,
            "SMPTE Example Live Broadcast LUT33 Profile"
        );
        assert_eq!(md1.attributes[1].0, "xmlns:clfbp");
        assert_eq!(
            md1.attributes[1].1,
            "http://www.smpte-ra.org/ns/2136-10/2026"
        );
        check_common_smpte_metadata(md1);
    }
    // Write, read back and check if metadata survives the roundtrip.
    {
        let config = crate::Config::create_raw();
        let s = group
            .write(&config, "Academy/ASC Common LUT Format")
            .unwrap();
        let t = parse(&s).unwrap();
        let mut md2 = FormatMetadata::default();
        t.to_metadata(&mut md2);
        // Root attributes (will be different as id is added). The generated
        // id differs from OCIO's (different hash of the op list).
        assert_eq!(md2.attributes.len(), 3);
        assert_eq!(md2.attributes[0].0, "id");
        assert!(md2.attributes[0].1.starts_with("urn:uuid:"));
        assert!(super::reader::validate_smpte_id(&md2.attributes[0].1));
        assert_eq!(md2.attributes[1].0, "name");
        assert_eq!(
            md2.attributes[1].1,
            "SMPTE Example Live Broadcast LUT33 Profile"
        );
        assert_eq!(md2.attributes[2].0, "xmlns:clfbp");
        assert_eq!(
            md2.attributes[2].1,
            "http://www.smpte-ra.org/ns/2136-10/2026"
        );
        check_common_smpte_metadata(&md2);
    }
}

#[test]
fn smpte_namespaces() {
    let name = "clf/smpte_only/namespaces.clf";
    let t = load(name).unwrap();
    assert_eq!(t.ops.len(), 1);
    as_lut1d(&t.ops[0]);

    let group = read_file_group(name);
    let md = &group.metadata;
    assert_eq!(md.attributes.len(), 3);
    assert_eq!(md.attributes[0], ("id".to_string(), "pl1".to_string()));
    assert_eq!(
        md.attributes[1],
        (
            "xmlns:clf".to_string(),
            "http://www.smpte-ra.org/ns/2136-1/2024".to_string()
        )
    );
    assert_eq!(
        md.attributes[2],
        (
            "xmlns:ds".to_string(),
            "http://www.w3.org/2000/09/xmldsig#".to_string()
        )
    );
    // Name-spaced description will be available without the namespace prefix.
    assert_eq!(md.children.len(), 1);
    assert_eq!(md.children[0].element_name, "Description");
    assert_eq!(
        md.children[0].element_value,
        "Example CLF file using namespaces."
    );
}

#[test]
fn difficult_syntax() {
    // This file contains a lot of unusual (but still legal) ways of writing
    // the XML.
    let (t, warnings) = load_with_warnings("clf/difficult_syntax.clf").unwrap();
    let expected = "difficult_syntax.clf(41): Unrecognized attribute 'unknown' of 'LUT1D'.";
    assert!(
        warnings.iter().any(|w| w.contains(expected)),
        "{warnings:?}"
    );

    let ver = CtfVersion::parse(
        "http://www.smpte-ra.org/ns/2136-1/2024",
        version_format::SMPTE_XMLNS,
    )
    .unwrap();
    assert_eq!(t.clf_version, ver);
    assert_eq!(t.id(), "id1");

    let md = &t.metadata;
    assert_eq!(md.children.len(), 2);
    assert_eq!(md.children[0].element_name, "Description");
    assert_eq!(
        md.children[0].element_value,
        "This is the ProcessList description."
    );
    assert_eq!(md.children[1].element_name, "Description");
    assert_eq!(md.children[1].element_value, "yet 'another' \"valid\" desc");

    let info = &t.info_metadata;
    assert_eq!(info.element_name, METADATA_INFO);
    assert_eq!(info.children.len(), 1);
    assert_eq!(info.children[0].element_name, "Stuff");
    assert_eq!(
        info.children[0].element_value,
        "This is a \"difficult\" but 'legal' color transform file."
    );

    assert_eq!(t.ops.len(), 2);
    {
        let m = as_matrix(&t.ops[0]);
        assert_eq!(op_id(&t.ops[0]), "'mat-25'");
        assert_eq!(op_name(&t.ops[0]), "\"quote\"");
        assert_eq!(
            op_descs(&t.ops[0]),
            vec!["third array dim value is ignored"]
        );
        let mut v = M_3X3;
        v[10] = 0.105730e+1;
        check_matrix(m, v, [0.0; 4]);
    }
    {
        let lut = as_lut1d(&t.ops[1]);
        assert_eq!(op_name(&t.ops[1]), "a multi-line  name");
        let d = op_descs(&t.ops[1]);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0], "the n\u{2013}dash description");
        assert_eq!(d[1], "another valid description element    ");
        assert_eq!(d[2], "& another <valid> desc");
        assert_eq!(lut.length, 128);
        assert_eq!(lut.num_components, 3);
        let v = &lut.values;
        assert_eq!(v.len(), 384);
        assert_eq!(v[0], 0.0);
        assert_eq!(v[1], 0.0);
        assert_eq!(v[2], 0.0);
        assert_eq!(v[3], 0.06780);
        assert_eq!(v[4], 0.06441);
        assert_eq!(v[5], 0.06102);
        assert_eq!(v[6], 0.09965);
        assert_eq!(v[378], 0.99562);
        assert_eq!(v[379], 0.94584);
        assert_eq!(v[380], 0.89606);
    }
}

#[test]
fn difficult_xml_unknown_elements() {
    let expected = [
        "(10): Unrecognized element 'Ignore' where its parent is 'ProcessList' (8): Unknown element",
        "(22): Unrecognized attribute 'id' of 'Array'",
        "(22): Unrecognized attribute 'foo' of 'Array'",
        "(27): Unrecognized element 'ProcessList' where its parent is 'ProcessList' (8): The Transform already exists",
        "(30): Unrecognized element 'Array' where its parent is 'Matrix' (16): Only one Array allowed per op",
        "(37): Unrecognized element 'just_ignore' where its parent is 'ProcessList' (8): Unknown element",
        "(69): Unrecognized element 'just_ignore' where its parent is 'Description' (66)",
        "(70): Unrecognized element 'just_ignore' where its parent is 'just_ignore' (69)",
        "(75): Unrecognized element 'Matrix' where its parent is 'LUT1D' (",
        "(76): Unrecognized element 'Description' where its parent is 'Matrix' (75)",
        "(77): Unrecognized element 'Array' where its parent is 'Matrix' (75)",
    ];
    let (t, warnings) = load_with_warnings("difficult_test1_v1.ctf").unwrap();
    assert_eq!(warnings.len(), 11, "{warnings:?}");
    for (w, e) in warnings.iter().zip(expected.iter()) {
        assert!(w.contains(e), "{w:?} does not contain {e:?}");
    }

    // Defaults to 1.2.
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_2);
    assert_eq!(t.ops.len(), 2);
    let mut v = M_3X3;
    v[10] = 0.105730e+1;
    check_matrix(as_matrix(&t.ops[0]), v, [0.0; 4]);

    let lut = as_lut1d(&t.ops[1]);
    assert_eq!(lut.length, 17);
    assert_eq!(lut.num_components, 3);
    let v = &lut.values;
    assert_eq!(v.len(), 51);
    assert_eq!(v[0], 0.0);
    assert_eq!(v[1], 0.0);
    assert_eq!(v[2], 0.0);
    assert_eq!(v[3], 0.28358);
    assert_eq!(v[4], 0.28358);
    assert_eq!(v[5], 0.28358);
    assert_eq!(v[6], 0.38860);
    assert_eq!(v[45], 0.97109);
    assert_eq!(v[46], 0.97109);
    assert_eq!(v[47], 0.97109);
}

#[test]
fn unknown_elements() {
    let expected = [
        "(34): Unrecognized element 'B' where its parent is 'ProcessList' (2): Unknown element",
        "(34): Unrecognized element 'C' where its parent is 'B' (34)",
        "(36): Unrecognized element 'A' where its parent is 'Description' (36)",
    ];
    // NB: This file has some added unknown elements A, B, and C as a test.
    let (t, warnings) = load_with_warnings("clf/illegal/unknown_elements.clf").unwrap();
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    for (w, e) in warnings.iter().zip(expected.iter()) {
        assert!(w.contains(e), "{w:?} does not contain {e:?}");
    }

    assert_eq!(t.ops.len(), 4);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(m.matrix[0], 3.24);
    assert_eq!(m.matrix[4], -0.96930);
    assert_eq!(m.matrix[10], 1.0573);

    let lut1 = as_lut1d(&t.ops[1]);
    assert_eq!(lut1.length, 17);
    assert_eq!(lut1.num_components, 3);
    assert_eq!(lut1.values.len(), 51);
    assert_eq!(lut1.values[3], 0.28358);
    assert_eq!(lut1.values[4], 0.28358);
    assert_eq!(lut1.values[5], 100.0);
    assert_eq!(lut1.values[50], 1.0);

    let lut2 = as_lut1d(&t.ops[2]);
    assert_eq!(lut2.file_output_bd, BitDepth::UInt10);
    assert_eq!(lut2.length, 32);
    assert_eq!(lut2.num_components, 1);
    let v = &lut2.values;
    assert_eq!(v.len(), 96);
    assert_eq!(v[0], 0.0);
    assert_eq!(v[1], 0.0);
    assert_eq!(v[2], 0.0);
    assert_eq!(v[3], 215.0f32 / 1023.0);
    assert_eq!(v[4], 215.0f32 / 1023.0);
    assert_eq!(v[5], 215.0f32 / 1023.0);
    assert_eq!(v[6], 294.0f32 / 1023.0);
    assert_eq!(v[92], 1008.0f32 / 1023.0);
    assert_eq!(v[93], 1023.0f32 / 1023.0);
    assert_eq!(v[94], 1023.0f32 / 1023.0);
    assert_eq!(v[95], 1023.0f32 / 1023.0);

    let lut3 = as_lut3d(&t.ops[3]);
    assert_eq!(lut3.file_output_bd, BitDepth::UInt10);
    assert_eq!(lut3.grid_size, 3);
    assert_eq!(lut3.num_components, 3);
    let v = &lut3.values;
    assert_eq!(v.len(), 81);
    assert_eq!(v[0], 0.0);
    assert_eq!(v[1], 30.0f32 / 1023.0);
    assert_eq!(v[2], 33.0f32 / 1023.0);
    assert_eq!(v[3], 0.0);
    assert_eq!(v[4], 0.0);
    assert_eq!(v[5], 133.0f32 / 1023.0);
    assert_eq!(v[78], 1023.0f32 / 1023.0);
    assert_eq!(v[79], 1023.0f32 / 1023.0);
    assert_eq!(v[80], 1023.0f32 / 1023.0);
}

#[test]
fn wrong_format() {
    check_load_err("logtolin_8to8.lut", "not a CTF/CLF file.");
}

#[test]
fn binary_file() {
    check_load_err("clf/illegal/image_png.clf", "is not a CTF/CLF file.");
}

#[test]
fn process_list_invalid_version() {
    check_load_err("process_list_invalid_version.ctf", "is not a valid version");
}

#[test]
fn clf_process_list_bad_version() {
    check_load_err(
        "clf/pre-smpte_only/illegal/process_list_bad_version.clf",
        "is not a valid version",
    );
}

#[test]
fn process_list_valid_version() {
    let t = load("process_list_valid_version.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_4);
}

#[test]
fn non_smpte_xmlns() {
    let t = load("clf/pre-smpte_only/process_list_v3_namespace.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(m.file_in_bd, BitDepth::F32);
    assert_eq!(m.file_out_bd, BitDepth::F32);
}

#[test]
fn process_list_higher_version() {
    check_load_err(
        "process_list_higher_version.ctf",
        "Unsupported transform file version",
    );
}

#[test]
fn clf_process_list_higher_version() {
    check_load_err(
        "clf/pre-smpte_only/illegal/process_list_higher_version.clf",
        "Unsupported transform file version",
    );
}

#[test]
fn process_list_version_revision() {
    let t = load("process_list_version_revision.ctf").unwrap();
    let ver = CtfVersion::new(1, 3, 10);
    assert_eq!(t.version, ver);
    assert!(CTF_PROCESS_LIST_VERSION_1_3 < t.version);
    assert!(t.version < CTF_PROCESS_LIST_VERSION_1_4);
}

#[test]
fn process_list_no_version() {
    let t = load("process_list_no_version.ctf").unwrap();
    assert_eq!(t.version, CTF_PROCESS_LIST_VERSION_1_2);
}

#[test]
fn smpte_conflicting_version() {
    check_load_err(
        "process_list_conflicting_versions.ctf",
        "SMPTE 'xmlns' version and 'Version' attribute cannot both be present.",
    );
}

#[test]
fn smpte_higher_ns_version() {
    check_load_err(
        "clf/smpte_only/illegal/process_list_higher_ns_version.clf",
        "No valid 'version', 'compCLFversion', or 'xmlns' attributes were found; at least one of them is required.",
    );
}

#[test]
fn info_element_version_test() {
    // VALID - No Version.
    load("info_version_without.ctf").unwrap();
    // VALID - Minor Version.
    load("info_version_valid_minor.ctf").unwrap();
    // INVALID - Invalid Version.
    check_load_err(
        "info_version_invalid.ctf",
        "Invalid Info element version attribute",
    );
    // INVALID - Unsupported Version.
    check_load_err(
        "info_version_unsupported.ctf",
        "Unsupported Info element version attribute",
    );
    // INVALID - Empty Version.
    check_load_err(
        "info_version_empty.ctf",
        "Invalid Info element version attribute",
    );
}

#[test]
fn process_list_missing() {
    check_load_err(
        "clf/illegal/process_list_missing.clf",
        "is not a CTF/CLF file.",
    );
}

#[test]
fn transform_missing() {
    check_load_err(
        "clf/illegal/transform_missing.clf",
        "is not a CTF/CLF file.",
    );
}

#[test]
fn transform_element_end_missing() {
    check_load_err(
        "clf/illegal/transform_element_end_missing.clf",
        "no element found",
    );
}

#[test]
fn transform_missing_id() {
    check_load_err(
        "clf/pre-smpte_only/illegal/transform_missing_id.clf",
        "Required attribute 'id'",
    );
}

#[test]
fn transform_missing_inbitdepth() {
    check_load_err(
        "clf/illegal/transform_missing_inbitdepth.clf",
        "inBitDepth is missing",
    );
}

#[test]
fn transform_missing_outbitdepth() {
    check_load_err(
        "clf/illegal/transform_missing_outbitdepth.clf",
        "outBitDepth is missing",
    );
}

#[test]
fn array_missing_values() {
    check_load_err(
        "clf/illegal/array_missing_values.clf",
        "Expected 3x3 Array values",
    );
}

#[test]
fn array_bad_value() {
    check_load_err("clf/illegal/array_bad_value.clf", "Illegal values");
}

#[test]
fn array_bad_dimension() {
    check_load_err(
        "clf/illegal/array_bad_dimension.clf",
        "Illegal array dimensions",
    );
}

#[test]
fn array_too_many_values() {
    check_load_err(
        "clf/illegal/array_too_many_values.clf",
        "Expected 3x3 Array, found too many values",
    );
}

#[test]
fn matrix_end_missing() {
    check_load_err(
        "clf/illegal/matrix_end_missing.clf",
        "no closing tag for 'Matrix'",
    );
}

#[test]
fn transform_bad_outdepth() {
    check_load_err(
        "clf/illegal/transform_bad_outdepth.clf",
        "outBitDepth unknown value",
    );
}

#[test]
fn transform_end_missing() {
    check_load_err(
        "clf/illegal/transform_element_end_missing.clf",
        "no element found",
    );
}

#[test]
fn transform_corrupted_tag() {
    check_load_err("clf/illegal/transform_corrupted_tag.clf", "no closing tag");
}

#[test]
fn transform_empty() {
    check_load_err("clf/illegal/transform_empty.clf", "No color operator");
}

#[test]
fn transform_id_empty() {
    check_load_err(
        "clf/pre-smpte_only/illegal/transform_id_empty.clf",
        "Attribute 'id' does not have a value",
    );
}

#[test]
fn transform_with_bitdepth_mismatch() {
    // Any mismatches in the file are an indication of improper/unreliable
    // formatting and an exception should be thrown.
    check_load_err(
        "clf/illegal/transform_bitdepth_mismatch.clf",
        "Bit-depth mismatch",
    );
}

#[test]
fn inverse_of_id_test() {
    let t = load("clf/inverseOf_id_test.clf").unwrap();
    assert_eq!(t.metadata.attribute_value("inverseOf"), "inverseOfIdTest");
}

#[test]
fn range_default() {
    // If style is not present, it defaults to clamp.
    let t = load("clf/range.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_range(&t.ops[0]);
    assert_eq!(r.file_in_bd, BitDepth::UInt16);
    assert_eq!(r.file_out_bd, BitDepth::UInt16);
    // NB: All exactly representable as float.
    assert_eq!(r.min_in, 16320. / 65535.);
    assert_eq!(r.max_in, 32640. / 65535.);
    assert_eq!(r.min_out, 16320. / 65535.);
    assert_eq!(r.max_out, 32640. / 65535.);
    assert!(!r.min_is_empty());
    assert!(!r.max_is_empty());
}

#[test]
fn range_test1_clamp() {
    let t = load("clf/range_test1_clamp.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_range(&t.ops[0]);
    assert_eq!(r.file_in_bd, BitDepth::UInt8);
    assert_eq!(r.file_out_bd, BitDepth::F32);
    assert_eq!(r.min_in, 16. / 255.);
    assert_eq!(r.max_in, 240. / 255.);
    assert_eq!(r.min_out, -0.5);
    assert_eq!(r.max_out, 2.);
    assert!(!r.min_is_empty());
    assert!(!r.max_is_empty());
}

pub(super) fn matrix_is_diagonal(m: &MatrixData) -> bool {
    (0..4).all(|r| (0..4).all(|c| r == c || m.matrix[r * 4 + c] == 0.0))
}

#[test]
fn range_test1_noclamp() {
    let t = load("clf/range_test1_noclamp.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    // Check that the noClamp style Range became a Matrix.
    let m = as_matrix(&t.ops[0]);
    assert_eq!(m.file_in_bd, BitDepth::UInt8);
    assert_eq!(m.file_out_bd, BitDepth::F32);

    let out_scale = BitDepth::F32.max_value();
    let mat_scale = out_scale / BitDepth::UInt8.max_value();
    let scalef = (1.05f32 + 0.05) / (272.0 + 16.0);
    let offsetf = -0.05f32 - scalef * -16.0;
    let prec = 10000.0f32;
    let scale = (prec * scalef) as i32;
    let offset = (prec * offsetf) as i32;

    assert!(matrix_is_diagonal(m));
    let p = f64::from(prec);
    assert_eq!((p * m.matrix[0] * mat_scale) as i32, scale);
    assert_eq!((p * m.matrix[5] * mat_scale) as i32, scale);
    assert_eq!((p * m.matrix[10] * mat_scale) as i32, scale);
    assert_eq!(m.matrix[15], 1.0);
    assert_eq!((p * m.offsets[0] * out_scale) as i32, offset);
    assert_eq!((p * m.offsets[1] * out_scale) as i32, offset);
    assert_eq!((p * m.offsets[2] * out_scale) as i32, offset);
    assert_eq!(m.offsets[3], 0.0);
}

#[test]
fn range_test2() {
    let t = load("clf/range_test2.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_range(&t.ops[0]);
    assert_eq!(r.file_in_bd, BitDepth::F32);
    assert_eq!(r.file_out_bd, BitDepth::F16);
    assert_eq!(r.min_in, 0.1);
    assert_eq!(r.min_out, 0.1);
    assert!(r.max_is_empty());
}

#[test]
fn range_nonmatching_clamp() {
    check_load_err(
        "clf/illegal/range_nonmatching_clamp.clf",
        "In and out minimum limits must be equal",
    );
}

#[test]
fn range_empty() {
    check_load_err(
        "clf/illegal/range_empty.clf",
        "At least minimum or maximum limits must be set",
    );
}

#[test]
fn range_bad_noclamp() {
    check_load_err(
        "clf/illegal/range_bad_noclamp.clf",
        "Non-clamping Range min & max values have to be set",
    );
}

#[test]
fn range_bad_values() {
    check_load_err(
        "clf/illegal/range_bad_values.clf",
        "Range maxInValue is too close to minInValue",
    );
}

#[test]
fn index_map_test() {
    check_load_err(
        "indexMap_test.ctf",
        "Only two entry IndexMaps are supported",
    );
}

#[test]
fn index_map_test1_clfv2() {
    // IndexMaps were allowed in CLF v2 (were removed in v3).
    let t = load("indexMap_test1_clfv2.clf").unwrap();
    assert_eq!(t.ops.len(), 2);
    // Check that the indexMap caused a Range to be inserted.
    let r = as_range(&t.ops[0]);
    assert_eq!(r.min_in * 1023., 64.5);
    assert_eq!(r.max_in * 1023., 940.);
    assert_eq!(r.min_out * 1023.0, 132.0); // 4*1023/31
    assert_eq!(r.max_out * 1023.0, 1089.0); // 33*1023/31
    assert_eq!(r.file_in_bd, BitDepth::UInt10);
    assert_eq!(r.file_out_bd, BitDepth::UInt10);
    // Check the LUT is ok.
    let l = as_lut1d(&t.ops[1]);
    assert_eq!(l.length, 32);
    assert_eq!(l.file_output_bd, BitDepth::UInt12);
}

#[test]
fn index_map_test2_clfv2() {
    let t = load("indexMap_test2_clfv2.clf").unwrap();
    assert_eq!(t.ops.len(), 2);
    let r = as_range(&t.ops[0]);
    assert_eq!(r.min_in, f64::from(-0.1f32));
    assert_eq!(r.max_in, 19.0);
    assert_eq!(r.min_out, 0.0);
    assert_eq!(r.max_out, 1.0);
    assert_eq!(r.file_in_bd, BitDepth::F32);
    assert_eq!(r.file_out_bd, BitDepth::F32);
    let l = as_lut3d(&t.ops[1]);
    assert_eq!(l.grid_size, 2);
    assert_eq!(l.file_output_bd, BitDepth::UInt10);
}

#[test]
fn clf3_index_map() {
    // Same as previous, but setting compCLFversion=3.0.
    let (_t, warnings) = load_with_warnings("clf/illegal/indexMap_test2.clf").unwrap();
    let expected = "Element 'IndexMap' is not valid since CLF 3 (or CTF 2)";
    assert!(
        warnings.iter().any(|w| w.contains(expected)),
        "{warnings:?}"
    );
}

#[test]
fn index_map_test3() {
    check_load_err("indexMap_test3.ctf", "Only one IndexMap allowed per LUT");
}

#[test]
fn index_map_test4_clfv2() {
    check_load_err(
        "indexMap_test4_clfv2.clf",
        "Only two entry IndexMaps are supported",
    );
}

pub(super) fn gamma_all_components_equal(g: &GammaData) -> bool {
    g.params[0] == g.params[1] && g.params[0] == g.params[2] && g.params[0] == g.params[3]
}

pub(super) fn gamma_is_identity(g: &GammaData) -> bool {
    gamma_all_components_equal(g) && g.is_identity_params(&g.params[0])
}

#[test]
fn gamma_test1() {
    let t = load("gamma_test1.ctf").unwrap();
    assert_eq!(t.id(), "id");
    let md = &t.metadata;
    assert_eq!(md.children.len(), 1);
    assert_eq!(md.children[0].element_name, "Description");
    assert_eq!(md.children[0].element_value, "2.4 gamma");
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::BasicFwd);
    let p = vec![2.4];
    assert_eq!(g.params[0], p);
    assert_eq!(g.params[1], p);
    assert_eq!(g.params[2], p);
    // Version of the ctf is less than 1.5, so alpha must be identity.
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(g.is_non_channel_dependent());
}

#[test]
fn gamma_test2() {
    let t = load("gamma_test2.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::BasicRev);
    assert_eq!(g.params[0], vec![2.4]);
    assert_eq!(g.params[1], vec![2.35]);
    assert_eq!(g.params[2], vec![2.2]);
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(!g.is_non_channel_dependent());
}

#[test]
fn gamma_test3() {
    let t = load("gamma_test3.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveFwd);
    // Precision test: a double exactly equal to 1/0.45 is required to
    // implement rec 709 exactly.
    let p = vec![1. / 0.45, 0.099];
    assert_eq!(g.params[0], p);
    assert_eq!(g.params[1], p);
    assert_eq!(g.params[2], p);
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(g.is_non_channel_dependent());
}

#[test]
fn gamma_test4() {
    let t = load("gamma_test4.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveRev);
    assert_eq!(g.params[0], vec![2.2, 0.001]);
    assert_eq!(g.params[1], vec![2.4, 0.01]);
    assert_eq!(g.params[2], vec![2.6, 0.1]);
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(!g.is_non_channel_dependent());
}

#[test]
fn gamma_test5() {
    // An old (< 1.5) transform file that contains an invalid GammaParams
    // for the A channel.
    check_load_err("gamma_test5.ctf", "Invalid channel");
}

#[test]
fn gamma_test6() {
    // An old (< 1.5) transform file with a single GammaParams with identity
    // values.
    let t = load("gamma_test6.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveFwd);
    assert!(gamma_all_components_equal(g));
    assert!(g.is_non_channel_dependent());
    assert!(gamma_is_identity(g));
}

#[test]
fn gamma_alpha_test1() {
    let t = load("gamma_alpha_test1.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::BasicFwd);
    let p = vec![2.4];
    assert_eq!(g.params[0], p);
    assert_eq!(g.params[1], p);
    assert_eq!(g.params[2], p);
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(g.is_non_channel_dependent());
}

#[test]
fn gamma_alpha_test2() {
    let t = load("gamma_alpha_test2.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::BasicRev);
    assert_eq!(g.params[0], vec![2.4]);
    assert_eq!(g.params[1], vec![2.35]);
    assert_eq!(g.params[2], vec![2.2]);
    assert_eq!(g.params[3], vec![2.5]);
    assert!(!gamma_all_components_equal(g));
    assert!(!g.is_non_channel_dependent());
}

#[test]
fn gamma_alpha_test3() {
    let t = load("gamma_alpha_test3.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveFwd);
    let p = vec![1. / 0.45, 0.099];
    assert_eq!(g.params[0], p);
    assert_eq!(g.params[1], p);
    assert_eq!(g.params[2], p);
    assert!(g.is_alpha_identity());
    assert!(!gamma_all_components_equal(g));
    assert!(g.is_non_channel_dependent());
}

#[test]
fn gamma_alpha_test4() {
    let t = load("gamma_alpha_test4.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveRev);
    assert_eq!(g.params[0], vec![2.2, 0.001]);
    assert_eq!(g.params[1], vec![2.4, 0.01]);
    assert_eq!(g.params[2], vec![2.6, 0.1]);
    assert_eq!(g.params[3], vec![2.0, 0.0001]);
    assert!(!gamma_all_components_equal(g));
    assert!(!g.is_non_channel_dependent());
}

#[test]
fn gamma_alpha_test5() {
    let t = load("gamma_alpha_test5.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_gamma(&t.ops[0]);
    assert_eq!(g.style, GammaStyle::MonCurveFwd);
    let p = vec![1. / 0.45, 0.099];
    assert_eq!(g.params[0], p);
    assert_eq!(g.params[1], p);
    assert_eq!(g.params[2], p);
    assert_eq!(g.params[3], vec![1.7, 0.33]);
    assert!(!gamma_all_components_equal(g));
    assert!(!g.is_non_channel_dependent());
}

#[test]
fn gamma_alpha_test6() {
    // An invalid GammaParams for the A channel (missing offset attribute).
    check_load_err("gamma_alpha_test6.ctf", "Missing required offset parameter");
}

#[test]
fn exponent_bad_value() {
    // The moncurve style requires a gamma value >= 1.
    check_load_err(
        "clf/illegal/exponent_bad_value.clf",
        "is less than lower bound",
    );
}

#[test]
fn exponent_bad_param() {
    // The basic style cannot use offset.
    check_load_err(
        "clf/illegal/exponent_bad_param.clf",
        "Illegal offset parameter",
    );
}

#[test]
fn exponent_all_styles() {
    let t = load("clf/exponent_all_styles.clf").unwrap();
    assert_eq!(t.ops.len(), 12);
    {
        // Op 0 == basicFwd.
        let g = as_gamma(&t.ops[0]);
        assert_eq!(
            op_descs(&t.ops[0]),
            vec!["If there is only one Params, use it for R, G, and B."]
        );
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::BasicFwd);
        assert!(g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
        assert_eq!(g.params[0], vec![2.4]);
    }
    {
        // Op 1 == basicRev.
        let g = as_gamma(&t.ops[1]);
        assert_eq!(op_id(&t.ops[1]), "a1");
        assert_eq!(op_name(&t.ops[1]), "gamma");
        assert_eq!(g.style.direction(), TransformDirection::Inverse);
        assert_eq!(g.style, GammaStyle::BasicRev);
        assert!(!g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
        assert_eq!(g.params[0], vec![2.4]);
        assert_eq!(g.params[1], vec![2.35]);
        assert_eq!(g.params[2], vec![2.2]);
    }
    {
        // Op 2 == monCurveFwd.
        let g = as_gamma(&t.ops[2]);
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::MonCurveFwd);
        assert!(g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
        assert_eq!(g.params[0], vec![1. / 0.45, 0.099]);
    }
    {
        // Op 3 == monCurveRev.
        let g = as_gamma(&t.ops[3]);
        assert_eq!(g.style.direction(), TransformDirection::Inverse);
        assert_eq!(g.style, GammaStyle::MonCurveRev);
        assert!(!g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
        assert_eq!(g.params[0], vec![2.2, 0.001]);
        assert_eq!(g.params[1], vec![2.4, 0.01]);
        assert_eq!(g.params[2], vec![2.6, 0.1]);
    }
    {
        // Op 4 == monCurveFwd.
        let g = as_gamma(&t.ops[4]);
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::MonCurveFwd);
        assert!(gamma_all_components_equal(g));
        assert!(g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
        assert!(g.is_identity_params(&g.params[0]));
    }
    {
        // Op 5 == basicMirrorFwd.
        let g = as_gamma(&t.ops[5]);
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::BasicMirrorFwd);
        assert!(!gamma_all_components_equal(g));
        assert!(g.is_non_channel_dependent());
        assert!(g.is_alpha_identity());
    }
    {
        // Op 6 == basicMirrorRev.
        let g = as_gamma(&t.ops[6]);
        assert_eq!(g.style.direction(), TransformDirection::Inverse);
        assert_eq!(g.style, GammaStyle::BasicMirrorRev);
        assert!(g.is_non_channel_dependent());
    }
    {
        // Op 7 == basicPassThruFwd.
        let g = as_gamma(&t.ops[7]);
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::BasicPassThruFwd);
        assert!(g.is_non_channel_dependent());
    }
    {
        // Op 8 == basicPassThruRev.
        let g = as_gamma(&t.ops[8]);
        assert_eq!(g.style.direction(), TransformDirection::Inverse);
        assert_eq!(g.style, GammaStyle::BasicPassThruRev);
        assert!(g.is_non_channel_dependent());
    }
    {
        // Op 9 == monCurveMirrorFwd.
        let g = as_gamma(&t.ops[9]);
        assert_eq!(g.style.direction(), TransformDirection::Forward);
        assert_eq!(g.style, GammaStyle::MonCurveMirrorFwd);
        assert!(g.is_non_channel_dependent());
    }
    {
        // Op 10 == monCurveMirrorRev.
        let g = as_gamma(&t.ops[10]);
        assert_eq!(g.style.direction(), TransformDirection::Inverse);
        assert_eq!(g.style, GammaStyle::MonCurveMirrorRev);
        assert!(!g.is_non_channel_dependent());
        assert_eq!(g.params[0], vec![3.0, 0.16]);
        assert!(g.is_identity_params(&g.params[1]));
        assert!(g.is_identity_params(&g.params[2]));
    }
    // Op 11 == Range.
    as_range(&t.ops[11]);
}

#[test]
fn clf2_exponent_parse() {
    let gamma_clf2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams gamma="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_err(
        parse(gamma_clf2),
        "CLF file version '2' does not support operator 'Exponent'",
    );

    let gamma_clf_alpha = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams gamma="2.6" />
        <ExponentParams channel="A" gamma="1.7" offset="0.33" />
    </Exponent>
</ProcessList>
"#;
    check_err(parse(gamma_clf_alpha), "Invalid channel: A");

    let gamma_ctf_mirror_1_7 = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.7" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams gamma="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_err(
        parse(gamma_ctf_mirror_1_7),
        "Style not handled: 'basicMirrorRev'",
    );
}

#[test]
fn cdl_clamp_fwd() {
    let t = load("clf/cdl_clamp_fwd.clf").unwrap();
    assert_eq!(
        descs(&t.metadata, METADATA_INPUT_DESCRIPTOR),
        vec!["inputDesc"]
    );
    assert_eq!(
        descs(&t.metadata, METADATA_OUTPUT_DESCRIPTOR),
        vec!["outputDesc"]
    );
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    assert_eq!(op_id(&t.ops[0]), "look 1");
    assert_eq!(op_name(&t.ops[0]), "cdl");
    assert_eq!(op_descs(&t.ops[0]), vec!["ASC CDL operation"]);
    assert_eq!(c.style, CdlOpStyle::V12Fwd);
    assert_eq!(c.style.name(), "Fwd");
    assert_eq!(c.slope, [1.35, 1.1, 0.71]);
    assert_eq!(c.offset, [0.05, -0.23, 0.11]);
    assert_eq!(c.power, [0.93, 0.81, 1.27]);
    assert_eq!(c.sat, 1.239);
}

#[test]
fn cdl_missing_style() {
    let t = load("clf/cdl_missing_style.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    // Note: Default for CLF is different from OCIO default.
    assert_eq!(c.style, CdlOpStyle::V12Fwd);
    assert_eq!(c.slope, [1.35, 1.1, 0.71]);
    assert_eq!(c.offset, [0.05, -0.23, 0.11]);
    assert_eq!(c.power, [0.93, 0.81, 1.27]);
    assert_eq!(c.sat, 1.239);
}

#[test]
fn cdl_all_styles() {
    let t = load("clf/cdl_all_styles.clf").unwrap();
    assert_eq!(t.ops.len(), 4);
    assert_eq!(as_cdl(&t.ops[0]).style, CdlOpStyle::V12Fwd);
    assert_eq!(as_cdl(&t.ops[1]).style, CdlOpStyle::V12Rev);
    assert_eq!(as_cdl(&t.ops[2]).style, CdlOpStyle::NoClampFwd);
    assert_eq!(as_cdl(&t.ops[3]).style, CdlOpStyle::NoClampRev);
}

#[test]
fn cdl_bad_slope() {
    check_load_err(
        "clf/illegal/cdl_bad_slope.clf",
        "SOPNode: 3 values required",
    );
}

#[test]
fn cdl_bad_sat() {
    check_load_err("clf/illegal/cdl_bad_sat.clf", "SatNode: non-single value");
}

#[test]
fn cdl_bad_power() {
    check_load_err(
        "clf/illegal/cdl_bad_power.clf",
        "CDLOpData: Invalid 'power' 0 should be greater than 0.",
    );
}

#[test]
fn cdl_missing_slope() {
    check_load_err(
        "clf/illegal/cdl_missing_slope.clf",
        "Required node 'Slope' is missing",
    );
}

#[test]
fn cdl_missing_offset() {
    check_load_err(
        "clf/illegal/cdl_missing_offset.clf",
        "Required node 'Offset' is missing",
    );
}

#[test]
fn cdl_missing_power() {
    check_load_err(
        "clf/illegal/cdl_missing_power.clf",
        "Required node 'Power' is missing",
    );
}

#[test]
fn cdl_bad_style() {
    check_load_err("clf/illegal/cdl_bad_style.clf", "Unknown style for CDL");
}

#[test]
fn cdl_missing_sop() {
    let t = load("clf/cdl_missing_sop.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    assert_eq!(c.slope, [1.0; 3]);
    assert_eq!(c.offset, [0.0; 3]);
    assert_eq!(c.power, [1.0; 3]);
    assert_eq!(c.sat, 1.239);
}

#[test]
fn cdl_missing_sat() {
    let t = load("clf/cdl_missing_sat.clf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    assert_eq!(c.slope, [1.35, 1.1, 0.71]);
    assert_eq!(c.offset, [0.05, -0.23, 0.11]);
    assert_eq!(c.power, [0.93, 0.81, 1.27]);
    assert_eq!(c.sat, 1.0);
}

#[test]
fn cdl_various_in_ctf() {
    // When CDL was added to the CLF spec in v2, the style names were
    // changed. Test that both the new and old style names work.
    let t = load("cdl_various.ctf").unwrap();
    assert_eq!(t.ops.len(), 8);
    let expected = [
        CdlOpStyle::V12Fwd,
        CdlOpStyle::V12Fwd,
        CdlOpStyle::V12Rev,
        CdlOpStyle::V12Rev,
        CdlOpStyle::NoClampFwd,
        CdlOpStyle::NoClampFwd,
        CdlOpStyle::NoClampRev,
        CdlOpStyle::NoClampRev,
    ];
    for (op, s) in t.ops.iter().zip(expected) {
        assert_eq!(as_cdl(op).style, s);
    }
}

#[track_caller]
fn check_close64(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tolerance {tol})");
}

#[test]
fn log_all_styles() {
    let t = load("clf/log_all_styles.clf").unwrap();
    assert_eq!(t.ops.len(), 11);
    let e = 1e-9;
    {
        // Op 0 == antiLog2.
        let l = as_log(&t.ops[0]);
        assert_eq!(op_descs(&t.ops[0]), vec!["AntiLog2 logarithm operation"]);
        assert_eq!(l.dir, TransformDirection::Inverse);
        assert!(l.is_log2());
    }
    {
        // Op 1 == log2.
        let l = as_log(&t.ops[1]);
        assert_eq!(op_id(&t.ops[1]), "a1");
        assert_eq!(op_name(&t.ops[1]), "logarithm");
        assert_eq!(l.dir, TransformDirection::Forward);
        assert!(l.is_log2());
        assert!(!l.is_log10());
        assert!(!l.is_camera());
    }
    {
        // Op 2 == linToLog.
        let l = as_log(&t.ops[2]);
        assert_eq!(l.dir, TransformDirection::Forward);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(!l.is_camera());
        assert!(l.all_components_equal());
        let p = &l.params[0];
        assert_eq!(p.len(), 4);
        check_close64(p[LOG_SIDE_SLOPE], 0.29325513196, e);
        check_close64(p[LOG_SIDE_OFFSET], 0.66959921799, e);
        check_close64(p[LIN_SIDE_SLOPE], 0.98920224838, e);
        check_close64(p[LIN_SIDE_OFFSET], 0.01079775162, e);
        assert_eq!(l.base, 10.);
    }
    {
        // Op 3 == antiLog10.
        let l = as_log(&t.ops[3]);
        assert_eq!(l.dir, TransformDirection::Inverse);
        assert!(!l.is_log2());
        assert!(l.is_log10());
    }
    {
        // Op 4 == log10.
        let l = as_log(&t.ops[4]);
        assert_eq!(l.dir, TransformDirection::Forward);
        assert!(!l.is_log2());
        assert!(l.is_log10());
    }
    {
        // Op 5 == logToLin.
        let l = as_log(&t.ops[5]);
        assert_eq!(l.dir, TransformDirection::Inverse);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(!l.is_camera());
        assert!(l.all_components_equal());
        let p = &l.params[0];
        assert_eq!(p.len(), 4);
        check_close64(p[LOG_SIDE_SLOPE], 0.29325513196, e);
        check_close64(p[LOG_SIDE_OFFSET], 0.66959921799, e);
        check_close64(p[LIN_SIDE_SLOPE], 0.98920224838, e);
        check_close64(p[LIN_SIDE_OFFSET], 0.01079775162, e);
        assert_eq!(l.base, 10.);
    }
    {
        // Op 6 == cameraLinToLog.
        let l = as_log(&t.ops[6]);
        assert_eq!(l.dir, TransformDirection::Forward);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(l.is_camera());
        assert!(l.all_components_equal());
        let p = &l.params[0];
        assert_eq!(p.len(), 5);
        check_close64(p[LOG_SIDE_SLOPE], 0.05707762557, e);
        check_close64(p[LOG_SIDE_OFFSET], 0.55479452050, e);
        check_close64(p[LIN_SIDE_SLOPE], 1., e);
        check_close64(p[LIN_SIDE_OFFSET], 0., e);
        check_close64(p[LIN_SIDE_BREAK], 0.00781250000, e);
        // Default base value is 2.
        assert_eq!(l.base, 2.);
    }
    {
        // Op 7 == cameraLogToLin.
        let l = as_log(&t.ops[7]);
        assert_eq!(l.dir, TransformDirection::Inverse);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(l.is_camera());
        assert!(l.all_components_equal());
        let p = &l.params[0];
        assert_eq!(p.len(), 5);
        check_close64(p[LOG_SIDE_SLOPE], 0.05707762557, e);
        check_close64(p[LOG_SIDE_OFFSET], 0.55479452050, e);
        check_close64(p[LIN_SIDE_SLOPE], 1., e);
        check_close64(p[LIN_SIDE_OFFSET], 0., e);
        check_close64(p[LIN_SIDE_BREAK], 0.00781250000, e);
        assert_eq!(l.base, 2.);
    }
    {
        // Op 8 == cameraLogToLin.
        let l = as_log(&t.ops[8]);
        assert_eq!(l.dir, TransformDirection::Inverse);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(l.is_camera());
        assert!(l.all_components_equal());
        let p = &l.params[0];
        assert_eq!(p.len(), 6);
        check_close64(p[LOG_SIDE_SLOPE], 0.25562072336, e);
        check_close64(p[LOG_SIDE_OFFSET], 0.41055718475, e);
        check_close64(p[LIN_SIDE_SLOPE], 5.26315789474, e);
        check_close64(p[LIN_SIDE_OFFSET], 0.05263157895, e);
        check_close64(p[LIN_SIDE_BREAK], 0.01125000000, e);
        check_close64(p[LINEAR_SLOPE], 6.62194371178, e);
        assert_eq!(l.base, 10.);
    }
    {
        // Op 9 == linToLog.
        let l = as_log(&t.ops[9]);
        assert_eq!(l.dir, TransformDirection::Forward);
        assert!(!l.is_log2());
        assert!(!l.is_log10());
        assert!(!l.is_camera());
        assert!(!l.all_components_equal());
        let chk = |p: &Vec<f64>, v: [f64; 4]| {
            assert_eq!(p.len(), 4);
            assert_eq!(p[LOG_SIDE_SLOPE], v[0]);
            assert_eq!(p[LOG_SIDE_OFFSET], v[1]);
            assert_eq!(p[LIN_SIDE_SLOPE], v[2]);
            assert_eq!(p[LIN_SIDE_OFFSET], v[3]);
        };
        chk(&l.params[0], [0.3, 0.6, 0.9, 0.05]);
        chk(&l.params[1], [0.25, 0.4, 5.0, 0.05]);
        chk(&l.params[2], [0.28, 0.5, 2.0, 0.1]);
        assert_eq!(l.base, 8.);
    }
    // Op 10 == Range.
    as_range(&t.ops[10]);
}

#[track_caller]
fn check_log_logtolin(name: &str) {
    let t = load(name).unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_log(&t.ops[0]);
    assert_eq!(l.dir, TransformDirection::Inverse);
    assert!(!l.is_log2());
    assert!(!l.is_log10());
    assert!(l.all_components_equal());
    let p = &l.params[0];
    assert_eq!(p.len(), 4);
    let e = 1e-9;
    // This file uses the original CTF/Cineon style params, verify they are
    // converted properly to the new OCIO style params.
    check_close64(p[LOG_SIDE_SLOPE], 0.29325513196, e);
    check_close64(p[LOG_SIDE_OFFSET], 0.66959921799, e);
    check_close64(p[LIN_SIDE_SLOPE], 0.98969709693, e);
    check_close64(p[LIN_SIDE_OFFSET], 0.01030290307, e);
}

#[test]
fn log_logtolin() {
    check_log_logtolin("log_logtolin.ctf");
}

#[test]
fn log_logtolinv2() {
    // Same as previous test, but CTF version set to 2.
    check_log_logtolin("log_logtolinv2.ctf");
}

#[test]
fn log_lintolog_3chan() {
    let t = load("log_lintolog_3chan.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_log(&t.ops[0]);
    assert_eq!(l.dir, TransformDirection::Forward);
    assert!(!l.all_components_equal());
    let e = 1e-9;
    let chk = |p: &Vec<f64>, v: [f64; 4]| {
        assert_eq!(p.len(), 4);
        check_close64(p[LOG_SIDE_SLOPE], v[0], e);
        check_close64(p[LOG_SIDE_OFFSET], v[1], e);
        check_close64(p[LIN_SIDE_SLOPE], v[2], e);
        check_close64(p[LIN_SIDE_OFFSET], v[3], e);
    };
    chk(
        &l.params[0],
        [
            0.244379276637,
            0.665689149560,
            1.111637101285,
            -0.000473391157,
        ],
    );
    chk(
        &l.params[1],
        [
            0.293255131964,
            0.666666666667,
            0.991514003046,
            0.008485996954,
        ],
    );
    chk(
        &l.params[2],
        [
            0.317693059628,
            0.667644183773,
            1.236287104632,
            0.010970316295,
        ],
    );
}

#[test]
fn log_bad_style() {
    check_load_err("clf/illegal/log_bad_style.clf", "is invalid");
}

#[test]
fn log_bad_version() {
    check_load_err(
        "clf/pre-smpte_only/illegal/log_bad_version.clf",
        "CLF file version '2' does not support operator 'Log'",
    );
}

#[test]
fn log_bad_param() {
    check_load_err(
        "clf/illegal/log_bad_param.clf",
        "Parameter 'linSideBreak' is only allowed for style",
    );
}

#[test]
fn log_missing_breakpnt() {
    check_load_err(
        "clf/illegal/log_missing_breakpnt.clf",
        "Parameter 'linSideBreak' should be defined for style",
    );
}

#[test]
fn log_ocio_params_channels() {
    // NB: The blue channel is missing and will use default values. Base can
    // be specified in any channel but has to be specified.
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='2'>\n\
<Log inBitDepth='10i' outBitDepth='16f' style='linToLog'>\n\
<LogParams channel='R' linSideSlope='1.1' linSideOffset='0.1' logSideSlope='0.9' logSideOffset='0.2' base='10.0' />\n\
<LogParams channel='G' logSideSlope='0.9' logSideOffset='0.23456' />\n\
</Log>\n\
</ProcessList>\n";
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_log(&t.ops[0]);
    assert_eq!(l.base, 10.0);
    assert!(!l.all_components_equal());
    let r = &l.params[0];
    assert_eq!(r[LIN_SIDE_SLOPE], 1.1);
    assert_eq!(r[LIN_SIDE_OFFSET], 0.1);
    assert_eq!(r[LOG_SIDE_SLOPE], 0.9);
    assert_eq!(r[LOG_SIDE_OFFSET], 0.2);
    let g = &l.params[1];
    assert_eq!(g[LIN_SIDE_SLOPE], 1.0);
    assert_eq!(g[LIN_SIDE_OFFSET], 0.0);
    assert_eq!(g[LOG_SIDE_SLOPE], 0.9);
    assert_eq!(g[LOG_SIDE_OFFSET], 0.23456);
    let b = &l.params[2];
    assert_eq!(b[LIN_SIDE_SLOPE], 1.0);
    assert_eq!(b[LIN_SIDE_OFFSET], 0.0);
    assert_eq!(b[LOG_SIDE_SLOPE], 1.0);
    assert_eq!(b[LOG_SIDE_OFFSET], 0.0);
}

#[test]
fn log_ocio_params_base_missmatch() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='2'>\n\
<Log inBitDepth='32f' outBitDepth='32f' style='linToLog'>\n\
<LogParams channel='R' linSideSlope='1.1' base='2.0'/>\n\
<LogParams channel='G' linSideSlope='1.2' base='2.5'/>\n\
</Log>\n\
</ProcessList>\n";
    check_err(parse(s), "base has to be the same");
}

#[test]
fn log_default_params() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='2'>\n\
<Log inBitDepth='32f' outBitDepth='32f' style='linToLog' />\n\
<Log inBitDepth='32f' outBitDepth='32f' style='cameraLinToLog'>\n\
<LogParams linSideBreak='0.1'/>\n\
</Log>\n\
</ProcessList>\n";
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 2);
    let l = as_log(&t.ops[0]);
    // Default value is 2.
    assert_eq!(l.base, 2.0);
    let r = &l.params[0];
    assert_eq!(r.len(), 4);
    assert_eq!(r[LIN_SIDE_SLOPE], 1.);
    assert_eq!(r[LIN_SIDE_OFFSET], 0.);
    assert_eq!(r[LOG_SIDE_SLOPE], 1.);
    assert_eq!(r[LOG_SIDE_OFFSET], 0.);

    let l = as_log(&t.ops[1]);
    assert_eq!(l.base, 2.0);
    let g = &l.params[1];
    assert_eq!(g.len(), 5);
    assert_eq!(g[LIN_SIDE_SLOPE], 1.);
    assert_eq!(g[LIN_SIDE_OFFSET], 0.);
    assert_eq!(g[LOG_SIDE_SLOPE], 1.);
    assert_eq!(g[LOG_SIDE_OFFSET], 0.);
    assert_eq!(g[LIN_SIDE_BREAK], 0.1);
}

#[test]
fn multiple_ops() {
    let t = load("clf/multiple_ops.clf").unwrap();
    assert_eq!(t.ops.len(), 9);
    {
        // Op 0 == CDL.
        let c = as_cdl(&t.ops[0]);
        assert_eq!(op_descs(&t.ops[0]), vec!["scene 1 exterior look"]);
        assert_eq!(c.style, CdlOpStyle::V12Rev);
        assert_eq!(c.slope, [1.1, 1., 0.8]);
        assert_eq!(c.offset, [-0.01, 0., 0.05]);
        assert_eq!(c.power, [1.05, 1.15, 0.8]);
        assert_eq!(c.sat, 0.85);
    }
    {
        // Op 1 == Lut1D.
        let l = as_lut1d(&t.ops[1]);
        assert_eq!(l.file_output_bd, BitDepth::UInt12);
        assert!(op_descs(&t.ops[1]).is_empty());
        assert_eq!(l.length, 65);
    }
    {
        // Op 2 == Range: check that the noClamp style Range became a Matrix.
        let m = as_matrix(&t.ops[2]);
        assert_eq!(m.file_in_bd, BitDepth::UInt12);
        assert_eq!(m.file_out_bd, BitDepth::UInt10);
        let out_scale = BitDepth::UInt10.max_value();
        let mat_scale = out_scale / BitDepth::UInt12.max_value();
        let scalef = (1200.0f32 - 20.0) / (3760.0 - 256.0);
        let offsetf = 20.0f32 - scalef * 256.0;
        let prec = 10000.0f32;
        let scale = (prec * scalef) as i32;
        let offset = (prec * offsetf) as i32;
        assert!(matrix_is_diagonal(m));
        let p = f64::from(prec);
        assert_eq!((p * m.matrix[0] * mat_scale) as i32, scale);
        assert_eq!((p * m.matrix[5] * mat_scale) as i32, scale);
        assert_eq!((p * m.matrix[10] * mat_scale) as i32, scale);
        assert_eq!(m.matrix[15], 1.0);
        assert_eq!((p * m.offsets[0] * out_scale) as i32, offset);
        assert_eq!((p * m.offsets[1] * out_scale) as i32, offset);
        assert_eq!((p * m.offsets[2] * out_scale) as i32, offset);
        assert_eq!(m.offsets[3], 0.0);
    }
    {
        // Op 3 == Range with Clamp.
        let r = as_range(&t.ops[3]);
        assert_eq!(r.file_in_bd, BitDepth::UInt10);
        assert_eq!(r.file_out_bd, BitDepth::UInt10);
    }
    {
        // Op 4 == Range with Clamp (a range without style defaults to clamp).
        let r = as_range(&t.ops[4]);
        assert_eq!(r.file_in_bd, BitDepth::UInt10);
        assert_eq!(r.file_out_bd, BitDepth::UInt10);
    }
    // Op 5 == Log.
    as_log(&t.ops[5]);
    {
        // Op 6 == Matrix with offset.
        let m = as_matrix(&t.ops[6]);
        assert_eq!(m.matrix[2], -0.2);
        assert_eq!(m.offsets[1], -0.11);
    }
    // Op 7 == Exponent.
    as_gamma(&t.ops[7]);
    // Op 8 == Lut3D.
    as_lut3d(&t.ops[8]);
}

// NOTE: These tests are on the ReferenceOpData itself, before it gets
// replaced with the ops from the file it is referencing.

#[test]
fn reference_load_alias() {
    let t = load("reference_alias.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_reference(&t.ops[0]);
    assert_eq!(op_name(&t.ops[0]), "name");
    assert_eq!(op_id(&t.ops[0]), "uuid");
    assert_eq!(r.path, "");
    assert_eq!(r.alias, "alias");
    assert_eq!(r.dir, TransformDirection::Forward);
}

#[test]
fn reference_load_path() {
    let t = load("reference_path_missing_file.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_reference(&t.ops[0]);
    assert_eq!(r.path, "toto/toto.ctf");
    assert_eq!(r.alias, "");
    assert_eq!(r.dir, TransformDirection::Inverse);
}

#[test]
fn reference_load_multiple() {
    // File contains 2 references, 1 range and 1 reference.
    let t = load("references_some_inverted.ctf").unwrap();
    assert_eq!(t.ops.len(), 4);
    let r0 = as_reference(&t.ops[0]);
    assert_eq!(r0.path, "matrix_example_1_3_offsets.ctf");
    assert_eq!(r0.dir, TransformDirection::Forward);
    let r1 = as_reference(&t.ops[1]);
    assert_eq!(r1.path, "clf/xyz_to_rgb.clf");
    assert_eq!(r1.dir, TransformDirection::Inverse);
    as_range(&t.ops[2]);
    let r3 = as_reference(&t.ops[3]);
    assert_eq!(r3.path, "clf/cdl_clamp_fwd.clf");
    // The "inverted" attribute set to anything other than true does not
    // result in an inverted transform.
    assert_eq!(r3.dir, TransformDirection::Forward);
}

#[test]
fn reference_load_path_utf8() {
    let t = load("reference_utf8.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_reference(&t.ops[0]);
    assert_eq!(
        r.path.as_bytes(),
        b"\xE6\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC"
    );
    assert_eq!(r.alias, "");
}

#[test]
fn reference_load_alias_path() {
    // Can't have alias and path at the same time.
    check_load_err(
        "reference_alias_path.ctf",
        "alias & path attributes for Reference should not be both defined",
    );
}

pub(super) fn ec_is_dynamic(ec: &EcData) -> bool {
    ec.exposure_dynamic || ec.contrast_dynamic || ec.gamma_dynamic
}

#[test]
fn exposure_contrast_video() {
    let t = load("exposure_contrast_video.ctf").unwrap();
    assert_eq!(t.ops.len(), 2);
    let ec = as_ec(&t.ops[0]);
    assert_eq!(ec.style, EcOpStyle::Video);
    assert_eq!(ec.exposure, -1.0);
    assert_eq!(ec.contrast, 1.5);
    assert_eq!(ec.pivot, 0.5);
    assert!(ec_is_dynamic(ec));
    assert!(ec.exposure_dynamic);
    assert!(ec.contrast_dynamic);
    assert!(!ec.gamma_dynamic);
    let ec = as_ec(&t.ops[1]);
    assert!(!ec_is_dynamic(ec));
    assert_eq!(ec.style, EcOpStyle::VideoRev);
}

#[test]
fn exposure_contrast_log() {
    let t = load("exposure_contrast_log.ctf").unwrap();
    assert_eq!(t.ops.len(), 2);
    let ec = as_ec(&t.ops[0]);
    assert_eq!(ec.style, EcOpStyle::Log);
    assert_eq!(ec.exposure, -1.5);
    assert_eq!(ec.contrast, 0.5);
    assert_eq!(ec.gamma, 1.2);
    assert_eq!(ec.pivot, 0.18);
    assert!(ec_is_dynamic(ec));
    assert!(ec.exposure_dynamic);
    assert!(ec.contrast_dynamic);
    assert!(!ec.gamma_dynamic);
    let ec = as_ec(&t.ops[1]);
    assert_eq!(ec.style, EcOpStyle::LogRev);
    assert!(ec_is_dynamic(ec));
    assert!(!ec.exposure_dynamic);
    assert!(!ec.contrast_dynamic);
    assert!(ec.gamma_dynamic);
}

#[test]
fn exposure_contrast_linear() {
    let t = load("exposure_contrast_linear.ctf").unwrap();
    assert_eq!(t.ops.len(), 2);
    let ec = as_ec(&t.ops[0]);
    assert_eq!(ec.style, EcOpStyle::Linear);
    assert_eq!(ec.exposure, 0.65);
    assert_eq!(ec.contrast, 1.2);
    assert_eq!(ec.gamma, 0.5);
    assert_eq!(ec.pivot, 1.0);
    assert!(ec.exposure_dynamic);
    assert!(ec.contrast_dynamic);
    assert!(ec.gamma_dynamic);
    let ec = as_ec(&t.ops[1]);
    assert_eq!(ec.style, EcOpStyle::LinearRev);
    assert!(!ec_is_dynamic(ec));
}

#[test]
fn exposure_contrast_no_gamma() {
    let t = load("exposure_contrast_no_gamma.ctf").unwrap();
    assert_eq!(t.ops.len(), 1);
    let ec = as_ec(&t.ops[0]);
    assert_eq!(ec.style, EcOpStyle::Video);
    assert_eq!(ec.exposure, 0.2);
    assert_eq!(ec.contrast, 0.65);
    assert_eq!(ec.pivot, 0.23);
    assert_eq!(ec.gamma, 1.0);
    assert!(!ec_is_dynamic(ec));
}

#[test]
fn exposure_contrast_failures() {
    check_load_err(
        "exposure_contrast_bad_style.ctf",
        "Unknown exposure contrast style",
    );
    check_load_err("exposure_contrast_missing_param.ctf", "exposure missing");
}

#[test]
fn attribute_float_parse_extra_values() {
    // Attribute float parsing will throw if extra values are present.
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="empty" version="1.7">
   <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
      <ECParams exposure="-1.5 1.2" contrast="0.5" gamma="1.2" pivot="0.18" />
   </ExposureContrast>
</ProcessList>
"#;
    check_err(parse(s), "Expecting 1 value, found 2 values");
}

#[test]
fn attribute_float_parse_leading_spaces() {
    // Attribute float parsing will not fail if extra leading white space is
    // present.
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="empty" version="1.7">
   <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
      <ECParams exposure="    -1.5 " contrast="0.5" gamma="1.2" pivot="0.18" />
   </ExposureContrast>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 1);
    assert_eq!(as_ec(&t.ops[0]).exposure, -1.5);
}

#[test]
fn load_deprecated_ops_file() {
    let t = load("deprecated_ops.ctf").unwrap();
    assert_eq!(t.ops.len(), 3);
    {
        // ACES RedMod03 (deprecated) conversion to the modern representation.
        let f = as_ff(&t.ops[0]);
        assert_eq!(f.style, FixedFunctionStyle::AcesRedMod03);
        assert_eq!(f.dir, TransformDirection::Inverse);
        f.validate().unwrap();
        assert!(f.params.is_empty());
    }
    {
        // ACES Surround (deprecated) conversion to the modern representation.
        let f = as_ff(&t.ops[1]);
        assert_eq!(f.style, FixedFunctionStyle::Rec2100Surround);
        assert_eq!(f.dir, TransformDirection::Forward);
        f.validate().unwrap();
        assert_eq!(f.params, vec![1.2]);
    }
    {
        // Function (deprecated) conversion to the modern representation.
        let f = as_ff(&t.ops[2]);
        assert_eq!(f.style, FixedFunctionStyle::RgbToHsv);
        assert_eq!(f.dir, TransformDirection::Inverse);
        f.validate().unwrap();
        assert!(f.params.is_empty());
    }
}

#[test]
fn load_fixed_function_file() {
    let t = load("fixed_function.ctf").unwrap();
    assert_eq!(t.ops.len(), 2);
    {
        let f = as_ff(&t.ops[0]);
        assert_eq!(f.style, FixedFunctionStyle::Rec2100Surround);
        assert_eq!(f.dir, TransformDirection::Forward);
        f.validate().unwrap();
        assert_eq!(f.params, vec![0.8]);
    }
    {
        let f = as_ff(&t.ops[1]);
        assert_eq!(f.style, FixedFunctionStyle::RgbToHsv);
        assert_eq!(f.dir, TransformDirection::Inverse);
        f.validate().unwrap();
        assert!(f.params.is_empty());
    }
}

/// Port of `WriteGroupCTF`.
pub(super) fn write_group_ctf(group: &GroupTransform) -> Result<String> {
    let config = crate::Config::create_raw();
    group.write(&config, crate::fileformats::FILEFORMAT_CTF)
}

/// Port of `WriteGroupCLF`.
pub(super) fn write_group_clf(group: &GroupTransform) -> Result<String> {
    let config = crate::Config::create_raw();
    group.write(&config, crate::fileformats::FILEFORMAT_CLF)
}

/// Port of `ValidateFixedFunctionStyle`: load & save any FixedFunction
/// style.
#[track_caller]
fn validate_fixed_function_style(style: &str, vers: &str, params: &str) {
    let mut ff = format!("<FixedFunction inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{style}\"");
    if !params.is_empty() {
        ff.push_str(&format!(" params=\"{params}\""));
    }
    ff.push('>');
    let s = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ProcessList version=\"{vers}\" id=\"ABCD\">\n    {ff}\n    </FixedFunction>\n</ProcessList>\n"
    );

    // Test parsing.
    let t = parse(&s).unwrap();
    assert_eq!(t.ops.len(), 1);
    let f = as_ff(&t.ops[0]);
    assert_eq!(
        super::opdata::ff_style_to_name(f.style, f.dir, false).unwrap(),
        style
    );

    let mut group = GroupTransform::new();
    group.metadata.add_attribute(METADATA_ID, "ABCD");
    group.transforms.push(t.ops[0].to_transform().unwrap());

    // Test serialization.
    let out = write_group_ctf(&group).unwrap();
    assert_eq!(out, s);
}

#[test]
fn ff_load_save_ctf() {
    let v = validate_fixed_function_style;
    v("RedMod03Fwd", "2", "");
    v("RedMod03Rev", "2", "");
    v("RedMod10Fwd", "2", "");
    v("RedMod10Rev", "2", "");
    v("Glow03Fwd", "2", "");
    v("Glow03Rev", "2", "");
    v("Glow10Fwd", "2", "");
    v("Glow10Rev", "2", "");
    v("DarkToDim10", "2", "");
    v("DimToDark10", "2", "");
    v(
        "GamutComp13Fwd",
        "2.1",
        "1.147 1.264 1.312 0.815 0.803 0.88 1.2",
    );
    v("Rec2100SurroundFwd", "2", "1");
    v("Rec2100SurroundRev", "2", "1");
    v("RGB_TO_HSV", "2", "");
    v("HSV_TO_RGB", "2", "");
    v("XYZ_TO_xyY", "2", "");
    v("xyY_TO_XYZ", "2", "");
    v("XYZ_TO_uvY", "2", "");
    v("uvY_TO_XYZ", "2", "");
    v("XYZ_TO_LUV", "2", "");
    v("LUV_TO_XYZ", "2", "");
    v("Lin_TO_PQ", "2.4", "");
    v("PQ_TO_Lin", "2.4", "");
    v(
        "Lin_TO_GammaLog",
        "2.4",
        "0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723",
    );
    v(
        "GammaLog_TO_Lin",
        "2.4",
        "0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723",
    );
    v(
        "Lin_TO_DoubleLog",
        "2.4",
        "10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0",
    );
    v(
        "DoubleLog_TO_Lin",
        "2.4",
        "10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0",
    );
    v(
        "ACESOutputTransform20Fwd",
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v(
        "ACESOutputTransform20Inv",
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v(
        "RGB_TO_JMh_20",
        "2.4",
        "0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v(
        "JMh_TO_RGB_20",
        "2.4",
        "0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v("ToneScaleCompress20Fwd", "2.4", "1000");
    v("ToneScaleCompress20Inv", "2.4", "1000");
    v(
        "GamutCompress20Fwd",
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v(
        "GamutCompress20Inv",
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    v("RGB_TO_HSY_LOG", "2.5", "");
    v("HSY_LOG_TO_RGB", "2.5", "");
    v("RGB_TO_HSY_LIN", "2.5", "");
    v("HSY_LIN_TO_RGB", "2.5", "");
    v("RGB_TO_HSY_VID", "2.5", "");
    v("HSY_VID_TO_RGB", "2.5", "");
    v(
        "RGB_TO_HMJ_20",
        "2.6",
        "0.7347 0.2653 0 1 0.0001 -0.077 0.32168 0.33767",
    );
    v(
        "HMJ_TO_RGB_20",
        "2.6",
        "0.7347 0.2653 0 1 0.0001 -0.077 0.32168 0.33767",
    );
}

#[test]
fn load_ff_fail_version() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='1.5'>\n    <FixedFunction inBitDepth='8i' outBitDepth='32f' \
params = '0.8' style = 'Rec2100SurroundFwd' />\n</ProcessList>\n";
    check_err(
        parse(s),
        "CTF file version '1.5' does not support operator 'FixedFunction'",
    );
}

#[test]
fn load_ff_fail_params() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='2'>\n    <FixedFunction inBitDepth='8i' outBitDepth='32f' \
params = '0.8 2.0' style = 'Rec2100SurroundFwd' />\n</ProcessList>\n";
    check_err(parse(s), "must have one parameter but 2 found");
}

#[test]
fn load_ff_fail_style() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='2.0'>\n    <FixedFunction inBitDepth='16i' outBitDepth='32f' style='UnknownStyle' />\n\
</ProcessList>\n";
    check_err(parse(s), "Unknown FixedFunction style");
}

#[test]
fn load_ff_aces_fail_gamma_param() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='1.5'>\n    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n\
        <ACESParams wrongParam='1.2' />\n    </ACES>\n</ProcessList>\n";
    check_err(parse(s), "Missing required parameter");
}

#[test]
fn load_ff_aces_fail_gamma_twice() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='1.5'>\n    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n\
        <ACESParams gamma='1.2' />\n        <ACESParams gamma='1.4' />\n    </ACES>\n</ProcessList>\n";
    check_err(parse(s), "only 1 gamma parameter");
}

#[test]
fn load_ff_aces_fail_missing_param() {
    let s = "<?xml version='1.0' encoding='UTF-8'?>\n\
<ProcessList id='none' version='1.5'>\n    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n\
    </ACES>\n</ProcessList>\n";
    check_err(parse(s), "must have one parameter");
}

use crate::transforms::grading::{
    default_hue_curve, default_rgb_curve, GradingControlPoint, GradingPrimary, GradingRgbm,
};

fn rgbm(r: f64, g: f64, b: f64, m: f64) -> GradingRgbm {
    GradingRgbm::new(r, g, b, m)
}

#[test]
fn load_grading_primary_log() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="log">
        <Brightness rgb="0.1 0 0" master="0" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Saturation master="1.1" />
        <Pivot contrast="-0.1" black="0.1" white="1.1" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="log">
        <Brightness rgb="0.1 0 0" master="0" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Saturation master="1.1" />
        <Pivot black="0.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
        <Brightness rgb="-0.25 0.5 7.62939453125e-06" master="1" />
        <Contrast rgb="1.25 1.5 -1.25" master="103125e-5" />
        <Gamma rgb="0.75 0.625 1.25" master="0.9e+1" />
        <Pivot contrast="0.75" />
        <Saturation master="1.03125" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 4);
    let def = GradingPrimary::new(GradingStyle::Log);

    let g = as_primary(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.brightness, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v.contrast, rgbm(1., 1.1, 1., 1.));
    assert_eq!(v.gamma, rgbm(1., 1., 1., 1.1));
    assert_eq!(v.saturation, 1.1);
    assert_eq!(v.pivot, -0.1);
    assert_eq!(v.pivot_black, 0.1);
    assert_eq!(v.pivot_white, 1.1);
    assert_eq!(v.clamp_black, 0.);
    assert_eq!(v.clamp_white, 1.);
    assert!(!g.dynamic);

    let g = as_primary(&t.ops[1]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert_eq!(g.value, def);
    assert!(g.dynamic);

    let g = as_primary(&t.ops[2]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.brightness, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v.contrast, def.contrast);
    assert_eq!(v.gamma, rgbm(1., 1., 1., 1.1));
    assert_eq!(v.saturation, 1.1);
    assert_eq!(v.pivot, def.pivot);
    assert_eq!(v.pivot_black, 0.1);
    assert_eq!(v.pivot_white, def.pivot_white);
    assert_eq!(v.clamp_black, def.clamp_black);
    assert_eq!(v.clamp_white, 1.);
    assert!(!g.dynamic);

    let g = as_primary(&t.ops[3]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.brightness, rgbm(-0.25, 0.5, 7.62939453125e-06, 1.));
    assert_eq!(v.contrast, rgbm(1.25, 1.5, -1.25, 103125.0e-5));
    assert_eq!(v.gamma, rgbm(0.75, 0.625, 1.25, 0.9e+1));
    assert_eq!(v.saturation, 1.03125);
    assert_eq!(v.pivot, 0.75);
    assert_eq!(v.pivot_black, def.pivot_black);
    assert_eq!(v.pivot_white, def.pivot_white);
    assert_eq!(v.clamp_black, def.clamp_black);
    assert_eq!(v.clamp_white, def.clamp_white);
    assert!(g.dynamic);
}

#[test]
fn load_grading_primary_lin() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linear">
        <Offset rgb="0.1 0 0" master="0" />
        <Exposure rgb="0 0 0" master="0.1" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Saturation master="1.1" />
        <Pivot contrast="0.28" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linearRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="linear">
        <Offset rgb="0.1 0 0" master="0" />
        <Exposure rgb="0 0 0" master="0.1" />
        <Saturation master="1.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="linear">
        <Offset rgb="0.25 -0.015625 +.03125" master="0" />
        <Exposure rgb="-0.25 0.5 7.62939453125e-06" master="1" />
        <Contrast rgb="0.75 0.625 1.25" master=".9e+1" />
        <Pivot contrast="1.25" />
        <Saturation master="1.125" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 4);
    let def = GradingPrimary::new(GradingStyle::Lin);

    let g = as_primary(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v0 = &g.value;
    assert_eq!(v0.offset, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v0.exposure, rgbm(0., 0., 0., 0.1));
    assert_eq!(v0.contrast, rgbm(1., 1.1, 1., 1.));
    assert_eq!(v0.saturation, 1.1);
    assert_eq!(v0.pivot, 0.28);
    assert_eq!(v0.clamp_black, 0.);
    assert_eq!(v0.clamp_white, 1.);
    assert!(!g.dynamic);

    let g = as_primary(&t.ops[1]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert_eq!(g.value, def);
    assert!(g.dynamic);

    let g = as_primary(&t.ops[2]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.offset, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v.exposure, rgbm(0., 0., 0., 0.1));
    assert_eq!(v.contrast, def.contrast);
    assert_eq!(v.saturation, 1.1);
    assert_eq!(v.pivot, def.pivot);
    assert_eq!(v.clamp_black, def.clamp_black);
    assert_eq!(v.clamp_white, 1.);
    assert!(!g.dynamic);

    let g = as_primary(&t.ops[3]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.offset, rgbm(0.25, -0.015625, 0.03125, 0.));
    assert_eq!(v.exposure, rgbm(-0.25, 0.5, 7.62939453125e-06, 1.));
    assert_eq!(v.contrast, rgbm(0.75, 0.625, 1.25, 0.9e+1));
    assert_eq!(v.saturation, 1.125);
    assert_eq!(v.pivot, 1.25);
    assert_eq!(v.clamp_black, def.clamp_black);
    assert_eq!(v.clamp_white, def.clamp_white);
    assert!(g.dynamic);
}

#[test]
fn load_grading_primary_video() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="video">
        <Lift rgb="0.1 0 0" master="0" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Gain rgb="1 1.1 1" master="1" />
        <Offset rgb="0 0.1 0" master="0" />
        <Saturation master="1.1" />
        <Pivot black="0.1" white="1.1" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="videoRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="video">
        <Lift rgb="0.1 0 0" master="0" />
        <Gain rgb="1 1.1 1" master="1" />
        <Pivot black="0.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
        <Lift rgb="0.25 -.015625 .03125" master="0" />
        <Gamma rgb=".75 .625 1.25" master=".09e2" />
        <Gain rgb="-0.25 00.500 7.62939453125e-06" master="1" />
        <Offset rgb="02.5 +0.5 -.125" master="1" />
        <Pivot black="-0.25" white="12" />
        <Saturation master="1" />
        <Clamp black="0.5" white="1.5" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 4);
    let def = GradingPrimary::new(GradingStyle::Video);

    let g = as_primary(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.lift, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v.gamma, rgbm(1., 1., 1., 1.1));
    assert_eq!(v.gain, rgbm(1., 1.1, 1., 1.));
    assert_eq!(v.offset, rgbm(0., 0.1, 0., 0.));
    assert_eq!(v.saturation, 1.1);
    assert_eq!(v.pivot_black, 0.1);
    assert_eq!(v.pivot_white, 1.1);
    assert_eq!(v.clamp_black, 0.);
    assert_eq!(v.clamp_white, 1.);
    assert!(!g.dynamic);

    let g = as_primary(&t.ops[1]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert_eq!(g.value, def);
    assert!(g.dynamic);

    let g = as_primary(&t.ops[2]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.lift, rgbm(0.1, 0., 0., 0.));
    assert_eq!(v.gamma, def.gamma);
    assert_eq!(v.gain, rgbm(1., 1.1, 1., 1.));
    assert_eq!(v.offset, def.offset);
    assert_eq!(v.saturation, def.saturation);
    assert_eq!(v.pivot_black, 0.1);
    assert_eq!(v.pivot_white, def.pivot_white);
    assert_eq!(v.clamp_black, def.clamp_black);
    assert_eq!(v.clamp_white, 1.);

    let g = as_primary(&t.ops[3]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.lift, rgbm(0.25, -0.015625, 0.03125, 0.));
    assert_eq!(v.gamma, rgbm(0.75, 0.625, 1.25, 0.9e+1));
    assert_eq!(v.gain, rgbm(-0.25, 0.500, 7.62939453125e-06, 1.));
    assert_eq!(v.offset, rgbm(2.5, 0.5, -0.125, 1.));
    assert_eq!(v.saturation, 1.);
    assert_eq!(v.pivot_black, -0.25);
    assert_eq!(v.pivot_white, 12.);
    assert_eq!(v.clamp_black, 0.5);
    assert_eq!(v.clamp_white, 1.5);
    assert!(g.dynamic);
}

#[test]
fn load_grading_primary_errors() {
    // Wrong version.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.8" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "CTF file version '1.8' does not support operator 'GradingPrimary'",
    );

    // Master attribute for pivot instead of contrast.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <Lift rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="1.0 1.0 1.0" master="1" />
      <Gain rgb="0.25 0.5 0.25" master="1" />
      <Offset rgb="0.25 0.5 0.25" master="1" />
      <Saturation master="1" />
      <Pivot master="1">
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Illegal attribute for 'Pivot': 'master'",
    );

    // Brightness does not have 3 rgb values.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
      <Brightness rgb="0.25 0.5" master="1" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Illegal number of 'rgb' values for 'Brightness'",
    );

    // Gamma values should be above lower bound.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <Lift rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.0 0.0 0.0" master="1" />
      <Gain rgb="0.25 0.5 0.25" master="1" />
      <Offset rgb="0.25 0.5 0.25" master="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "GradingPrimary gamma '<r=0, g=0, b=0, m=1>' are below lower bound (0.01)",
    );

    // Brightness does not have a master value.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
      <Brightness rgb="0.25 0.5 0.25" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Missing 'master' attribute for 'Brightness'",
    );

    // Missing style attribute.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingPrimary inBitDepth="16f" outBitDepth="32f">
      <Brightness rgb="0.25 0.5 0.25" master=".1" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Unsupported dynamic parameter for GradingPrimary.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="CONTRAST" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Dynamic parameter 'CONTRAST' is not supported in 'GradingPrimary'",
    );
}

#[test]
fn load_grading_rgbcurves_lin() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         -7 -6
                          0 0
                          7 7
            </ControlPoints>
        </Red>
        <Master>
            <ControlPoints>
                         -7 -7
                          0 0
                          7 7
                         16 10
            </ControlPoints>
        </Master>
    </GradingRGBCurve>
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear" bypassLinToLog="true">
        <DynamicParameter param="RGB_CURVE" />
    </GradingRGBCurve>
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="video">
        <Master>
            <ControlPoints>
                          0 0 1 1
                        1.5 1.4
            </ControlPoints>
            <Slopes> 0 0.7 1.1 </Slopes>
        </Master>
    </GradingRGBCurve>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 3);
    let def_lin = default_rgb_curve(GradingStyle::Lin);

    let g = as_rgb_curve(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert!(!g.bypass_lin_to_log);
    assert!(!g.dynamic);
    assert_eq!(g.value.curve(RgbCurveType::Master).num_control_points(), 4);
    assert_eq!(*g.value.curve(RgbCurveType::Blue), def_lin);
    assert_ne!(*g.value.curve(RgbCurveType::Red), def_lin);

    let g = as_rgb_curve(&t.ops[1]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert!(g.bypass_lin_to_log);
    assert!(g.dynamic);
    assert_eq!(*g.value.curve(RgbCurveType::Red), def_lin);
    assert!(g.value.curve(RgbCurveType::Red).slopes_are_default());

    let g = as_rgb_curve(&t.ops[2]);
    assert_eq!(g.style, GradingStyle::Video);
    assert!(!g.bypass_lin_to_log);
    assert!(!g.dynamic);
    let master = g.value.curve(RgbCurveType::Master);
    assert_eq!(master.num_control_points(), 3);
    assert_eq!(master.control_points[0], GradingControlPoint::new(0.0, 0.0));
    assert_eq!(master.control_points[1], GradingControlPoint::new(1.0, 1.0));
    assert_eq!(master.control_points[2], GradingControlPoint::new(1.5, 1.4));
    assert!(!master.slopes_are_default());
    assert_eq!(master.slope(0), 0.0);
    assert_eq!(master.slope(1), 0.7);
    assert_eq!(master.slope(2), 1.1);
}

#[test]
fn load_grading_rgbcurves_log() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingRGBCurve inBitDepth="16f" outBitDepth="32f" style="logRev">
      <Red>
         <ControlPoints>
            0.015625 0
            0.5 0.5
            2 2
         </ControlPoints>
      </Red>
      <Green>
         <ControlPoints>
            0.015625 0.1
            2.5 0.5
            3.5 1.5
         </ControlPoints>
      </Green>
      <Blue>
         <ControlPoints>
            -4 -4
            4.5 0.5
            5 3
         </ControlPoints>
      </Blue>
      <Master>
         <ControlPoints>
            11 11 12.5 11.5 13.5 12.5 26.5 15
         </ControlPoints>
      </Master>
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_rgb_curve(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert!(!g.bypass_lin_to_log);
    assert!(g.dynamic);
    let red = g.value.curve(RgbCurveType::Red);
    assert_eq!(red.num_control_points(), 3);
    let p = &red.control_points;
    assert_eq!((p[0].x, p[0].y), (0.015625, 0.0));
    assert_eq!((p[1].x, p[1].y), (0.5, 0.5));
    assert_eq!((p[2].x, p[2].y), (2.0, 2.0));
    let master = g.value.curve(RgbCurveType::Master);
    assert_eq!(master.num_control_points(), 4);
    let p = &master.control_points;
    assert_eq!((p[0].x, p[0].y), (11.0, 11.0));
    assert_eq!((p[1].x, p[1].y), (12.5, 11.5));
    assert_eq!((p[2].x, p[2].y), (13.5, 12.5));
    assert_eq!((p[3].x, p[3].y), (26.5, 15.0));
}

#[test]
fn load_grading_huecurves_log() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="empty">
   <GradingHueCurve inBitDepth="16f" outBitDepth="32f" style="logRev" hsyTransform="none">
      <HueHue>
         <ControlPoints>
            0.015625 0
            0.5 0.6
            0.9 0.8
         </ControlPoints>
      </HueHue>
      <HueSat>
         <ControlPoints>
            0.015625 1
            0.5 0.5
            0.9 1.5
         </ControlPoints>
      </HueSat>
      <HueLum>
         <ControlPoints>
            0.1, 1.5, 0.2, 0.7, 0.4, 1.4, 0.5, 0.8, 0.8, 0.5
         </ControlPoints>
      </HueLum>
      <LumSat>
         <ControlPoints>
           -0.1  1.0
            0.5  1.5
            1.0  0.9
            1.1  1.2
         </ControlPoints>
      </LumSat>
      <SatSat>
         <ControlPoints>
            0., 0.1, 0.5, 0.45, 1., 1.1
         </ControlPoints>
      </SatSat>
      <LumLum>
         <ControlPoints>
            0.  -0.0005
            0.5  0.3
            1.  0.9
         </ControlPoints>
      </LumLum>
      <SatLum>
         <ControlPoints>
            0., 1.2, 0.6, 0.8, 0.9, 1.1
         </ControlPoints>
      </SatLum>
      <HueFx>
         <ControlPoints>
            0.2, 0.05, .4, -0.09, .6, -0.2, .8, 0.05, 0.99, -0.02
         </ControlPoints>
      </HueFx>
      <DynamicParameter param="HUE_CURVE" />
   </GradingHueCurve>
</ProcessList>"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 1);
    let g = as_hue_curve(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert_eq!(g.rgb_to_hsy, HsyTransformStyle::None);
    assert!(g.dynamic);
    let pts = |c: HueCurveType| -> Vec<(f32, f32)> {
        g.value
            .curve(c)
            .control_points
            .iter()
            .map(|p| (p.x, p.y))
            .collect()
    };
    assert_eq!(
        pts(HueCurveType::HueHue),
        vec![(0.015625, 0.0), (0.5, 0.6), (0.9, 0.8)]
    );
    assert_eq!(
        pts(HueCurveType::LumSat),
        vec![(-0.1, 1.0), (0.5, 1.5), (1.0, 0.9), (1.1, 1.2)]
    );
    assert_eq!(
        pts(HueCurveType::SatLum),
        vec![(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)]
    );
}

#[test]
fn load_grading_curves_errors() {
    // Wrong version.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.8" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "CTF file version '1.8' does not support operator 'GradingRGBCurve'",
    );
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.0" id="empty">
  <GradingHueCurve inBitDepth="16f" outBitDepth="32f" style="video">
  </GradingHueCurve>
</ProcessList>
"#,
        ),
        "CTF file version '2' does not support operator 'GradingHueCurve'",
    );

    // Missing style attribute.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f">
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="empty">
  <GradingHueCurve inBitDepth="16f" outBitDepth="32f">
  </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Wrong dynamic property param.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f">
      <DynamicParameter param="PRIMARY" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Odd number of values for control points.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         -7 -6 0 0 7
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Control points element: odd number of values",
    );

    // Not enough control points.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         0 1
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "There must be at least 2 control points",
    );

    // Control points don't have increasing x.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                 -7 -6 0 0 -1 7
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Control point at index 2 has a x coordinate '-1' that is less than previous control point x coordinate '0'",
    );

    // Number of slopes matches control points.
    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                 -7 -6 0 0 1 7
            </ControlPoints>
            <Slopes> 1 1 1 1 </Slopes>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Number of slopes must match number of control points",
    );

    // Wrong curve: warning is logged.
    let (_t, warnings) = parse_with_warnings(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Grn>
            <ControlPoints>
                         0 1 1 2 2 3
            </ControlPoints>
        </Grn>
    </GradingRGBCurve>
</ProcessList>
"#,
    )
    .unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("Unrecognized element 'Grn'"));
    assert!(warnings[1].contains("Unrecognized element 'ControlPoints'"));

    check_err(
        parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="UIDGradingCurves">
    <GradingHueCurve inBitDepth="32f" outBitDepth="32f" style="linear" hsyTransform="hsy1">
        <DynamicParameter param="HUE_CURVE" />
    </GradingHueCurve>
</ProcessList>
"#,
        ),
        "Unknown hsyTransform value: 'hsy1'",
    );
}

use crate::transforms::grading::{GradingRgbmsw, GradingTone};

fn rgbmsw(r: f64, g: f64, b: f64, m: f64, s: f64, w: f64) -> GradingRgbmsw {
    GradingRgbmsw::new(r, g, b, m, s, w)
}

#[test]
fn load_grading_tone() {
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="log">
        <Blacks rgb="1.1 1 1" master="1" start="0" width="0.98765432198" />
        <Shadows rgb="1 1.1 1" master="1" start="1" pivot="0" />
        <Midtones rgb="1 1 1" master="1.1" center="0" width="1" />
        <Highlights rgb="1 1 1" master="1.1" start="0" pivot="1" />
        <Whites rgb="1 1 1.1" master="1" start="0" width="1" />
        <SContrast master="1.1" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="TONE" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="linear">
        <Blacks rgb="1.1 1 1" master="1" start="0" width="1" />
        <Shadows rgb="1 1.1 1" master="1" start="1" pivot="0" />
        <Whites rgb="1 1 1.1" master="1" start="0" width="1" />
        <SContrast master="1.1" />
        <DynamicParameter param="TONE" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="linearRev">
        <SContrast master="1.12345678912" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="video" />
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="videoRev">
        <Shadows rgb="1 1 1.12345678912" master="1" start="0.6" pivot="0" />
    </GradingTone>
</ProcessList>
"#;
    let t = parse(s).unwrap();
    assert_eq!(t.ops.len(), 6);

    let g = as_tone(&t.ops[0]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.blacks, rgbmsw(1.1, 1., 1., 1., 0., 0.98765432198));
    assert_eq!(v.shadows, rgbmsw(1., 1.1, 1., 1., 1., 0.));
    assert_eq!(v.midtones, rgbmsw(1., 1., 1., 1.1, 0., 1.));
    assert_eq!(v.highlights, rgbmsw(1., 1., 1., 1.1, 0., 1.));
    assert_eq!(v.whites, rgbmsw(1., 1., 1.1, 1., 0., 1.));
    assert_eq!(v.s_contrast, 1.1);
    assert!(!g.dynamic);

    let g = as_tone(&t.ops[1]);
    assert_eq!(g.style, GradingStyle::Log);
    assert_eq!(g.dir, TransformDirection::Inverse);
    assert_eq!(g.value, GradingTone::new(g.style));
    assert!(g.dynamic);

    let g = as_tone(&t.ops[2]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Forward);
    let v = &g.value;
    assert_eq!(v.blacks, rgbmsw(1.1, 1., 1., 1., 0., 1.));
    assert_eq!(v.shadows, rgbmsw(1., 1.1, 1., 1., 1., 0.));
    let def_lin = GradingTone::new(g.style);
    assert_eq!(v.midtones, def_lin.midtones);
    assert_eq!(v.highlights, def_lin.highlights);
    assert_eq!(v.whites, rgbmsw(1., 1., 1.1, 1., 0., 1.));
    assert_eq!(v.s_contrast, 1.1);
    assert!(g.dynamic);

    let g = as_tone(&t.ops[3]);
    assert_eq!(g.style, GradingStyle::Lin);
    assert_eq!(g.dir, TransformDirection::Inverse);
    let mut expected = GradingTone::new(g.style);
    expected.s_contrast = 1.12345678912;
    assert_eq!(g.value, expected);
    assert!(!g.dynamic);

    let g = as_tone(&t.ops[4]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Forward);
    assert_eq!(g.value, GradingTone::new(g.style));
    assert!(!g.dynamic);

    let g = as_tone(&t.ops[5]);
    assert_eq!(g.style, GradingStyle::Video);
    assert_eq!(g.dir, TransformDirection::Inverse);
    let mut expected = GradingTone::new(g.style);
    expected.shadows.blue = 1.12345678912;
    assert_eq!(g.value, expected);
    assert!(!g.dynamic);
}

#[test]
fn default_hue_curves_are_used() {
    // A hue curve op without curves holds the default curves of its style.
    let s = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="x">
    <GradingHueCurve inBitDepth="32f" outBitDepth="32f" style="linear" />
</ProcessList>
"#;
    let t = parse(s).unwrap();
    let g = as_hue_curve(&t.ops[0]);
    for c in [
        HueCurveType::HueHue,
        HueCurveType::SatLum,
        HueCurveType::HueFx,
    ] {
        assert_eq!(*g.value.curve(c), default_hue_curve(c, GradingStyle::Lin));
    }
}
