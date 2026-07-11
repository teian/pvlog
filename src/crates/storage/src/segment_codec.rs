//! Deterministic telemetry segment v1 encoding and verified decoding.

use prost::Message;
use std::{collections::BTreeMap, io::Cursor};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentPoint {
    pub timestamp_epoch_millis: i64,
    pub generation_power_watts: Option<i64>,
    pub extended: BTreeMap<Uuid, i64>,
    pub source_kind: String,
    pub source_reference: String,
    pub received_at_epoch_millis: i64,
    pub quality_flags: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivedSegmentBytes {
    pub schema_version: u16,
    pub row_count: u32,
    pub uncompressed_length: u64,
    pub compressed_length: u64,
    pub content_hash: [u8; 32],
    pub compressed_bytes: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct Envelope {
    #[prost(uint32, tag = "1")]
    schema_version: u32,
    #[prost(bytes = "vec", tag = "2")]
    system_id_uuid: Vec<u8>,
    #[prost(sint64, tag = "3")]
    range_start: i64,
    #[prost(sint64, tag = "4")]
    range_end: i64,
    #[prost(uint32, tag = "5")]
    row_count: u32,
    #[prost(sint64, tag = "6")]
    base_timestamp: i64,
    #[prost(sint64, repeated, packed = "true", tag = "7")]
    timestamp_deltas: Vec<i64>,
    #[prost(message, repeated, tag = "8")]
    columns: Vec<IntegerColumn>,
    #[prost(message, repeated, tag = "9")]
    extended: Vec<ExtendedColumn>,
    #[prost(message, repeated, tag = "10")]
    provenance: Vec<Provenance>,
    #[prost(uint32, repeated, packed = "true", tag = "11")]
    provenance_index: Vec<u32>,
    #[prost(uint32, repeated, packed = "true", tag = "12")]
    quality_flags: Vec<u32>,
}
#[derive(Clone, PartialEq, Message)]
struct IntegerColumn {
    #[prost(int32, tag = "1")]
    field: i32,
    #[prost(bytes = "vec", tag = "2")]
    presence: Vec<u8>,
    #[prost(sint32, tag = "3")]
    scale: i32,
    #[prost(sint64, repeated, packed = "true", tag = "4")]
    deltas: Vec<i64>,
}
#[derive(Clone, PartialEq, Message)]
struct ExtendedColumn {
    #[prost(bytes = "vec", tag = "1")]
    channel_id: Vec<u8>,
    #[prost(int32, tag = "2")]
    value_type: i32,
    #[prost(bytes = "vec", tag = "3")]
    presence: Vec<u8>,
    #[prost(sint32, tag = "4")]
    scale: i32,
    #[prost(sint64, repeated, packed = "true", tag = "5")]
    deltas: Vec<i64>,
    #[prost(bytes = "vec", tag = "6")]
    booleans: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Provenance {
    #[prost(string, tag = "1")]
    source_kind: String,
    #[prost(string, tag = "2")]
    source_reference: String,
    #[prost(sint64, tag = "3")]
    received_delta: i64,
}

/// Encodes one ordered system interval into deterministic Protobuf and Zstandard bytes.
/// # Errors
/// Returns an error for empty/unsorted input, overflow, oversized rows, or compression failure.
#[allow(clippy::too_many_lines)]
pub fn encode_segment_v1(
    system_id: Uuid,
    points: &[SegmentPoint],
) -> Result<ArchivedSegmentBytes, SegmentCodecError> {
    if points.is_empty()
        || points
            .windows(2)
            .any(|pair| pair[0].timestamp_epoch_millis >= pair[1].timestamp_epoch_millis)
    {
        return Err(SegmentCodecError::Ordering);
    }
    let row_count = u32::try_from(points.len()).map_err(|_| SegmentCodecError::Size)?;
    let base = points[0].timestamp_epoch_millis;
    let timestamp_deltas = points
        .windows(2)
        .map(|pair| {
            pair[1]
                .timestamp_epoch_millis
                .checked_sub(pair[0].timestamp_epoch_millis)
                .ok_or(SegmentCodecError::Overflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let generation = column(points.iter().map(|point| point.generation_power_watts));
    let columns = (!generation.0.iter().all(|byte| *byte == 0))
        .then_some(IntegerColumn {
            field: 1,
            presence: generation.0,
            scale: 0,
            deltas: generation.1,
        })
        .into_iter()
        .collect();
    let mut channel_ids = points
        .iter()
        .flat_map(|point| point.extended.keys().copied())
        .collect::<Vec<_>>();
    channel_ids.sort_unstable_by_key(|channel| channel.into_bytes());
    channel_ids.dedup();
    let extended = channel_ids
        .into_iter()
        .map(|channel| {
            let (presence, deltas) = column(
                points
                    .iter()
                    .map(|point| point.extended.get(&channel).copied()),
            );
            ExtendedColumn {
                channel_id: channel.as_bytes().to_vec(),
                value_type: 1,
                presence,
                scale: 0,
                deltas,
                booleans: Vec::new(),
            }
        })
        .collect();
    let mut provenance = Vec::<Provenance>::new();
    let mut provenance_index = Vec::with_capacity(points.len());
    for point in points {
        let entry = Provenance {
            source_kind: point.source_kind.clone(),
            source_reference: point.source_reference.clone(),
            received_delta: point
                .received_at_epoch_millis
                .checked_sub(point.timestamp_epoch_millis)
                .ok_or(SegmentCodecError::Overflow)?,
        };
        let index = provenance
            .iter()
            .position(|candidate| candidate == &entry)
            .unwrap_or_else(|| {
                provenance.push(entry);
                provenance.len() - 1
            });
        provenance_index.push(u32::try_from(index).map_err(|_| SegmentCodecError::Size)?);
    }
    let envelope = Envelope {
        schema_version: 1,
        system_id_uuid: system_id.as_bytes().to_vec(),
        range_start: base,
        range_end: points
            .last()
            .and_then(|point| point.timestamp_epoch_millis.checked_add(1))
            .ok_or(SegmentCodecError::Overflow)?,
        row_count,
        base_timestamp: base,
        timestamp_deltas,
        columns,
        extended,
        provenance,
        provenance_index,
        quality_flags: points.iter().map(|point| point.quality_flags).collect(),
    };
    let uncompressed = envelope.encode_to_vec();
    let content_hash = *blake3::hash(&uncompressed).as_bytes();
    let mut encoder =
        zstd::stream::Encoder::new(Vec::new(), 9).map_err(|_| SegmentCodecError::Compression)?;
    encoder
        .include_checksum(true)
        .map_err(|_| SegmentCodecError::Compression)?;
    encoder
        .write_all(&uncompressed)
        .map_err(|_| SegmentCodecError::Compression)?;
    let compressed_bytes = encoder
        .finish()
        .map_err(|_| SegmentCodecError::Compression)?;
    Ok(ArchivedSegmentBytes {
        schema_version: 1,
        row_count,
        uncompressed_length: u64::try_from(uncompressed.len())
            .map_err(|_| SegmentCodecError::Size)?,
        compressed_length: u64::try_from(compressed_bytes.len())
            .map_err(|_| SegmentCodecError::Size)?,
        content_hash,
        compressed_bytes,
    })
}

/// Verifies and decodes a telemetry segment v1 artifact.
/// # Errors
/// Returns an error for length/hash corruption, unsupported versions, malformed protobuf, or inconsistent columns.
pub fn decode_segment_v1(
    artifact: &ArchivedSegmentBytes,
) -> Result<(Uuid, Vec<SegmentPoint>), SegmentCodecError> {
    if artifact.schema_version != 1
        || usize::try_from(artifact.compressed_length).ok() != Some(artifact.compressed_bytes.len())
    {
        return Err(SegmentCodecError::Integrity);
    }
    let uncompressed = zstd::stream::decode_all(Cursor::new(&artifact.compressed_bytes))
        .map_err(|_| SegmentCodecError::Compression)?;
    if usize::try_from(artifact.uncompressed_length).ok() != Some(uncompressed.len())
        || *blake3::hash(&uncompressed).as_bytes() != artifact.content_hash
    {
        return Err(SegmentCodecError::Integrity);
    }
    let envelope =
        Envelope::decode(uncompressed.as_slice()).map_err(|_| SegmentCodecError::Protobuf)?;
    if envelope.schema_version != 1 || envelope.row_count != artifact.row_count {
        return Err(SegmentCodecError::Integrity);
    }
    let system_id =
        Uuid::from_slice(&envelope.system_id_uuid).map_err(|_| SegmentCodecError::Protobuf)?;
    let count = usize::try_from(envelope.row_count).map_err(|_| SegmentCodecError::Size)?;
    if count == 0
        || envelope.timestamp_deltas.len() + 1 != count
        || envelope.provenance_index.len() != count
        || envelope.quality_flags.len() != count
    {
        return Err(SegmentCodecError::Integrity);
    }
    let mut timestamps = Vec::with_capacity(count);
    timestamps.push(envelope.base_timestamp);
    for delta in envelope.timestamp_deltas {
        timestamps.push(
            timestamps
                .last()
                .and_then(|timestamp| timestamp.checked_add(delta))
                .ok_or(SegmentCodecError::Overflow)?,
        );
    }
    let generation = envelope
        .columns
        .iter()
        .find(|column| column.field == 1)
        .map(|column| expand(&column.presence, &column.deltas, count))
        .transpose()?
        .unwrap_or_else(|| vec![None; count]);
    let mut extended_rows = vec![BTreeMap::new(); count];
    for column in envelope.extended {
        let channel =
            Uuid::from_slice(&column.channel_id).map_err(|_| SegmentCodecError::Protobuf)?;
        for (index, value) in expand(&column.presence, &column.deltas, count)?
            .into_iter()
            .enumerate()
        {
            if let Some(value) = value {
                extended_rows[index].insert(channel, value);
            }
        }
    }
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let provenance = envelope
            .provenance
            .get(
                usize::try_from(envelope.provenance_index[index])
                    .map_err(|_| SegmentCodecError::Size)?,
            )
            .ok_or(SegmentCodecError::Integrity)?;
        points.push(SegmentPoint {
            timestamp_epoch_millis: timestamps[index],
            generation_power_watts: generation[index],
            extended: std::mem::take(&mut extended_rows[index]),
            source_kind: provenance.source_kind.clone(),
            source_reference: provenance.source_reference.clone(),
            received_at_epoch_millis: timestamps[index]
                .checked_add(provenance.received_delta)
                .ok_or(SegmentCodecError::Overflow)?,
            quality_flags: envelope.quality_flags[index],
        });
    }
    Ok((system_id, points))
}

fn column(values: impl Iterator<Item = Option<i64>>) -> (Vec<u8>, Vec<i64>) {
    let values = values.collect::<Vec<_>>();
    let mut presence = vec![0; values.len().div_ceil(8)];
    let mut deltas = Vec::new();
    let mut previous = 0;
    for (index, value) in values.into_iter().enumerate() {
        if let Some(value) = value {
            presence[index / 8] |= 1 << (index % 8);
            deltas.push(value - previous);
            previous = value;
        }
    }
    (presence, deltas)
}
fn expand(
    presence: &[u8],
    deltas: &[i64],
    count: usize,
) -> Result<Vec<Option<i64>>, SegmentCodecError> {
    if presence.len() != count.div_ceil(8) {
        return Err(SegmentCodecError::Integrity);
    }
    let mut values = Vec::with_capacity(count);
    let mut delta_index = 0;
    let mut previous = 0_i64;
    for index in 0..count {
        if presence[index / 8] & (1 << (index % 8)) == 0 {
            values.push(None);
        } else {
            let delta = *deltas
                .get(delta_index)
                .ok_or(SegmentCodecError::Integrity)?;
            previous = previous
                .checked_add(delta)
                .ok_or(SegmentCodecError::Overflow)?;
            values.push(Some(previous));
            delta_index += 1;
        }
    }
    if delta_index != deltas.len() {
        return Err(SegmentCodecError::Integrity);
    }
    Ok(values)
}

use std::io::Write as _;
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SegmentCodecError {
    #[error("segment points must be non-empty and strictly ordered")]
    Ordering,
    #[error("segment size is unsupported")]
    Size,
    #[error("segment arithmetic overflowed")]
    Overflow,
    #[error("segment compression failed")]
    Compression,
    #[error("segment protobuf is malformed")]
    Protobuf,
    #[error("segment integrity verification failed")]
    Integrity,
}
