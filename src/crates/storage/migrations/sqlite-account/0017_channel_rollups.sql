-- Aggregates of bound measurement channels, one row per channel, resolution
-- and local-calendar bucket. A row carries the sums a coarser bucket can be
-- built from, so a month is merged from its days instead of read from raw
-- values; the value a channel reports for a period follows from its
-- aggregation kind when the row is read.
--
-- target_kind and target_id are the binding the row was built under. A plant
-- binding has no target_id.
CREATE TABLE channel_rollups (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    channel_id BLOB NOT NULL REFERENCES channel_definitions(id) ON DELETE CASCADE CHECK (length(channel_id) = 16),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('plant', 'inverter', 'string')),
    target_id BLOB CHECK (target_id IS NULL OR length(target_id) = 16),
    resolution TEXT NOT NULL CHECK (resolution IN ('fifteen_minute', 'hour', 'day', 'month', 'year')),
    bucket_start INTEGER NOT NULL,
    bucket_end INTEGER NOT NULL,
    point_count INTEGER NOT NULL CHECK (point_count > 0),
    value_sum INTEGER NOT NULL,
    value_min INTEGER NOT NULL,
    value_max INTEGER NOT NULL,
    value_first INTEGER NOT NULL,
    value_last INTEGER NOT NULL,
    -- Growth of a counter within the bucket. Absent for other channels and
    -- for a bucket whose first value has no predecessor.
    counter_delta INTEGER,
    calculated_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, resolution, bucket_start),
    CHECK (bucket_end > bucket_start),
    CHECK ((target_kind = 'plant') = (target_id IS NULL))
) STRICT;

CREATE INDEX channel_rollups_target_idx
    ON channel_rollups(system_id, target_kind, target_id, resolution, bucket_start);

-- How far a channel's values are aggregated: the receive time of the newest
-- reading that was included, and how many values of the channel were received
-- at that very instant. Both are absent while nothing is built, which is also
-- the state a changed binding returns a channel to.
ALTER TABLE channel_definitions ADD COLUMN rollup_watermark INTEGER;
ALTER TABLE channel_definitions ADD COLUMN rollup_watermark_values INTEGER;
