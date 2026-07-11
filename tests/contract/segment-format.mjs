import { readFileSync } from "node:fs";

const proto = readFileSync(
  "src/crates/storage/proto/telemetry_segment_v1.proto",
  "utf8",
);
const docs = readFileSync(
  "docs/architecture/telemetry-segment-format.md",
  "utf8",
);
for (const required of [
  "schema_version = 1",
  "timestamp_delta_millis = 7",
  "presence_bitmap",
  "provenance_index",
  "quality_flags",
]) {
  if (!proto.includes(required))
    throw new Error(`segment schema is missing ${required}`);
}
if (/\bmap\s*</u.test(proto))
  throw new Error("deterministic segment schemas must not use protobuf maps");
for (const required of [
  "Zstandard frame with level 9",
  "BLAKE3",
  "uncompressed length",
  "deterministic Protobuf serializer",
]) {
  if (!docs.includes(required))
    throw new Error(`segment documentation is missing ${required}`);
}
console.log("Telemetry segment v1 schema contract is complete");
