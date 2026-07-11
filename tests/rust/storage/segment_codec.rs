use pvlog_storage::{SegmentCodecError, SegmentPoint, decode_segment_v1, encode_segment_v1};
use std::{collections::BTreeMap, error::Error, str::FromStr as _};
use uuid::Uuid;

#[test]
fn segment_v1_is_deterministic_sparse_extended_and_corruption_safe() -> Result<(), Box<dyn Error>> {
    let system = Uuid::from_str("019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70")?;
    let channel = Uuid::from_str("019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71")?;
    let points = vec![
        point(1_000, Some(10), BTreeMap::new()),
        point(1_300, None, BTreeMap::from([(channel, 7)])),
        point(1_900, Some(15), BTreeMap::from([(channel, 9)])),
    ];
    let first = encode_segment_v1(system, &points)?;
    let second = encode_segment_v1(system, &points)?;
    assert_eq!(first, second);
    assert_eq!(decode_segment_v1(&first)?, (system, points));
    assert_eq!(
        hex(&first.compressed_bytes),
        include_str!("../../fixtures/segments/v1-basic.hex").trim()
    );
    let mut corrupt = first;
    corrupt.compressed_bytes[3] ^= 0xff;
    assert!(matches!(
        decode_segment_v1(&corrupt),
        Err(SegmentCodecError::Compression | SegmentCodecError::Integrity)
    ));
    for count in 1..64 {
        let generated = (0..count)
            .map(|index| {
                point(
                    10_000 + i64::from(index) * 300,
                    (index % 3 != 0).then_some(i64::from(index) - 20),
                    BTreeMap::new(),
                )
            })
            .collect::<Vec<_>>();
        let encoded = encode_segment_v1(system, &generated)?;
        assert_eq!(decode_segment_v1(&encoded)?.1, generated);
    }
    Ok(())
}
fn point(timestamp: i64, generation: Option<i64>, extended: BTreeMap<Uuid, i64>) -> SegmentPoint {
    SegmentPoint {
        timestamp_epoch_millis: timestamp,
        generation_power_watts: generation,
        extended,
        source_kind: "modern_api".to_owned(),
        source_reference: "fixture".to_owned(),
        received_at_epoch_millis: timestamp + 5,
        quality_flags: 0,
    }
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)] as char,
                DIGITS[usize::from(byte & 15)] as char,
            ]
        })
        .collect()
}
