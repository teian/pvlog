use pvlog_storage::{
    MergedReadError, RawObservation, RawObservationOrigin, SegmentPoint, decode_segment_v1,
    merge_raw_observations,
};
use std::{collections::BTreeMap, error::Error, io::Cursor};
use uuid::Uuid;

#[test]
fn merged_reads_are_ordered_deduplicated_and_preserve_quality() -> Result<(), Box<dyn Error>> {
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    let removed = Uuid::from_u128(3);
    let segment = vec![
        observation(first, 200, 10, 1, RawObservationOrigin::Segment),
        observation(removed, 300, 30, 2, RawObservationOrigin::Segment),
    ];
    let hot = vec![observation(first, 150, 15, 4, RawObservationOrigin::Hot)];
    let mut corrected = observation(second, 100, 20, 8, RawObservationOrigin::Overlay);
    corrected.revision = 2;
    let mut tombstone = observation(removed, 300, 0, 0, RawObservationOrigin::Overlay);
    tombstone.deleted = true;

    let merged = merge_raw_observations(segment, hot, [corrected, tombstone])?;
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].observation_id, second);
    assert_eq!(merged[0].quality_flags, 8);
    assert_eq!(merged[1].observation_id, first);
    assert_eq!(merged[1].generation_power_watts, Some(15));
    assert_eq!(merged[1].origin, RawObservationOrigin::Hot);
    Ok(())
}

#[test]
fn old_version_fixture_is_read_and_future_versions_are_rejected() -> Result<(), Box<dyn Error>> {
    let compressed = decode_hex(include_str!("../../fixtures/segments/v1-basic.hex").trim())?;
    let uncompressed = zstd::stream::decode_all(Cursor::new(&compressed))?;
    let archived = pvlog_storage::ArchivedSegmentBytes {
        schema_version: 1,
        row_count: 3,
        uncompressed_length: u64::try_from(uncompressed.len())?,
        compressed_length: u64::try_from(compressed.len())?,
        content_hash: *blake3::hash(&uncompressed).as_bytes(),
        compressed_bytes: compressed,
    };
    // The fixture is already covered byte-for-byte by the codec test. Its version is also accepted
    // by the merged reader when materialized as archived raw data.
    let (_, mut decoded) = decode_segment_v1(&archived)?;
    let point = decoded.remove(0);
    assert_eq!(point.generation_power_watts, Some(10));
    let accepted = from_segment_point(Uuid::from_u128(9), point, archived.schema_version);
    assert_eq!(merge_raw_observations([accepted], [], [])?.len(), 1);

    let future = from_segment_point(
        Uuid::from_u128(10),
        SegmentPoint {
            timestamp_epoch_millis: 2_000,
            generation_power_watts: None,
            extended: BTreeMap::new(),
            source_kind: "fixture".into(),
            source_reference: "future".into(),
            received_at_epoch_millis: 2_000,
            quality_flags: 0,
        },
        2,
    );
    assert_eq!(
        merge_raw_observations([future], [], []),
        Err(MergedReadError::UnsupportedSegmentVersion(Some(2)))
    );
    Ok(())
}

fn observation(
    id: Uuid,
    timestamp: i64,
    watts: i64,
    quality: u32,
    origin: RawObservationOrigin,
) -> RawObservation {
    RawObservation {
        observation_id: id,
        timestamp_epoch_millis: timestamp,
        generation_power_watts: Some(watts),
        extended: BTreeMap::new(),
        source_kind: "fixture".into(),
        source_reference: "test".into(),
        received_at_epoch_millis: timestamp,
        quality_flags: quality,
        revision: 1,
        deleted: false,
        origin,
        segment_version: (origin == RawObservationOrigin::Segment).then_some(1),
    }
}

fn from_segment_point(id: Uuid, point: SegmentPoint, version: u16) -> RawObservation {
    RawObservation {
        observation_id: id,
        timestamp_epoch_millis: point.timestamp_epoch_millis,
        generation_power_watts: point.generation_power_watts,
        extended: point.extended,
        source_kind: point.source_kind,
        source_reference: point.source_reference,
        received_at_epoch_millis: point.received_at_epoch_millis,
        quality_flags: point.quality_flags,
        revision: 1,
        deleted: false,
        origin: RawObservationOrigin::Segment,
        segment_version: Some(version),
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd fixture hex length".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&value[index..index + 2], 16)?))
        .collect()
}
