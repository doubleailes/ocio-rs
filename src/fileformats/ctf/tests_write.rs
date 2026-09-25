//! Writer tests (port of the writer part of `FileFormatCTF_tests.cpp`).
//!
//! The OCIO tests build the group transform to write from a processor of a
//! `FileTransform` (`GetFileTransformProcessor` + `createGroupTransform`).
//! For a CLF/CTF file, this group is the content of the file, so these tests
//! read the file with the CLF/CTF format instead ([`read_file_group`]).

use super::opdata::*;
use super::tests_read::*;
use super::transform::*;
use crate::error::Result;
use crate::transforms::grading::*;
use crate::transforms::*;
use crate::types::*;

/// Port of `WriteRead`: write a transform as CTF and read it back.
fn write_read(t: impl Into<Transform>) -> Result<CtfReaderTransform> {
    let mut group = GroupTransform::new();
    group.transforms.push(t.into());
    let s = write_group_ctf(&group)?;
    parse(&s)
}

#[test]
fn load_edit_save_matrix() {
    let mut group = read_file_group("clf/pre-smpte_only/matrix_example.clf");
    group
        .metadata
        .add_attribute(ATTR_INVERSE_OF, "added inverseOf");
    group.metadata.add_attribute("Unknown", "not saved");
    group.metadata.add_child_element("Unknown", "not saved");
    {
        let info = group.metadata.add_child_element(METADATA_INFO, "Preserved");
        info.add_attribute("attrib", "value");
        info.add_child_element("Child", "Preserved");
    }
    assert_eq!(group.transforms.len(), 1);
    let short_name = r#"A ' short ' " name"#;
    let description1 = r#"A " short " description with a ' inside"#;
    let description2 = r#"<test"'&>"#;
    match &mut group.transforms[0] {
        Transform::Matrix(m) => {
            // Validate how escape characters are saved.
            m.metadata
                .add_child_element(METADATA_DESCRIPTION, description1)
                .add_attribute("Unknown", "not saved");
            m.metadata
                .add_child_element(METADATA_DESCRIPTION, description2);
            m.metadata.set_name(short_name);
            m.offset = [0.1, 1.2, 2.3456789123456, 0.0];
        }
        _ => panic!("expected a matrix"),
    }

    let out = write_group_ctf(&group).unwrap();

    // Output matrix array as '3 4 3'.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="old array dims example" inverseOf="added inverseOf">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Info attrib="value">
    Preserved
        <Child>Preserved</Child>
    </Info>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" name="A &apos; short &apos; &quot; name" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>A &quot; short &quot; description with a &apos; inside</Description>
        <Description>&lt;test&quot;&apos;&amp;&gt;</Description>
        <Array dim="3 4 3">
          0.4123908          0.35758434          0.18048079                 0.1
         0.21263901          0.71516868          0.07219232                 1.2
         0.01933082          0.01191948          0.95053215     2.3456789123456
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);

    // Read the stream back.
    let t = parse(&out).unwrap();
    assert_eq!(t.ops.len(), 1);
    as_matrix(&t.ops[0]);
    let md = t.ops[0].metadata();
    assert_eq!(md.attributes.len(), 2);
    assert_eq!(md.attributes[0].0, METADATA_ID);
    assert_eq!(md.attributes[1].0, METADATA_NAME);
    assert_eq!(md.attributes[1].1, short_name);
    assert_eq!(md.children.len(), 4);
    assert_eq!(md.children[0].element_name, METADATA_DESCRIPTION);
    assert_eq!(md.children[0].element_value, "Legacy matrix");
    assert_eq!(md.children[2].element_name, METADATA_DESCRIPTION);
    assert_eq!(md.children[2].element_value, description1);
    assert_eq!(md.children[3].element_name, METADATA_DESCRIPTION);
    assert_eq!(md.children[3].element_value, description2);
}

#[test]
fn save_matrix() {
    let mut mat = MatrixTransform::default();
    let offset4 = [0.123456789123, 0.11, 0.111, 0.2];
    mat.offset = offset4;
    mat.direction = TransformDirection::Forward;
    let t = write_read(mat).unwrap();
    assert_eq!(t.ops.len(), 1);
    let m = as_matrix(&t.ops[0]);
    assert_eq!(m.offsets, offset4);
}

#[test]
fn save_cdl() {
    let mut cdl = CdlTransform::default();
    cdl.direction = TransformDirection::Forward;
    let slope = [1.1, 1.2, 1.3];
    let offset = [2.1, 2.2, 2.3];
    let power = [3.1, 3.2, 3.3];
    let sat = 0.7;
    cdl.slope = slope;
    cdl.offset = offset;
    cdl.power = power;
    cdl.sat = sat;
    let md = &mut cdl.metadata;
    md.add_attribute(METADATA_ID, "test-cdl-1");
    md.add_child_element(METADATA_DESCRIPTION, "CDL description 1");
    md.add_child_element(METADATA_DESCRIPTION, "CDL description 2");
    md.add_child_element(METADATA_INPUT_DESCRIPTION, "Input");
    md.add_child_element(METADATA_VIEWING_DESCRIPTION, "Viewing");
    md.add_child_element(METADATA_SOP_DESCRIPTION, "SOP description 1");
    md.add_child_element(METADATA_SOP_DESCRIPTION, "SOP description 2");
    md.add_child_element(METADATA_SAT_DESCRIPTION, "Sat description 1");
    md.add_child_element(METADATA_SAT_DESCRIPTION, "Sat description 2");

    let t = write_read(cdl).unwrap();
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    assert_eq!(op_id(&t.ops[0]), "test-cdl-1");
    let md = &c.metadata;
    assert_eq!(md.children.len(), 8);
    let names: Vec<&str> = md
        .children
        .iter()
        .map(|c| c.element_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            METADATA_DESCRIPTION,
            METADATA_DESCRIPTION,
            METADATA_INPUT_DESCRIPTION,
            METADATA_VIEWING_DESCRIPTION,
            METADATA_SOP_DESCRIPTION,
            METADATA_SOP_DESCRIPTION,
            METADATA_SAT_DESCRIPTION,
            METADATA_SAT_DESCRIPTION
        ]
    );
    assert_eq!(c.slope, slope);
    assert_eq!(c.offset, offset);
    assert_eq!(c.power, power);
    assert_eq!(c.sat, sat);
}

#[track_caller]
fn test_save_log(base: f64) {
    let t = write_read(LogTransform::new(base)).unwrap();
    assert_eq!(t.ops.len(), 1);
    assert_eq!(as_log(&t.ops[0]).base, base);
}

#[test]
fn save_log() {
    test_save_log(2.0);
    test_save_log(10.0);
    test_save_log(8.0);
}

#[test]
fn save_log_affine() {
    let mut l = LogAffineTransform::default();
    let base = 8.0;
    l.base = base;
    let vals = [0.9, 1.1, 1.2];
    l.lin_side_slope = vals;
    let t = write_read(l).unwrap();
    assert_eq!(t.ops.len(), 1);
    let log = as_log(&t.ops[0]);
    assert_eq!(log.base, base);
    assert_eq!(log.params[0][LIN_SIDE_SLOPE], vals[0]);
    assert_eq!(log.params[1][LIN_SIDE_SLOPE], vals[1]);
    assert_eq!(log.params[2][LIN_SIDE_SLOPE], vals[2]);
}

#[test]
fn save_log_camera() {
    let vals_break = [0.4, 0.5, 0.6];
    let mut l = LogCameraTransform::new(vals_break);
    let base = 8.0;
    l.base = base;
    let vals = [0.9, 1.1, 1.2];
    l.lin_side_slope = vals;
    let vals_ls = [1.2, 1.3, 1.4];
    l.linear_slope = Some(vals_ls);
    let t = write_read(l).unwrap();
    assert_eq!(t.ops.len(), 1);
    let log = as_log(&t.ops[0]);
    assert_eq!(log.base, base);
    for c in 0..3 {
        assert_eq!(log.params[c][LIN_SIDE_SLOPE], vals[c]);
        assert_eq!(log.params[c][LIN_SIDE_BREAK], vals_break[c]);
        assert_eq!(log.params[c][LINEAR_SLOPE], vals_ls[c]);
    }
}

#[test]
fn save_lut_1d_1component() {
    let group = read_file_group("clf/lut1d_32f_example.clf");
    let result = write_group_ctf(&group).unwrap();
    assert!(result.contains("<Array dim=\"65 1\">"));
}

#[test]
fn save_lut_1d_3components() {
    let group = read_file_group("lut1d_green.ctf");
    let result = write_group_ctf(&group).unwrap();
    assert!(result.contains("<Array dim=\"32 3\">"));
}

#[test]
fn save_invlut_1d_3components() {
    let group = read_file_group("lut1d_inv.ctf");
    let result = write_group_ctf(&group).unwrap();
    assert!(result.contains("</InverseLUT1D>"));
    // Components are equal, so only 1 get saved.
    assert!(result.contains("<Array dim=\"17 1\">"));
}

#[test]
fn save_lut1d_halfdomain() {
    let size = 65536;
    let mut lut = Lut1DTransform::new(size, true);
    lut.file_output_bit_depth = BitDepth::UInt10;
    for i in 0..size {
        let v = half_bits_to_f32(u16::try_from(i).unwrap());
        lut.set_value(i, v, v, v);
    }
    let t = write_read(lut).unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_lut1d(&t.ops[0]);
    assert_eq!(l.file_output_bd, BitDepth::UInt10);
    assert!(l.half_domain);
    assert_eq!(l.length, size);
    for i in 0..size {
        let expected = half::f16::from_bits(u16::try_from(i).unwrap());
        let loaded_val = l.values[3 * i];
        let loaded = half::f16::from_f32(loaded_val);
        if expected.is_nan() {
            assert!(loaded.is_nan());
            assert!(l.values[3 * i + 1].is_nan());
            assert!(l.values[3 * i + 2].is_nan());
        } else {
            assert_eq!(loaded.to_bits(), expected.to_bits(), "index {i}");
            assert_eq!(loaded_val, l.values[3 * i + 1]);
            assert_eq!(loaded_val, l.values[3 * i + 2]);
        }
    }
}

#[test]
fn save_lut1d_f16_raw() {
    let mut lut = Lut1DTransform::new(2, false);
    lut.file_output_bit_depth = BitDepth::F16;
    lut.output_raw_halfs = true;
    let h = |v: f32| half::f16::from_f32(v).to_f32();
    let values = [
        h(1.0 / 3.0),
        half::f16::MAX.to_f32(),
        half::f16::MIN_POSITIVE.to_f32(),
        h(1.0 / 7.0),
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    lut.set_value(0, values[0], values[1], values[2]);
    lut.set_value(1, values[3], values[4], values[5]);
    let t = write_read(lut).unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_lut1d(&t.ops[0]);
    assert_eq!(l.file_output_bd, BitDepth::F16);
    assert_eq!(l.length, 2);
    for (i, v) in values.iter().enumerate() {
        assert_eq!(*v, l.values[i]);
    }
}

#[test]
fn save_lut1d_f32() {
    let mut lut = Lut1DTransform::new(8, false);
    lut.file_output_bit_depth = BitDepth::F32;
    let values = [
        1.0f32 / 3.0,
        0.0000000000000001,
        0.9999999,
        0.0,
        f32::MAX,
        -f32::MIN_POSITIVE,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    for (i, v) in values.iter().enumerate() {
        lut.set_value(i, *v, *v, *v);
    }
    let t = write_read(lut).unwrap();
    assert_eq!(t.ops.len(), 1);
    let l = as_lut1d(&t.ops[0]);
    assert_eq!(l.file_output_bd, BitDepth::F32);
    assert_eq!(l.length, 8);
    for (i, v) in values.iter().enumerate() {
        assert_eq!(l.values[3 * i], *v);
    }
}

#[test]
fn save_lut1d_interpolation() {
    let mut lut = Lut1DTransform::default();
    lut.interpolation = Interpolation::Default;
    let group = GroupTransform::from_transforms(vec![Transform::Lut1D(lut.clone())]);
    let result = write_group_clf(&group).unwrap();
    assert!(result.contains(r#"<LUT1D inBitDepth="32f" outBitDepth="32f">"#));

    lut.interpolation = Interpolation::Best;
    let group = GroupTransform::from_transforms(vec![Transform::Lut1D(lut.clone())]);
    let result = write_group_clf(&group).unwrap();
    assert!(result.contains(r#"<LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">"#));

    lut.interpolation = Interpolation::Linear;
    let group = GroupTransform::from_transforms(vec![Transform::Lut1D(lut.clone())]);
    let result = write_group_clf(&group).unwrap();
    assert!(result.contains(r#"<LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">"#));

    lut.interpolation = Interpolation::Cubic;
    let group = GroupTransform::from_transforms(vec![Transform::Lut1D(lut)]);
    check_err(
        write_group_clf(&group),
        "1D LUT does not support interpolation algorithm: cubic",
    );
}

#[test]
fn save_invalid_lut_1d() {
    let mut lut = Lut1DTransform::new(8, false);
    lut.file_output_bit_depth = BitDepth::F32;
    lut.input_half_domain = true;
    check_err(write_read(lut), "65536 required for halfDomain 1D LUT");
}

#[test]
fn save_lut_3d() {
    let group = read_file_group("clf/lut3d_identity_12i_16f.clf");
    let result = write_group_ctf(&group).unwrap();
    assert!(result.contains("<Array dim=\"2 2 2 3\">"));
}

#[test]
fn save_range() {
    let range = RangeTransform::new(Some(0.0), Some(0.5), Some(0.5), Some(1.5));
    let t = write_read(range.clone()).unwrap();
    assert_eq!(t.ops.len(), 1);
    let r = as_range(&t.ops[0]);
    assert_eq!(Some(r.min_in), range.min_in);
    assert_eq!(Some(r.max_in), range.max_in);
    assert_eq!(Some(r.min_out), range.min_out);
    assert_eq!(Some(r.max_out), range.max_out);
}

#[test]
fn save_group() {
    let range = RangeTransform::new(Some(0.0), Some(0.5), Some(0.5), Some(1.5));
    let mut mat = MatrixTransform::default();
    mat.offset = [0.123456789123, 0.11, 0.111, 0.2];
    let group =
        GroupTransform::from_transforms(vec![Transform::Range(range), Transform::Matrix(mat)]);
    let t = write_read(group).unwrap();
    assert_eq!(t.ops.len(), 2);
    as_range(&t.ops[0]);
    as_matrix(&t.ops[1]);
}

#[test]
fn load_save_matrix() {
    let group = read_file_group("clf/pre-smpte_only/matrix_example.clf");
    let out = write_group_ctf(&group).unwrap();
    // Output matrix array as '3 3 3'.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="old array dims example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Array dim="3 3 3">
          0.4123908          0.35758434          0.18048079
         0.21263901          0.71516868          0.07219232
         0.01933082          0.01191948          0.95053215
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);
}

#[test]
fn save_matrix_444() {
    let config = crate::Config::create_raw();
    let mut mat = MatrixTransform::default();
    mat.matrix = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0.5, 0.5, 0., 1.,
    ];
    let processor = config
        .get_processor_for_transform(&Transform::Matrix(mat), TransformDirection::Forward)
        .unwrap();
    let group = processor.create_group_transform();
    assert_eq!(group.transforms.len(), 1);
    let out = write_group_ctf(&group).unwrap();
    // Output matrix array as '4 4 4'.
    assert!(out.contains("\"4 4 4\""));
}

#[test]
fn save_matrix_444_direct() {
    // Same as save_matrix_444, without the processor.
    let mut mat = MatrixTransform::default();
    mat.matrix = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0.5, 0.5, 0., 1.,
    ];
    let out = write_group_ctf(&GroupTransform::from_transforms(vec![Transform::Matrix(
        mat,
    )]))
    .unwrap();
    assert!(out.contains("\"4 4 4\""));
}

#[test]
fn load_edit_save_matrix_clf() {
    let mut group = read_file_group("clf/pre-smpte_only/matrix_example.clf");
    assert_eq!(group.transforms.len(), 1);
    let set_offset = |group: &mut GroupTransform, o: [f64; 4]| match &mut group.transforms[0] {
        Transform::Matrix(m) => m.offset = o,
        _ => panic!("expected a matrix"),
    };
    match &mut group.transforms[0] {
        Transform::Matrix(m) => {
            m.metadata
                .add_child_element(METADATA_DESCRIPTION, "Added description");
        }
        _ => panic!("expected a matrix"),
    }
    set_offset(&mut group, [0.1, 1.2, 2.3, 0.0]);

    // CLF Academy.
    let out = write_group_clf(&group).unwrap();
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="old array dims example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>Added description</Description>
        <Array dim="3 4">
          0.4123908          0.35758434          0.18048079                 0.1
         0.21263901          0.71516868          0.07219232                 1.2
         0.01933082          0.01191948          0.95053215                 2.3
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected_clf.len(), out.len());
    assert_eq!(expected_clf, out);

    // CTF.
    set_offset(&mut group, [0.1, 1.2, 2.3, 0.9]);
    let out = write_group_ctf(&group).unwrap();
    let expected_ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="old array dims example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>Added description</Description>
        <Array dim="4 5 4">
          0.4123908          0.35758434          0.18048079                   0                 0.1
         0.21263901          0.71516868          0.07219232                   0                 1.2
         0.01933082          0.01191948          0.95053215                   0                 2.3
                  0                   0                   0                   1                 0.9
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected_ctf.len(), out.len());
    assert_eq!(expected_ctf, out);
}

fn group_with_id(id: &str, t: impl Into<Transform>) -> GroupTransform {
    let mut group = GroupTransform::new();
    group.metadata.add_attribute(METADATA_ID, id);
    group.transforms.push(t.into());
    group
}

#[test]
fn matrix3x3_clf() {
    let mut mat = MatrixTransform::default();
    mat.file_input_bit_depth = BitDepth::UInt10;
    mat.file_output_bit_depth = BitDepth::UInt10;
    mat.matrix = [
        1. / 3.,
        10. / 3.,
        100. / 3.,
        0.,
        3.,
        4.,
        5.,
        0.,
        6.,
        7.,
        8.,
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    let group = group_with_id("UID42", mat);
    let out = write_group_clf(&group).unwrap();
    // In/out bit-depth equal, matrix not scaled.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Matrix inBitDepth="10i" outBitDepth="10i">
        <Array dim="3 3">
  0.333333333333333    3.33333333333333    33.3333333333333
                  3                   4                   5
                  6                   7                   8
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);
}

#[test]
fn matrix_offset_alpha_ctf() {
    let mut mat = MatrixTransform::default();
    mat.file_input_bit_depth = BitDepth::UInt10;
    mat.file_output_bit_depth = BitDepth::UInt10;
    mat.matrix = [
        1., 10., 20., 0.5, 3., 4., 5., 0.9, 6., 7., 8., 1.1, 2., 30., 11., 1.,
    ];
    mat.offset = [0.1, 0.2, 0.3, 1.0];
    let group = group_with_id("UID42", mat);
    let out = write_group_ctf(&group).unwrap();
    // Note that offset is scale by 1023 (for output bit-depth).
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="10i" outBitDepth="10i">
        <Array dim="4 5 4">
                  1                  10                  20                 0.5               102.3
                  3                   4                   5                 0.9               204.6
                  6                   7                   8                 1.1               306.9
                  2                  30                  11                   1                1023
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);

    // Alpha not handled by CLF.
    check_err(
        write_group_clf(&group),
        "Transform uses the 'Matrix with alpha component' op which cannot be written as CLF",
    );
}

#[test]
fn matrix_offset_alpha_bitdepth_ctf() {
    let mut mat = MatrixTransform::default();
    mat.file_input_bit_depth = BitDepth::UInt8;
    mat.file_output_bit_depth = BitDepth::UInt12;
    mat.matrix = [
        255. / 4095.,
        0.,
        0.,
        0.,
        0.,
        510. / 4095.,
        0.,
        0.,
        0.,
        0.,
        51. / 91.,
        0.,
        0.,
        0.,
        0.,
        51. / 182.,
    ];
    mat.offset = [0.01, 0.02, 0.03, 0.001];
    let group = group_with_id("UID42", mat);
    let out = write_group_ctf(&group).unwrap();
    // Matrix scale following input bit-depth.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="8i" outBitDepth="12i">
        <Array dim="4 5 4">
                  1                   0                   0                   0               40.95
                  0                   2                   0                   0                81.9
                  0                   0                   9                   0              122.85
                  0                   0                   0                 4.5               4.095
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);
}

#[test]
fn matrix_offset_alpha_inverse_ctf() {
    let mut mat = MatrixTransform::default();
    mat.file_input_bit_depth = BitDepth::F16;
    mat.file_output_bit_depth = BitDepth::F32;
    mat.matrix = [
        2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 8., 0., 0., 0., 0., 1.,
    ];
    mat.offset = [0.1, 0.2, 0.3, 1.0];
    mat.direction = TransformDirection::Inverse;
    let group = group_with_id("UID42", mat);
    let out = write_group_ctf(&group).unwrap();
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="32f" outBitDepth="16f">
        <Array dim="4 5 4">
                0.5                   0                   0                   0               -0.05
                  0                0.25                   0                   0               -0.05
                  0                   0               0.125                   0             -0.0375
                  0                   0                   0                   1                  -1
        </Array>
    </Matrix>
</ProcessList>
"#;
    assert_eq!(expected.len(), out.len());
    assert_eq!(expected, out);
}

#[track_caller]
fn check_ctf(group: &GroupTransform, expected: &str) {
    let out = write_group_ctf(group).unwrap();
    assert_eq!(expected.len(), out.len(), "{out}");
    assert_eq!(expected, out);
}

#[track_caller]
fn check_clf(group: &GroupTransform, expected: &str) {
    let out = write_group_clf(group).unwrap();
    assert_eq!(expected.len(), out.len(), "{out}");
    assert_eq!(expected, out);
}

// Note: OCIO's `legacy_cdl` test writes a CDL with an OCIO v1 config (as
// Matrix/Gamma/Matrix). The config major version is not available to the
// writer of this port which always uses the OCIO v2 behavior.
#[test]
fn legacy_cdl_v2_behavior() {
    let mut cdl = CdlTransform::default();
    cdl.set_sop(&[1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3]);
    cdl.sat = 2.1;
    let group = group_with_id("cdl0", cdl);
    let out = write_group_ctf(&group).unwrap();
    assert!(out.contains("<ASC_CDL inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"FwdNoClamp\">"));
}

#[test]
fn cdl_clf() {
    let mut cdl = CdlTransform::default();
    cdl.set_sop(&[1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3]);
    cdl.sat = 2.1;
    let md = &mut cdl.metadata;
    md.add_attribute(METADATA_NAME, "TestCDL");
    md.add_attribute(METADATA_ID, "CDL42");
    md.add_child_element(METADATA_DESCRIPTION, "CDL node for unit test");
    md.add_child_element(METADATA_DESCRIPTION, "Adding another description");
    md.add_child_element(METADATA_INPUT_DESCRIPTION, "Input");
    md.add_child_element(METADATA_VIEWING_DESCRIPTION, "Viewing");
    md.add_child_element(METADATA_SOP_DESCRIPTION, "SOP description 1");
    md.add_child_element(METADATA_SOP_DESCRIPTION, "SOP description 2");
    md.add_child_element(METADATA_SAT_DESCRIPTION, "Sat description 1");
    md.add_child_element(METADATA_SAT_DESCRIPTION, "Sat description 2");

    let mut group = GroupTransform::new();
    // Need to specify an id so that it does not get generated.
    group.metadata.add_attribute(METADATA_ID, "cdl1");
    group
        .metadata
        .add_child_element(METADATA_DESCRIPTION, "ProcessList description");
    group
        .metadata
        .add_child_element(METADATA_DESCRIPTION, "=======================");
    group.transforms.push(Transform::Cdl(cdl));
    {
        let info = group.metadata.add_child_element(METADATA_INFO, "");
        info.add_child_element("Release", "2019");
        let sub = info.add_child_element("Directors", "");
        for last in ["Cronenberg", "Lynch", "Fincher", "Lean"] {
            let d = sub.add_child_element("Director", "");
            d.add_attribute("FirstName", "David");
            d.add_attribute("LastName", last);
        }
    }

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="cdl1">
    <Description>ProcessList description</Description>
    <Description>=======================</Description>
    <Info>
        <Release>2019</Release>
        <Directors>
            <Director FirstName="David" LastName="Cronenberg"></Director>
            <Director FirstName="David" LastName="Lynch"></Director>
            <Director FirstName="David" LastName="Fincher"></Director>
            <Director FirstName="David" LastName="Lean"></Director>
        </Directors>
    </Info>
    <ASC_CDL id="CDL42" name="TestCDL" inBitDepth="32f" outBitDepth="32f" style="FwdNoClamp">
        <Description>CDL node for unit test</Description>
        <Description>Adding another description</Description>
        <InputDescription>Input</InputDescription>
        <ViewingDescription>Viewing</ViewingDescription>
        <SOPNode>
            <Description>SOP description 1</Description>
            <Description>SOP description 2</Description>
            <Slope>1 1.1 1.2</Slope>
            <Offset>0.2 0.3 0.4</Offset>
            <Power>3.1 3.2 3.3</Power>
        </SOPNode>
        <SatNode>
            <Description>Sat description 1</Description>
            <Description>Sat description 2</Description>
            <Saturation>2.1</Saturation>
        </SatNode>
    </ASC_CDL>
</ProcessList>
"#;
    check_clf(&group, expected);

    // Now test if the read is working.
    let t = parse(expected).unwrap();
    assert_eq!(t.ops.len(), 1);
    let c = as_cdl(&t.ops[0]);
    assert_eq!(c.slope, [1., 1.1, 1.2]);
}

#[test]
fn cdl_ctf() {
    let mut cdl = CdlTransform::default();
    cdl.style = CdlStyle::Asc;
    cdl.set_sop(&[1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3]);
    cdl.sat = 2.1;
    let group = group_with_id("cdl2", cdl);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.7" id="cdl2">
    <ASC_CDL inBitDepth="32f" outBitDepth="32f" style="Fwd">
        <SOPNode>
            <Slope>1 1.1 1.2</Slope>
            <Offset>0.2 0.3 0.4</Offset>
            <Power>3.1 3.2 3.3</Power>
        </SOPNode>
        <SatNode>
            <Saturation>2.1</Saturation>
        </SatNode>
    </ASC_CDL>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn range_ctf() {
    // Non-clamping range are converted to matrix.
    let mut range = RangeTransform::new(Some(0.1), Some(0.9), Some(0.0), Some(1.2));
    range.style = RangeStyle::NoClamp;
    range
        .metadata
        .add_child_element(METADATA_DESCRIPTION, "Range node for unit test");
    range.metadata.add_attribute(METADATA_NAME, "TestRange");
    range.metadata.add_attribute(METADATA_ID, "Range42");
    let mut group = group_with_id("mat0", range);
    group
        .metadata
        .add_child_element(METADATA_INPUT_DESCRIPTOR, "Input descriptor");
    group
        .metadata
        .add_child_element(METADATA_OUTPUT_DESCRIPTOR, "Output descriptor");
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="mat0">
    <InputDescriptor>Input descriptor</InputDescriptor>
    <OutputDescriptor>Output descriptor</OutputDescriptor>
    <Matrix id="Range42" name="TestRange" inBitDepth="32f" outBitDepth="32f">
        <Description>Range node for unit test</Description>
        <Array dim="3 4 3">
                1.5                   0                   0               -0.15
                  0                 1.5                   0               -0.15
                  0                   0                 1.5               -0.15
        </Array>
    </Matrix>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn range1_clf() {
    // Forward clamping range with all 4 values set and with metadata.
    let mut range = RangeTransform::new(
        Some(16.0 / 255.0),
        Some(235. / 255.0),
        Some(-0.5),
        Some(2.1),
    );
    range.file_input_bit_depth = BitDepth::UInt8;
    range.style = RangeStyle::Clamp;
    range
        .metadata
        .add_child_element(METADATA_DESCRIPTION, "Range node for unit test");
    range.metadata.add_attribute(METADATA_NAME, "TestRange");
    range.metadata.add_attribute(METADATA_ID, "Range42");
    let mut group = group_with_id("UID42", range);
    group
        .metadata
        .add_child_element(METADATA_INPUT_DESCRIPTOR, "Input descriptor");
    group
        .metadata
        .add_child_element(METADATA_OUTPUT_DESCRIPTOR, "Output descriptor");
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <InputDescriptor>Input descriptor</InputDescriptor>
    <OutputDescriptor>Output descriptor</OutputDescriptor>
    <Range id="Range42" name="TestRange" inBitDepth="8i" outBitDepth="32f">
        <Description>Range node for unit test</Description>
        <minInValue> 16 </minInValue>
        <maxInValue> 235 </maxInValue>
        <minOutValue> -0.5 </minOutValue>
        <maxOutValue> 2.1 </maxOutValue>
    </Range>
</ProcessList>
"#;
    check_clf(&group, expected);
}

#[test]
fn range2_clf() {
    // Forward clamping range with just minValues set.
    let mut range = RangeTransform::new(Some(0.1), None, Some(0.1), None);
    range.file_input_bit_depth = BitDepth::UInt10;
    range.file_output_bit_depth = BitDepth::UInt8;
    range.metadata.add_attribute(METADATA_ID, "Range42");
    let group = group_with_id("UID42", range);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Range id="Range42" inBitDepth="10i" outBitDepth="8i">
        <minInValue> 102.3 </minInValue>
        <minOutValue> 25.5 </minOutValue>
    </Range>
</ProcessList>
"#;
    check_clf(&group, expected);
}

#[test]
fn range3_clf() {
    // This will only do bit-depth conversion (with a clamp at 0).
    let mut range = RangeTransform::new(Some(0.), None, Some(0.), None);
    range.file_input_bit_depth = BitDepth::F16;
    range.file_output_bit_depth = BitDepth::UInt12;
    range.metadata.add_attribute(METADATA_ID, "Range42");
    let group = group_with_id("UID42", range);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Range id="Range42" inBitDepth="16f" outBitDepth="12i">
        <minInValue> 0 </minInValue>
        <minOutValue> 0 </minOutValue>
    </Range>
</ProcessList>
"#;
    check_clf(&group, expected);
}

#[test]
fn range4_clf() {
    // Inverse clamping range with all 4 values set.
    let mut range = RangeTransform::new(Some(0.), Some(1.0), Some(0.5), Some(1.0));
    range.file_input_bit_depth = BitDepth::F16;
    range.file_output_bit_depth = BitDepth::UInt12;
    range.direction = TransformDirection::Inverse;
    range.metadata.add_attribute(METADATA_ID, "Range42");
    let group = group_with_id("UID42", range);
    // Range is saved in the forward direction.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Range id="Range42" inBitDepth="12i" outBitDepth="16f">
        <minInValue> 2047.5 </minInValue>
        <maxInValue> 4095 </maxInValue>
        <minOutValue> 0 </minOutValue>
        <maxOutValue> 1 </maxOutValue>
    </Range>
</ProcessList>
"#;
    check_clf(&group, expected);
}

fn exp_with_linear(
    gamma: [f64; 4],
    offset: [f64; 4],
    dir: TransformDirection,
) -> ExponentWithLinearTransform {
    let mut e = ExponentWithLinearTransform::default();
    e.gamma = gamma;
    e.offset = offset;
    e.direction = dir;
    e
}

fn exponent(value: [f64; 4], dir: TransformDirection, neg: NegativeStyle) -> ExponentTransform {
    let mut e = ExponentTransform::new(value);
    e.direction = dir;
    e.negative_style = neg;
    e
}

#[test]
fn exponent_ctf() {
    let e = exp_with_linear(
        [1.1, 1.2, 1.3, 1.0],
        [0.1, 0.2, 0.1, 0.0],
        TransformDirection::Forward,
    );
    let group = group_with_id("UID42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <GammaParams channel="R" gamma="1.1" offset="0.1" />
        <GammaParams channel="G" gamma="1.2" offset="0.2" />
        <GammaParams channel="B" gamma="1.3" offset="0.1" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn gamma1_ctf() {
    let e = exponent(
        [2.6, 2.6, 2.6, 1.0],
        TransformDirection::Inverse,
        NegativeStyle::Clamp,
    );
    let group = group_with_id("UID42", e);
    // Identity alpha. Transform written as version 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <GammaParams gamma="2.6" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_clf(&group, expected_clf);
}

#[test]
fn gamma1_mirror_ctf() {
    let e = exponent(
        [2.6, 2.6, 2.6, 1.0],
        TransformDirection::Inverse,
        NegativeStyle::Mirror,
    );
    let group = group_with_id("UID42", e);
    // Identity alpha. Transform written as version 2 because of new style.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_ctf(&group, expected);
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_clf(&group, expected_clf);
}

#[test]
fn gamma1_pass_thru_ctf() {
    let e = exponent(
        [2.6, 2.6, 2.6, 1.0],
        TransformDirection::Inverse,
        NegativeStyle::PassThru,
    );
    let group = group_with_id("UID42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicPassThruRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_ctf(&group, expected);
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicPassThruRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;
    check_clf(&group, expected_clf);
}

#[test]
fn gamma2_ctf() {
    let e = exp_with_linear(
        [2.4, 2.2, 2.0, 1.8],
        [0.1, 0.2, 0.4, 0.8],
        TransformDirection::Inverse,
    );
    let group = group_with_id("UID42", e);
    // Non-identity alpha. Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveRev">
        <GammaParams channel="R" gamma="2.4" offset="0.1" />
        <GammaParams channel="G" gamma="2.2" offset="0.2" />
        <GammaParams channel="B" gamma="2" offset="0.4" />
        <GammaParams channel="A" gamma="1.8" offset="0.8" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
    // CLF does not allow alpha channel.
    check_err(
        write_group_clf(&group),
        "Transform uses the 'Gamma with alpha component' op which cannot be written as CLF",
    );
}

#[test]
fn gamma3_ctf() {
    let e = exp_with_linear(
        [2.42, 2.42, 2.42, 1.0],
        [0.099, 0.099, 0.099, 0.0],
        TransformDirection::Forward,
    );
    let group = group_with_id("UID42", e);
    // Identity alpha. Transform written as version 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <GammaParams gamma="2.42" offset="0.099" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <ExponentParams exponent="2.42" offset="0.099" />
    </Exponent>
</ProcessList>
"#;
    check_clf(&group, expected_clf);
}

#[test]
fn gamma4_ctf() {
    let e = exponent(
        [2.6, 2.5, 2.4, 2.2],
        TransformDirection::Forward,
        NegativeStyle::Clamp,
    );
    let group = group_with_id("UID42", e);
    // Non-identity alpha. Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicFwd">
        <GammaParams channel="R" gamma="2.6" />
        <GammaParams channel="G" gamma="2.5" />
        <GammaParams channel="B" gamma="2.4" />
        <GammaParams channel="A" gamma="2.2" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn gamma5_ctf() {
    let g = 1. / 0.45;
    let e = exp_with_linear([g, g, g, g], [0.099; 4], TransformDirection::Forward);
    let group = group_with_id("UID42", e);
    // Non-identity alpha. Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <GammaParams channel="R" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="G" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="B" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="A" gamma="2.22222222222222" offset="0.099" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn gamma6_ctf() {
    let e = exponent(
        [2.4, 2.5, 2.6, 1.0],
        TransformDirection::Forward,
        NegativeStyle::Clamp,
    );
    let group = group_with_id("UID42", e);
    // R,G,B channels different, but alpha is identity. Written as 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicFwd">
        <GammaParams channel="R" gamma="2.4" />
        <GammaParams channel="G" gamma="2.5" />
        <GammaParams channel="B" gamma="2.6" />
    </Gamma>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

fn ff(
    style: FixedFunctionStyle,
    params: &[f64],
    dir: TransformDirection,
) -> FixedFunctionTransform {
    let mut f = FixedFunctionTransform::new(style, params);
    f.direction = dir;
    f
}

#[track_caller]
fn check_ff(t: FixedFunctionTransform, version: &str, style: &str, params: &str) {
    let group = group_with_id("UIDFF42", t);
    let expected = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ProcessList version=\"{version}\" id=\"UIDFF42\">\n    <FixedFunction inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{style}\" params=\"{params}\">\n    </FixedFunction>\n</ProcessList>\n"
    );
    check_ctf(&group, &expected);
}

#[test]
fn fixed_function_aces_gamut_comp_13_ctf() {
    let data = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let t = ff(
        FixedFunctionStyle::AcesGamutComp13,
        &data,
        TransformDirection::Forward,
    );
    check_ff(
        t,
        "2.1",
        "GamutComp13Fwd",
        "1.147 1.264 1.312 0.815 0.803 0.88 1.2",
    );
}

#[test]
fn fixed_function_aces_gamut_comp_13_inverse_ctf() {
    let data = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let t = ff(
        FixedFunctionStyle::AcesGamutComp13,
        &data,
        TransformDirection::Inverse,
    );
    check_ff(
        t,
        "2.1",
        "GamutComp13Rev",
        "1.147 1.264 1.312 0.815 0.803 0.88 1.2",
    );
}

#[test]
fn fixed_function_rec2100_ctf() {
    let t = ff(
        FixedFunctionStyle::Rec2100Surround,
        &[0.5],
        TransformDirection::Forward,
    );
    check_ff(t, "2", "Rec2100SurroundFwd", "0.5");
}

#[test]
fn fixed_function_rec2100_inverse_ctf() {
    let t = ff(
        FixedFunctionStyle::Rec2100Surround,
        &[0.5],
        TransformDirection::Inverse,
    );
    check_ff(t, "2", "Rec2100SurroundRev", "0.5");
}

#[test]
fn fixed_function_lin_to_gammalog_ctf() {
    let vals = [
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        2.718,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    let p = "0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723";
    check_ff(
        ff(
            FixedFunctionStyle::LinToGammaLog,
            &vals,
            TransformDirection::Forward,
        ),
        "2.4",
        "Lin_TO_GammaLog",
        p,
    );
    check_ff(
        ff(
            FixedFunctionStyle::LinToGammaLog,
            &vals,
            TransformDirection::Inverse,
        ),
        "2.4",
        "GammaLog_TO_Lin",
        p,
    );
}

#[test]
fn fixed_function_lin_to_doublelog_ctf() {
    let vals = [
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];
    let p = "10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0";
    check_ff(
        ff(
            FixedFunctionStyle::LinToDoubleLog,
            &vals,
            TransformDirection::Forward,
        ),
        "2.4",
        "Lin_TO_DoubleLog",
        p,
    );
    check_ff(
        ff(
            FixedFunctionStyle::LinToDoubleLog,
            &vals,
            TransformDirection::Inverse,
        ),
        "2.4",
        "DoubleLog_TO_Lin",
        p,
    );
}

#[test]
fn fixed_function_aces_rgb_to_hmj_20_ctf() {
    let data = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    let p = "0.7347 0.2653 0 1 0.0001 -0.077 0.32168 0.33767";
    check_ff(
        ff(
            FixedFunctionStyle::AcesRgbToHmj20,
            &data,
            TransformDirection::Forward,
        ),
        "2.6",
        "RGB_TO_HMJ_20",
        p,
    );
}

#[test]
fn fixed_function_aces_hmj_to_rgb_20_ctf() {
    let data = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    let p = "0.7347 0.2653 0 1 0.0001 -0.077 0.32168 0.33767";
    check_ff(
        ff(
            FixedFunctionStyle::AcesRgbToHmj20,
            &data,
            TransformDirection::Inverse,
        ),
        "2.6",
        "HMJ_TO_RGB_20",
        p,
    );
}

fn ec(style: ExposureContrastStyle) -> ExposureContrastTransform {
    ExposureContrastTransform {
        style,
        ..Default::default()
    }
}

#[test]
fn exposure_contrast_video_ctf() {
    let mut e = ec(ExposureContrastStyle::Video);
    e.exposure_dynamic = true;
    e.gamma_dynamic = true;
    let group = group_with_id("UIDEC42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="video">
        <ECParams exposure="0" contrast="1" gamma="1" pivot="0.18" />
        <DynamicParameter param="EXPOSURE" />
        <DynamicParameter param="GAMMA" />
    </ExposureContrast>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn exposure_contrast_log_ctf() {
    let mut e = ec(ExposureContrastStyle::Logarithmic);
    e.exposure = -1.5;
    e.contrast = 0.5;
    e.gamma = 1.5;
    e.exposure_dynamic = true;
    e.contrast_dynamic = true;
    let group = group_with_id("UIDEC42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
        <ECParams exposure="-1.5" contrast="0.5" gamma="1.5" pivot="0.18" />
        <DynamicParameter param="EXPOSURE" />
        <DynamicParameter param="CONTRAST" />
    </ExposureContrast>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn exposure_contrast_linear_ctf() {
    let mut e = ec(ExposureContrastStyle::Linear);
    e.exposure = 0.65;
    e.contrast = 1.2;
    e.gamma = 0.8;
    e.pivot = 1.0;
    e.exposure_dynamic = true;
    e.contrast_dynamic = true;
    let group = group_with_id("UIDEC42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="linear">
        <ECParams exposure="0.65" contrast="1.2" gamma="0.8" pivot="1" />
        <DynamicParameter param="EXPOSURE" />
        <DynamicParameter param="CONTRAST" />
    </ExposureContrast>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn exposure_contrast_not_dynamic_ctf() {
    let group = group_with_id("UIDEC42", ec(ExposureContrastStyle::Video));
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="video">
        <ECParams exposure="0" contrast="1" gamma="1" pivot="0.18" />
    </ExposureContrast>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn exposure_contrast_log_params_ctf() {
    let mut e = ec(ExposureContrastStyle::Logarithmic);
    e.exposure = 0.65;
    e.contrast = 1.2;
    e.gamma = 0.5;
    e.pivot = 1.0;
    e.log_exposure_step = 0.1;
    e.log_mid_gray = 0.5;
    e.exposure_dynamic = true;
    let group = group_with_id("UIDEC42", e);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
        <ECParams exposure="0.65" contrast="1.2" gamma="0.5" pivot="1" logExposureStep="0.1" logMidGray="0.5" />
        <DynamicParameter param="EXPOSURE" />
    </ExposureContrast>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn grading_primary_log_ctf() {
    let mut gp = GradingPrimaryTransform::new(GradingStyle::Log);
    // Non-default parameters.
    let mut values = GradingPrimary::new(GradingStyle::Log);
    values.brightness.red += 0.1;
    values.contrast.green += 0.1;
    values.gamma.master += 0.1;
    values.saturation += 0.1;
    values.pivot += 0.1;
    values.pivot_black += 0.1;
    values.pivot_white += 0.1;
    values.clamp_black = 0.0;
    values.clamp_white = 1.0;
    gp.value = values;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="log">
        <Brightness rgb="0.1 0 0" master="0" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Saturation master="1.1" />
        <Pivot contrast="-0.1" black="0.1" white="1.1" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp.clone()), expected);

    // Use default parameters, make dynamic and change direction.
    gp.value = GradingPrimary::new(GradingStyle::Log);
    gp.dynamic = true;
    gp.direction = TransformDirection::Inverse;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp), expected);
}

#[test]
fn grading_primary_lin_ctf() {
    let mut gp = GradingPrimaryTransform::new(GradingStyle::Lin);
    let mut values = GradingPrimary::new(GradingStyle::Lin);
    values.offset.red += 0.1;
    values.exposure.master += -0.99999;
    values.contrast.green += 0.1;
    values.saturation = 1.00001;
    values.pivot += 0.1;
    values.pivot_black += 0.1;
    values.pivot_white += 0.1;
    values.clamp_black = 0.0;
    values.clamp_white = 1.0;
    gp.value = values;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linear">
        <Offset rgb="0.1 0 0" master="0" />
        <Exposure rgb="0 0 0" master="-0.99999" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Saturation master="1.00001" />
        <Pivot contrast="0.28" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp.clone()), expected);

    gp.value = GradingPrimary::new(GradingStyle::Lin);
    gp.dynamic = true;
    gp.direction = TransformDirection::Inverse;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linearRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp), expected);
}

#[test]
fn grading_primary_video_ctf() {
    let mut gp = GradingPrimaryTransform::new(GradingStyle::Video);
    let mut values = GradingPrimary::new(GradingStyle::Video);
    values.lift.red += 0.1;
    values.gamma.master += 0.1;
    values.gain.green += 0.1;
    values.offset.green += 0.1;
    values.saturation += 0.1;
    values.pivot_black += 0.1;
    values.pivot_white += 0.1;
    values.clamp_black = 0.0;
    values.clamp_white = 1.0;
    gp.value = values;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
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
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp.clone()), expected);

    gp.value = GradingPrimary::new(GradingStyle::Video);
    gp.dynamic = true;
    gp.direction = TransformDirection::Inverse;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="videoRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gp), expected);
}

#[test]
fn grading_rgbcurve_log_ctf() {
    let mut gc = GradingRgbCurveTransform::new(GradingStyle::Log);
    // Two of the curves are the default curves, they are not saved.
    let mut curves = GradingRgbCurve::new(GradingStyle::Log);
    curves.curve_mut(RgbCurveType::Green).control_points[0].y = -0.5;
    let master = curves.curve_mut(RgbCurveType::Master);
    master.set_num_control_points(4);
    master.control_points[3].x = 1.5;
    master.control_points[3].y = 1.4;
    gc.value = curves;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="log">
        <Green>
            <ControlPoints>
                          0 -0.5
                        0.5 0.5
                          1 1
            </ControlPoints>
        </Green>
        <Master>
            <ControlPoints>
                          0 0
                        0.5 0.5
                          1 1
                        1.5 1.4
            </ControlPoints>
        </Master>
    </GradingRGBCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc.clone()), expected);

    // All curves are default curves, no curve is saved.
    gc.value = GradingRgbCurve::new(GradingStyle::Log);
    gc.direction = TransformDirection::Inverse;
    // Make it dynamic so it is not identity.
    gc.dynamic = true;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="RGB_CURVE" />
    </GradingRGBCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc), expected);
}

#[test]
fn grading_rgbcurve_video_ctf() {
    let mut gc = GradingRgbCurveTransform::new(GradingStyle::Video);
    gc.dynamic = true;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="video">
        <DynamicParameter param="RGB_CURVE" />
    </GradingRGBCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc), expected);
}

#[test]
fn grading_rgbcurve_lin_ctf() {
    let mut gc = GradingRgbCurveTransform::new(GradingStyle::Lin);
    let mut curves = GradingRgbCurve::new(GradingStyle::Lin);
    curves.curve_mut(RgbCurveType::Red).control_points[0].y = -6.015625;
    let master = curves.curve_mut(RgbCurveType::Master);
    master.set_num_control_points(4);
    master.control_points[3].x = 16.0;
    master.control_points[3].y = 10.0;
    master.set_slope(0, 1.0);
    master.set_slope(1, 0.75);
    master.set_slope(2, 1.1);
    master.set_slope(3, 1.0);
    gc.value = curves;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         -7 -6.015625
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
            <Slopes>
                          1 0.75 1.1 1 
            </Slopes>
        </Master>
    </GradingRGBCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc.clone()), expected);

    // All curves are default curves, no curve is saved.
    gc.value = GradingRgbCurve::new(GradingStyle::Lin);
    gc.bypass_lin_to_log = true;
    gc.dynamic = true;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear" bypassLinToLog="true">
        <DynamicParameter param="RGB_CURVE" />
    </GradingRGBCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc), expected);
}

#[test]
fn grading_huecurve_lin_ctf() {
    let mut gc = GradingHueCurveTransform::new(GradingStyle::Lin);
    // Note: The default curves are not saved.
    let mut curves = GradingHueCurve::new(GradingStyle::Lin);
    curves.curve_mut(HueCurveType::LumSat).control_points[0].y = 1.1;
    let ll = curves.curve_mut(HueCurveType::LumLum);
    ll.set_num_control_points(4);
    ll.control_points[3].x = 16.0;
    ll.control_points[3].y = 10.0;
    ll.set_slope(0, 1.0);
    ll.set_slope(1, 0.75);
    ll.set_slope(2, 1.1);
    ll.set_slope(3, 1.0);
    gc.value = curves;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="UIDGradingCurves">
    <GradingHueCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <LumSat>
            <ControlPoints>
                         -7 1.1
                          0 1
                          7 1
            </ControlPoints>
        </LumSat>
        <LumLum>
            <ControlPoints>
                         -7 -7
                          0 0
                          7 7
                         16 10
            </ControlPoints>
            <Slopes>
                          1 0.75 1.1 1 
            </Slopes>
        </LumLum>
    </GradingHueCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc.clone()), expected);

    // All curves are default curves, no curve is saved.
    gc.value = GradingHueCurve::new(GradingStyle::Lin);
    gc.rgb_to_hsy = HsyTransformStyle::None;
    gc.dynamic = true;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="UIDGradingCurves">
    <GradingHueCurve inBitDepth="32f" outBitDepth="32f" style="linear" hsyTransform="none">
        <DynamicParameter param="HUE_CURVE" />
    </GradingHueCurve>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDGradingCurves", gc), expected);
}

#[test]
fn grading_tone_log_ctf() {
    let mut gt = GradingToneTransform::new(GradingStyle::Log);
    // Leave midtones and scontrast as default and verify they aren't saved.
    let mut values = GradingTone::new(GradingStyle::Log);
    values.blacks.red += 0.12345678912;
    values.shadows.green += 0.1;
    values.highlights.start += 0.1;
    values.whites.width += 0.1;
    gt.value = values;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="log">
        <Blacks rgb="1.12345678912 1 1" master="1" start="0.4" width="0.4" />
        <Shadows rgb="1 1.1 1" master="1" start="0.5" pivot="0" />
        <Highlights rgb="1 1 1" master="1" start="0.4" pivot="1" />
        <Whites rgb="1 1 1" master="1" start="0.4" width="0.6" />
    </GradingTone>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gt.clone()), expected);

    // All are non-default.
    let mut values = gt.value;
    values.blacks = GradingRgbmsw::new(1.1, 0.99999, 1.3, 1.2, 1.1, 1.00001);
    values.shadows = GradingRgbmsw::new(0.99999, 1.00001, 1.3, 1.2, 1.1, 0.1);
    values.midtones = GradingRgbmsw::new(1.1, 1.3, 0.99999, 1.00001, 1.1, 1.2);
    values.highlights = GradingRgbmsw::new(1.1, 1.00001, 1.3, 0.99999, 1.1, 1.2);
    values.whites = GradingRgbmsw::new(1.00001, 1.1, 1.3, 1.2, 0.99999, 1.1);
    values.s_contrast += 0.1111;
    gt.value = values;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="log">
        <Blacks rgb="1.1 0.99999 1.3" master="1.2" start="1.1" width="1.00001" />
        <Shadows rgb="0.99999 1.00001 1.3" master="1.2" start="1.1" pivot="0.1" />
        <Midtones rgb="1.1 1.3 0.99999" master="1.00001" center="1.1" width="1.2" />
        <Highlights rgb="1.1 1.00001 1.3" master="0.99999" start="1.1" pivot="1.2" />
        <Whites rgb="1.00001 1.1 1.3" master="1.2" start="0.99999" width="1.1" />
        <SContrast master="1.1111" />
    </GradingTone>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gt.clone()), expected);

    // Use default parameters, make dynamic and change direction.
    gt.value = GradingTone::new(GradingStyle::Log);
    gt.dynamic = true;
    gt.direction = TransformDirection::Inverse;
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="TONE" />
    </GradingTone>
</ProcessList>
"#;
    check_ctf(&group_with_id("UIDEC42", gt), expected);
}

#[track_caller]
fn check_tone_style(style: GradingStyle, name: &str) {
    // Only test the style, sub elements tested with GRADING_LOG.
    let mut gt = GradingToneTransform::new(style);
    // Make dynamic so it gets saved.
    gt.dynamic = true;
    let expected = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ProcessList version=\"2\" id=\"UIDEC42\">\n    <GradingTone inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{name}\">\n        <DynamicParameter param=\"TONE\" />\n    </GradingTone>\n</ProcessList>\n"
    );
    check_ctf(&group_with_id("UIDEC42", gt.clone()), &expected);
    gt.direction = TransformDirection::Inverse;
    let expected = expected.replace(
        &format!("style=\"{name}\""),
        &format!("style=\"{name}Rev\""),
    );
    check_ctf(&group_with_id("UIDEC42", gt), &expected);
}

#[test]
fn grading_tone_lin_ctf() {
    check_tone_style(GradingStyle::Lin, "linear");
}

#[test]
fn grading_tone_video_ctf() {
    check_tone_style(GradingStyle::Video, "video");
}

#[test]
fn log_lin_to_log_ctf() {
    let mut l = LogAffineTransform::default();
    l.base = 2.0;
    l.lin_side_slope = [0.9, 1.1, 1.2];
    l.lin_side_offset = [0.1, 0.2, 0.3];
    l.log_side_slope = [1.3, 1.4, 1.5];
    l.log_side_offset = [0.4, 0.5, 0.6];
    let group = group_with_id("UIDLOG42", l);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="linToLog">
        <LogParams channel="R" base="2" linSideSlope="0.9" linSideOffset="0.1" logSideSlope="1.3" logSideOffset="0.4" />
        <LogParams channel="G" base="2" linSideSlope="1.1" linSideOffset="0.2" logSideSlope="1.4" logSideOffset="0.5" />
        <LogParams channel="B" base="2" linSideSlope="1.2" linSideOffset="0.3" logSideSlope="1.5" logSideOffset="0.6" />
    </Log>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn log_log_to_lin_ctf() {
    let mut l = LogAffineTransform::default();
    l.direction = TransformDirection::Inverse;
    l.base = 2.0;
    l.lin_side_slope = [0.9, 0.9, 0.9];
    let group = group_with_id("UIDLOG42", l);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="logToLin">
        <LogParams base="2" linSideSlope="0.9" linSideOffset="0" logSideSlope="1" logSideOffset="0" />
    </Log>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn log_antilog2_ctf() {
    let mut l = LogAffineTransform::default();
    l.direction = TransformDirection::Inverse;
    l.base = 2.0;
    let group = group_with_id("UIDLOG42", l);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="antiLog2">
    </Log>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn lut1d_clf() {
    let mut lut = Lut1DTransform::default();
    lut.interpolation = Interpolation::Linear;
    let group = group_with_id("UIDLUT42", lut);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UIDLUT42">
    <LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    check_clf(&group, expected);
}

#[test]
fn lut1d_inverse_clf() {
    let mut lut = Lut1DTransform::default();
    lut.direction = TransformDirection::Inverse;
    let group = group_with_id("UIDLUT42", lut);
    check_err(
        write_group_clf(&group),
        "Transform uses the 'InverseLUT1D' op which cannot be written as CLF",
    );
}

#[test]
fn lut1d_ctf() {
    let mut lut = Lut1DTransform::default();
    lut.interpolation = Interpolation::Default;
    let group = group_with_id("UIDLUT42", lut);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn lut1d_attributes_ctf() {
    let mut lut = Lut1DTransform::new(65536, true);
    lut.metadata.add_attribute(METADATA_NAME, "test-lut");
    lut.metadata.add_attribute(METADATA_ID, "lut01");
    lut.file_output_bit_depth = BitDepth::UInt10;
    lut.interpolation = Interpolation::Default;
    lut.output_raw_halfs = true;
    lut.hue_adjust = Lut1DHueAdjust::Dw3;
    let [r, g, b] = lut.value(1000);
    lut.set_value(1000, r * 1.001, g * 1.002, b * 1.003);
    let group = group_with_id("UIDLUT42", lut);
    let out = write_group_ctf(&group).unwrap();
    let mut lines = out.lines();
    assert_eq!(
        lines.next().unwrap(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
    );
    assert_eq!(
        lines.next().unwrap(),
        r#"<ProcessList version="1.4" id="UIDLUT42">"#
    );
    assert_eq!(
        lines.next().unwrap().trim(),
        "<LUT1D id=\"lut01\" name=\"test-lut\" inBitDepth=\"32f\" outBitDepth=\"10i\" halfDomain=\"true\" rawHalfs=\"true\" hueAdjust=\"dw3\">"
    );
    assert_eq!(lines.next().unwrap().trim(), r#"<Array dim="65536 3">"#);
    let mut line = "";
    for _ in 0..=1000 {
        line = lines.next().unwrap();
    }
    assert_eq!(line.trim(), "11216 11218 11220");
}

fn lut1d_16(three_components: bool) -> Lut1DTransform {
    let mut lut = Lut1DTransform::new(16, false);
    lut.interpolation = Interpolation::Default;
    lut.metadata.add_attribute(METADATA_NAME, "test-lut");
    lut.metadata.add_attribute(METADATA_ID, "lut01");
    lut.file_output_bit_depth = BitDepth::UInt10;
    let mut rgb = 0.0f32;
    for i in 0..16 {
        if three_components {
            lut.set_value(i, rgb / 1023.0, (rgb + 1.0) / 1023.0, (rgb + 2.0) / 1023.0);
        } else {
            let v = rgb / 1023.0;
            lut.set_value(i, v, v, v);
        }
        rgb += 3.0;
    }
    lut
}

const ARRAY_16X3: &str = "   0    1    2
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
";

#[test]
fn lut1d_array_16x1_ctf() {
    let group = group_with_id("UIDLUT42", lut1d_16(false));
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="16 1">
   0
   3
   6
   9
  12
  15
  18
  21
  24
  27
  30
  33
  36
  39
  42
  45
        </Array>
    </LUT1D>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn lut1d_array_16x3_ctf() {
    let group = group_with_id("UIDLUT42", lut1d_16(true));
    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="16 3">
{ARRAY_16X3}        </Array>
    </LUT1D>
</ProcessList>
"#
    );
    check_ctf(&group, &expected);
}

#[test]
fn lut1d_10i_ctf() {
    let mut lut = Lut1DTransform::new(3, false);
    lut.interpolation = Interpolation::Default;
    lut.metadata.add_attribute(METADATA_NAME, "test-lut");
    lut.metadata.add_attribute(METADATA_ID, "lut01");
    lut.file_output_bit_depth = BitDepth::UInt10;
    lut.set_value(1, 511.0 / 1023.0, 4011.12345 / 1023.0, -24.10297 / 1023.0);
    let group = group_with_id("UIDLUT42", lut);
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="3 3">
   0    0    0
 511 4011.12 -24.103
   1023    1023    1023
        </Array>
    </LUT1D>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn lut1d_inverse_ctf() {
    let mut lut = lut1d_16(true);
    lut.direction = TransformDirection::Inverse;
    let group = group_with_id("UIDLUT42", lut);
    // For an InverseLUT1D, the scaling of array values is based on the
    // inBitDepth.
    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <InverseLUT1D id="lut01" name="test-lut" inBitDepth="10i" outBitDepth="32f">
        <Array dim="16 3">
{ARRAY_16X3}        </Array>
    </InverseLUT1D>
</ProcessList>
"#
    );
    check_ctf(&group, &expected);
}

fn lut3d_3() -> Lut3DTransform {
    let mut lut = Lut3DTransform::new(3);
    lut.metadata.add_attribute(METADATA_NAME, "test-lut3d");
    lut.metadata.add_attribute(METADATA_ID, "lut01");
    lut.file_output_bit_depth = BitDepth::UInt10;
    let mut rgb = 0.0f32;
    for r in 0..3 {
        for g in 0..3 {
            for b in 0..3 {
                lut.set_value(
                    r,
                    g,
                    b,
                    [rgb / 1023.0, (rgb + 1.0) / 1023.0, (rgb + 2.0) / 1023.0],
                );
                rgb += 3.0;
            }
        }
    }
    lut
}

const ARRAY_27X3: &str = "   0    1    2
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
  48   49   50
  51   52   53
  54   55   56
  57   58   59
  60   61   62
  63   64   65
  66   67   68
  69   70   71
  72   73   74
  75   76   77
  78   79   80
";

#[test]
fn lut3d_array_ctf() {
    let mut lut = lut3d_3();
    lut.interpolation = Interpolation::Tetrahedral;
    let group = group_with_id("UIDLUT42", lut);
    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT3D id="lut01" name="test-lut3d" inBitDepth="32f" outBitDepth="10i" interpolation="tetrahedral">
        <Array dim="3 3 3 3">
{ARRAY_27X3}        </Array>
    </LUT3D>
</ProcessList>
"#
    );
    check_ctf(&group, &expected);
}

#[test]
fn lut3d_inverse_clf() {
    let mut lut = lut3d_3();
    lut.direction = TransformDirection::Inverse;
    let group = group_with_id("UIDLUT42", lut);
    check_err(
        write_group_clf(&group),
        "Transform uses the 'InverseLUT3D' op which cannot be written as CLF",
    );
}

#[test]
fn lut3d_inverse_ctf() {
    let mut lut = lut3d_3();
    lut.direction = TransformDirection::Inverse;
    let group = group_with_id("UIDLUT42", lut);
    // For an InverseLUT3D, the scaling of array values is based on the
    // inBitDepth.
    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.6" id="UIDLUT42">
    <InverseLUT3D id="lut01" name="test-lut3d" inBitDepth="10i" outBitDepth="32f">
        <Array dim="3 3 3 3">
{ARRAY_27X3}        </Array>
    </InverseLUT3D>
</ProcessList>
"#
    );
    check_ctf(&group, &expected);
}

#[test]
fn save_lut3d_interpolation() {
    let mut lut = Lut3DTransform::default();
    let write = |lut: &Lut3DTransform| {
        write_group_ctf(&GroupTransform::from_transforms(vec![Transform::Lut3D(
            lut.clone(),
        )]))
    };

    lut.interpolation = Interpolation::Default;
    assert!(write(&lut)
        .unwrap()
        .contains(r#"<LUT3D inBitDepth="32f" outBitDepth="32f">"#));

    // INTERP_BEST is not a valid CTF/CLF method, so write its concrete
    // value, which is tetrahedral.
    lut.interpolation = Interpolation::Best;
    assert!(write(&lut)
        .unwrap()
        .contains(r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="tetrahedral">"#));

    lut.interpolation = Interpolation::Linear;
    assert!(write(&lut)
        .unwrap()
        .contains(r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="trilinear">"#));

    lut.interpolation = Interpolation::Tetrahedral;
    assert!(write(&lut)
        .unwrap()
        .contains(r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="tetrahedral">"#));

    lut.interpolation = Interpolation::Cubic;
    check_err(
        write(&lut),
        "Lut3D does not support interpolation algorithm: cubic",
    );
}

#[test]
fn bitdepth_ctf() {
    let mut mat = MatrixTransform::default();
    mat.file_input_bit_depth = BitDepth::UInt8;
    mat.file_output_bit_depth = BitDepth::UInt10;

    let mut lut = Lut1DTransform::new(3, false);
    lut.interpolation = Interpolation::Default;
    lut.file_output_bit_depth = BitDepth::UInt10;

    let exp = ExponentTransform::default();

    let mut range = RangeTransform::new(Some(0.1), Some(0.9), Some(-0.1), Some(1.1));
    range.file_input_bit_depth = BitDepth::F16;
    range.file_output_bit_depth = BitDepth::UInt12;

    let log = LogTransform::default();

    let mut invlut = Lut1DTransform::new(3, false);
    invlut.direction = TransformDirection::Inverse;
    invlut.file_output_bit_depth = BitDepth::UInt16;

    let mut mat2 = MatrixTransform::default();
    mat2.file_input_bit_depth = BitDepth::UInt8;
    mat2.file_output_bit_depth = BitDepth::UInt10;
    mat2.direction = TransformDirection::Inverse;

    let mut group = GroupTransform::new();
    group.metadata.add_attribute(METADATA_ID, "UID42");
    // First op keeps its in & out bit-depth.
    group.transforms.push(Transform::Matrix(mat.clone()));
    // Previous op out bit-depth used for in bit-depth.
    group.transforms.push(Transform::Lut1D(lut));
    // Previous op out bit-depth used for in bit-depth, and next op (range)
    // in bit-depth used for out bit-depth.
    group.transforms.push(Transform::Exponent(exp));
    // In bit-depth preserved and has been used for out bit-depth of
    // previous op.
    group.transforms.push(Transform::Range(range));
    // Previous op out bit-depth used for in bit-depth.
    group.transforms.push(Transform::Matrix(mat));
    // Out depth is set by preference of the next op.
    group.transforms.push(Transform::Log(log));
    // Preferred in bit-depth is preserved. Out depth is set by the next op.
    group.transforms.push(Transform::Lut1D(invlut.clone()));
    // Sets both its preferred in and out depth (swapped as inverse).
    group.transforms.push(Transform::Matrix(mat2));
    // This time it doesn't get its preferred in depth, since the previous
    // op has priority. The array values are scaled accordingly.
    group.transforms.push(Transform::Lut1D(invlut));

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Matrix inBitDepth="8i" outBitDepth="10i">
        <Array dim="3 3">
   4.01176470588235                   0                   0
                  0    4.01176470588235                   0
                  0                   0    4.01176470588235
        </Array>
    </Matrix>
    <LUT1D inBitDepth="10i" outBitDepth="10i">
        <Array dim="3 1">
   0
511.5
 1023
        </Array>
    </LUT1D>
    <Exponent inBitDepth="10i" outBitDepth="16f" style="basicFwd">
        <ExponentParams exponent="1" />
    </Exponent>
    <Range inBitDepth="16f" outBitDepth="12i">
        <minInValue> 0.1 </minInValue>
        <maxInValue> 0.9 </maxInValue>
        <minOutValue> -409.5 </minOutValue>
        <maxOutValue> 4504.5 </maxOutValue>
    </Range>
    <Matrix inBitDepth="12i" outBitDepth="10i">
        <Array dim="3 3">
   0.24981684981685                   0                   0
                  0    0.24981684981685                   0
                  0                   0    0.24981684981685
        </Array>
    </Matrix>
    <Log inBitDepth="10i" outBitDepth="16i" style="log2">
    </Log>
    <InverseLUT1D inBitDepth="16i" outBitDepth="10i">
        <Array dim="3 1">
    0
32767.5
  65535
        </Array>
    </InverseLUT1D>
    <Matrix inBitDepth="10i" outBitDepth="8i">
        <Array dim="3 3">
  0.249266862170088                   0                   0
                  0   0.249266862170088                   0
                  0                   0   0.249266862170088
        </Array>
    </Matrix>
    <InverseLUT1D inBitDepth="8i" outBitDepth="32f">
        <Array dim="3 1">
  0
127.5
  255
        </Array>
    </InverseLUT1D>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

#[test]
fn no_ops_ctf() {
    let mut group = GroupTransform::new();
    group.metadata.add_attribute(METADATA_ID, "UIDEC42");
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <Matrix inBitDepth="32f" outBitDepth="32f">
        <Array dim="3 3 3">
                  1                   0                   0
                  0                   1                   0
                  0                   0                   1
        </Array>
    </Matrix>
</ProcessList>
"#;
    check_ctf(&group, expected);
}

// ---------------------------------------------------------------------------
// Baker tests. They need the baker helpers and the config (other modules).

fn bake(config_yaml: &str, set: impl FnOnce(&mut crate::Baker), format: &str) -> String {
    let config = crate::Config::create_from_str(config_yaml).unwrap();
    let mut baker = crate::Baker::new();
    baker.config = Some(config);
    baker.format = format.to_string();
    set(&mut baker);
    let bytes = super::create().bake(&baker, format).unwrap();
    String::from_utf8(bytes).unwrap()
}

const BAKE_1D_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: input
  reference: input

colorspaces:
  - !<ColorSpace>
    name: input
    family: input

  - !<ColorSpace>
    name: target
    family: target
"#;

#[test]
fn bake_1d() {
    let set = |b: &mut crate::Baker| {
        b.input_space = "input".to_string();
        b.target_space = "target".to_string();
        b.metadata.add_attribute(METADATA_ID, "UID42");
        b.cube_size = Some(2);
    };
    let out = bake(BAKE_1D_CONFIG, set, crate::fileformats::FILEFORMAT_CLF);
    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    assert_eq!(expected_clf, out);

    let out = bake(BAKE_1D_CONFIG, set, crate::fileformats::FILEFORMAT_CTF);
    let expected_ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    assert_eq!(expected_ctf, out);
}

const BAKE_SHAPER_CONFIG: &str = r#"
        ocio_profile_version: 1

        colorspaces:
        - !<ColorSpace>
          name : Raw
          isdata : false

        - !<ColorSpace>
          name: Log2
          isdata: false
          from_reference: !<GroupTransform>
            children:
              - !<MatrixTransform> {matrix: [5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 1]}
              - !<LogTransform> {base: 2}
              - !<MatrixTransform> {offset: [6.5, 6.5, 6.5, 0]}
              - !<MatrixTransform> {matrix: [0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 1]}
    "#;

#[test]
fn bake_1d_shaper() {
    {
        // Lin to Log.
        let out = bake(
            BAKE_SHAPER_CONFIG,
            |b| {
                b.input_space = "Raw".to_string();
                b.target_space = "Log2".to_string();
                b.shaper_space = "Log2".to_string();
                b.metadata.add_attribute(METADATA_ID, "UID42");
                b.cube_size = Some(10);
            },
            crate::fileformats::FILEFORMAT_CLF,
        );
        let t = parse(&out).unwrap();
        assert_eq!(t.ops.len(), 2);
        let r = as_range(&t.ops[0]);
        assert_eq!(r.min_in, 0.00198873621411622);
        assert_eq!(r.max_in, 16.291877746582);
        assert_eq!(r.min_out, 0.0);
        assert_eq!(r.max_out, 1.0);
        let l = as_lut1d(&t.ops[1]);
        assert_eq!(l.length, 10);
        let expected = [
            0.0, 0.7562682, 0.83313024, 0.87810701, 0.9100228, 0.93478036, 0.9550097, 0.97211391,
            0.98693061, 1.0,
        ];
        for (i, e) in expected.iter().enumerate() {
            assert!((l.values[3 * i] - e).abs() <= 1e-5);
        }
    }
    {
        // Log to Lin.
        let out = bake(
            BAKE_SHAPER_CONFIG,
            |b| {
                b.input_space = "Log2".to_string();
                b.target_space = "Raw".to_string();
                b.metadata.add_attribute(METADATA_ID, "UID42");
                b.cube_size = Some(10);
            },
            crate::fileformats::FILEFORMAT_CLF,
        );
        let t = parse(&out).unwrap();
        assert_eq!(t.ops.len(), 1);
        let l = as_lut1d(&t.ops[0]);
        assert_eq!(l.length, 10);
        let expected = [
            0.0019887362,
            0.0054125111,
            0.014730596,
            0.040090535,
            0.10910972,
            0.29695117,
            0.80817699,
            2.1995215,
            5.9861789,
            16.291878,
        ];
        for (i, e) in expected.iter().enumerate() {
            assert!((l.values[3 * i] - e).abs() <= 1e-5);
        }
    }
}

const BAKE_3D_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: input
  reference: input

colorspaces:
  - !<ColorSpace>
    name: input
    family: input

  - !<ColorSpace>
    name: target
    family: target
    from_scene_reference: !<CDLTransform> {sat: 0.5}
"#;

#[test]
fn bake_3d() {
    let out = bake(
        BAKE_3D_CONFIG,
        |b| {
            let data = &mut b.metadata;
            data.add_attribute(METADATA_ID, "TestID");
            data.add_child_element(METADATA_DESCRIPTION, "OpenColorIO Test Line 1");
            data.add_child_element(METADATA_DESCRIPTION, "OpenColorIO Test Line 2");
            data.add_child_element("Anything", "Not Saved");
            data.add_child_element(METADATA_INPUT_DESCRIPTOR, "Input descriptor 1");
            data.add_child_element(METADATA_INPUT_DESCRIPTOR, "Input descriptor 2");
            data.add_child_element(METADATA_OUTPUT_DESCRIPTOR, "Output descriptor 1");
            data.add_child_element(METADATA_OUTPUT_DESCRIPTOR, "Output descriptor 2");
            let info = data.add_child_element(METADATA_INFO, "");
            info.add_attribute("attrib1", "val1");
            info.add_attribute("attrib2", "val2");
            info.add_child_element("anything", "is saved");
            info.add_child_element("anything", "is also saved");
            b.input_space = "input".to_string();
            b.target_space = "target".to_string();
            b.cube_size = Some(2);
        },
        crate::fileformats::FILEFORMAT_CLF,
    );
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="TestID">
    <Description>OpenColorIO Test Line 1</Description>
    <Description>OpenColorIO Test Line 2</Description>
    <InputDescriptor>Input descriptor 1</InputDescriptor>
    <InputDescriptor>Input descriptor 2</InputDescriptor>
    <OutputDescriptor>Output descriptor 1</OutputDescriptor>
    <OutputDescriptor>Output descriptor 2</OutputDescriptor>
    <Info attrib1="val1" attrib2="val2">
        <anything>is saved</anything>
        <anything>is also saved</anything>
    </Info>
    <LUT3D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 2 2 3">
          0           0           0
     0.0361      0.0361  0.53609997
     0.3576  0.85759997      0.3576
     0.3937      0.8937      0.8937
     0.6063      0.1063      0.1063
 0.64240003      0.1424  0.64239997
 0.96389997  0.96389997      0.4639
          1           1           1
        </Array>
    </LUT3D>
</ProcessList>
"#;
    assert_eq!(expected, out);
}

const BAKE_1D_3D_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: input
  reference: input

colorspaces:
  - !<ColorSpace>
    name: input
    family: input

  - !<ColorSpace>
    name: shaper
    family: shaper
    from_scene_reference: !<MatrixTransform> {matrix: [0.8, 0, 0, 0, 0, 0.8, 0, 0, 0, 0, 0.8, 0, 0, 0, 0, 1], offset: [0.1, 0.1, 0.1, 0]}

  - !<ColorSpace>
    name: target
    family: target
    from_scene_reference: !<CDLTransform> {sat: 0.5, style: asc}
"#;

#[test]
fn bake_1d_3d() {
    let set = |b: &mut crate::Baker| {
        b.metadata.add_attribute(METADATA_ID, "UID42");
        b.input_space = "input".to_string();
        b.shaper_space = "shaper".to_string();
        b.target_space = "target".to_string();
        b.cube_size = Some(2);
    };
    let out = bake(BAKE_1D_3D_CONFIG, set, crate::fileformats::FILEFORMAT_CLF);
    let t = parse(&out).unwrap();
    assert_eq!(t.ops.len(), 2);
    let shaper = as_lut1d(&t.ops[0]);
    assert!(shaper.half_domain);
    // The index for 0.5 in a half-domain LUT1D.
    let index = usize::from(half::f16::from_f32(0.5).to_bits()) * 3;
    let res = 0.5f32 * 0.8 + 0.1;
    assert!((shaper.values[index] - res).abs() <= 1e-5);
    assert_eq!(shaper.values[index], shaper.values[index + 1]);
    assert_eq!(shaper.values[index], shaper.values[index + 2]);

    let lut = as_lut3d(&t.ops[1]);
    assert_eq!(lut.grid_size, 2);
    let expected = [
        0.0, 0.0, 0.0, 0.0361, 0.0361, 0.5361, 0.3576, 0.8576, 0.3576, 0.3937, 0.8937, 0.8937,
        0.6063, 0.1063, 0.1063, 0.6424, 0.1424, 0.6424, 0.9639, 0.9639, 0.4639, 1.0, 1.0, 1.0,
    ];
    for (i, e) in expected.iter().enumerate() {
        assert!((lut.values[i] - e).abs() <= 1e-5, "index {i}");
    }

    let out = bake(
        BAKE_1D_3D_CONFIG,
        |b| {
            set(b);
            b.shaper_size = Some(10);
        },
        crate::fileformats::FILEFORMAT_CLF,
    );
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" xmlns="http://www.smpte-ra.org/ns/2136-1/2024" id="UID42">
    <Range inBitDepth="32f" outBitDepth="32f">
        <minInValue> -0.125 </minInValue>
        <maxInValue> 1.125 </maxInValue>
        <minOutValue> 0 </minOutValue>
        <maxOutValue> 1 </maxOutValue>
    </Range>
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="10 1">
          0
 0.11111112
 0.22222224
 0.33333334
 0.44444448
 0.55555558
 0.66666675
 0.77777779
 0.88888896
          1
        </Array>
    </LUT1D>
    <LUT3D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 2 2 3">
          0           0           0
     0.0361      0.0361  0.53609997
     0.3576  0.85759997      0.3576
     0.3937      0.8937      0.8937
     0.6063      0.1063      0.1063
 0.64240003      0.1424  0.64239997
 0.96389997  0.96389997      0.4639
          1           1           1
        </Array>
    </LUT3D>
</ProcessList>
"#;
    let out_lines: Vec<&str> = out.lines().collect();
    let exp_lines: Vec<&str> = expected.lines().collect();
    assert_eq!(out_lines.len(), exp_lines.len());
    for (i, (o, e)) in out_lines.iter().zip(exp_lines.iter()).enumerate() {
        if (10..=19).contains(&i) || (24..=31).contains(&i) {
            let ov: Vec<f32> = o.split_whitespace().map(|v| v.parse().unwrap()).collect();
            let ev: Vec<f32> = e.split_whitespace().map(|v| v.parse().unwrap()).collect();
            assert_eq!(ov.len(), ev.len());
            for (a, b) in ov.iter().zip(ev.iter()) {
                assert!((a - b).abs() <= 1e-5, "line {i}: {o} vs {e}");
            }
        } else {
            assert_eq!(o, e);
        }
    }
}

#[test]
fn lut_interpolation_option() {
    // CLF/CTF is different from other formats because the syntax allows
    // specifying an interpolation in the file itself: the FileTransform
    // interpolation is used for LUTs without an interpolation (if valid).
    let read = |name: &str, interp: Interpolation| -> Lut3DTransform {
        let p = test_path(name);
        let data = std::fs::read(&p).unwrap();
        let group = super::create().read(&data, &p, interp).unwrap().group;
        assert_eq!(group.transforms.len(), 1);
        match &group.transforms[0] {
            Transform::Lut3D(l) => l.clone(),
            _ => panic!("expected a 3D LUT"),
        }
    };
    // CLF file containing a LUT3D that does not specify an interpolation.
    let f = "clf/lut3d_17x17x17_10i_12i.clf";
    assert_eq!(
        read(f, Interpolation::Default).interpolation,
        Interpolation::Default
    );
    assert_eq!(
        read(f, Interpolation::Best).interpolation,
        Interpolation::Best
    );
    // If the FileTransform interpolation is not supported by the LUT, it is
    // ignored and default interpolation is used.
    assert_eq!(
        read(f, Interpolation::Cubic).interpolation,
        Interpolation::Default
    );

    // CTF file containing a LUT3D that specifies tetrahedral interpolation:
    // whatever the file transform interpolation, LUT keeps its interpolation.
    let f = "lut3d_example_Inv.ctf";
    assert_eq!(
        read(f, Interpolation::Default).interpolation,
        Interpolation::Tetrahedral
    );
    assert_eq!(
        read(f, Interpolation::Linear).interpolation,
        Interpolation::Tetrahedral
    );
}

#[test]
fn lut_interpolation_option_processor() {
    let config = crate::Config::create_raw();
    let get = |src: &str, interp: Interpolation| -> Interpolation {
        let ft = FileTransform {
            src: test_path(src),
            interpolation: interp,
            ..Default::default()
        };
        let proc = config
            .get_processor_for_transform(&Transform::File(ft), TransformDirection::Forward)
            .unwrap();
        let group = proc.create_group_transform();
        assert_eq!(group.transforms.len(), 1);
        match &group.transforms[0] {
            Transform::Lut3D(l) => l.interpolation,
            _ => panic!("expected a 3D LUT"),
        }
    };
    let f = "clf/lut3d_17x17x17_10i_12i.clf";
    assert_eq!(get(f, Interpolation::Default), Interpolation::Default);
    assert_eq!(get(f, Interpolation::Best), Interpolation::Best);
    assert_eq!(get(f, Interpolation::Cubic), Interpolation::Default);
    let f = "lut3d_example_Inv.ctf";
    assert_eq!(get(f, Interpolation::Default), Interpolation::Tetrahedral);
    assert_eq!(get(f, Interpolation::Linear), Interpolation::Tetrahedral);
}
