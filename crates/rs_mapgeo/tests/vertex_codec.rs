use rs_mapgeo::{ElementFormat, ElementName, VertexDescription, VertexElement, VertexUsage};

fn layout(format: ElementFormat) -> VertexDescription {
    VertexDescription {
        usage: VertexUsage::Static,
        elements: vec![VertexElement {
            name: ElementName::Texcoord0,
            format,
        }],
    }
}

fn bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

#[test]
fn half_attributes_use_signed_float_values_not_normalized_integers() {
    for (format, words, expected) in [
        (
            ElementFormat::XyPacked1616,
            vec![0xbc00, 0x4000],
            vec![-1.0, 2.0],
        ),
        (
            ElementFormat::XyzPacked161616,
            vec![0xbc00, 0x3800, 0x4000, 0xdead],
            vec![-1.0, 0.5, 2.0],
        ),
        (
            ElementFormat::XyzwPacked16161616,
            vec![0xbc00, 0x3800, 0x4000, 0x4400],
            vec![-1.0, 0.5, 2.0, 4.0],
        ),
    ] {
        let description = layout(format);
        let raw = bytes(&words);
        assert_eq!(
            description
                .read_attribute(&raw, ElementName::Texcoord0, 1)
                .unwrap()
                .unwrap(),
            expected
        );
        let mut encoded = raw.clone();
        encoded[..expected.len() * 2].fill(0);
        description
            .write_attribute(&mut encoded, ElementName::Texcoord0, &expected)
            .unwrap();
        assert_eq!(encoded, raw, "{format:?}: components and padding");
    }
}

#[test]
fn half_codec_handles_special_values_and_subnormals() {
    let description = layout(ElementFormat::XyPacked1616);
    let raw = bytes(&[0x0001, 0x8000, 0x7c00, 0xfc00, 0x7e00, 0x7bff]);
    let decoded = description
        .read_attribute(&raw, ElementName::Texcoord0, 3)
        .unwrap()
        .unwrap();
    assert_eq!(decoded[0], 2.0f32.powi(-24));
    assert_eq!(decoded[1].to_bits(), (-0.0f32).to_bits());
    assert_eq!(decoded[2], f32::INFINITY);
    assert_eq!(decoded[3], f32::NEG_INFINITY);
    assert!(decoded[4].is_nan());
    assert_eq!(decoded[5], 65504.0);
    let mut encoded = vec![0; raw.len()];
    description
        .write_attribute(&mut encoded, ElementName::Texcoord0, &decoded)
        .unwrap();
    assert_eq!(encoded, raw);
}

#[test]
fn half_encoding_rounds_to_nearest_even() {
    let description = layout(ElementFormat::XyPacked1616);
    let mut encoded = vec![0; 4];
    description
        .write_attribute(
            &mut encoded,
            ElementName::Texcoord0,
            &[1.0 + 2.0f32.powi(-11), 1.0 + 3.0 * 2.0f32.powi(-11)],
        )
        .unwrap();
    assert_eq!(encoded, bytes(&[0x3c00, 0x3c02]));
}

#[test]
fn packed_byte_colors_keep_their_normalized_swizzle() {
    let description = layout(ElementFormat::BgraPacked8888);
    let raw = [255, 0, 128, 255];
    let values = description
        .read_attribute(&raw, ElementName::Texcoord0, 1)
        .unwrap()
        .unwrap();
    assert_eq!(values, [128.0 / 255.0, 0.0, 1.0, 1.0]);
    let mut encoded = [0; 4];
    description
        .write_attribute(&mut encoded, ElementName::Texcoord0, &values)
        .unwrap();
    assert_eq!(encoded, raw);
}

#[test]
fn attribute_reads_reject_truncated_or_overflowing_ranges() {
    let description = layout(ElementFormat::XyPacked1616);
    assert!(
        description
            .read_attribute(&[0; 3], ElementName::Texcoord0, 1)
            .is_err()
    );
    assert!(
        description
            .read_attribute(&[], ElementName::Texcoord0, usize::MAX)
            .is_err()
    );
    assert_eq!(
        description
            .read_attribute(&[], ElementName::Texcoord0, 0)
            .unwrap(),
        Some(vec![])
    );
}
