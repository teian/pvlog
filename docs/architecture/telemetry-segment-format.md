# Telemetry segment format

PVLog archives each stable system-day as a deterministic
`pvlog.telemetry.segment.v1.SegmentEnvelope`, defined in
`src/crates/storage/proto/telemetry_segment_v1.proto`.

## Canonical encoding

- `schema_version` is `1`; readers dispatch on this value and must reject an
  unsupported version without interpreting its payload.
- UUIDs are exactly 16 bytes. Timestamps are UTC epoch milliseconds. The first
  timestamp is the base and subsequent timestamps are signed deltas.
- Nullable integer series use an LSB-first presence bitmap and zigzag-encoded
  value deltas. Unused bitmap bits are zero.
- Standard columns are ordered by numeric `Field`. Extended columns are ordered
  by raw channel UUID bytes. Protobuf `map` fields are forbidden.
- Provenance dictionary entries are ordered by first occurrence; each row has
  one dictionary index and one quality bit set.
- Writers use the deterministic Protobuf serializer and emit no unknown fields.

## Compression and integrity

The uncompressed envelope is compressed as one Zstandard frame with level 9,
checksum enabled, content size enabled, no dictionary, and no long-distance
matching. Storage metadata records schema version, row count, UTC range,
compressed length, uncompressed length, and a 32-byte BLAKE3 hash of the exact
uncompressed Protobuf bytes. Readers verify lengths and hash before decoding.

Released readers retain version dispatch and golden fixtures. A future encoder
uses a new schema package/version; it never changes v1 field numbers or
canonical ordering rules in place.
