-- Aggregates of bound measurement channels, one row per channel, resolution
-- and local-calendar bucket. A row carries the sums a coarser bucket can be
-- built from, so a month is merged from its days instead of read from raw
-- values; the value a channel reports for a period follows from its
-- aggregation kind when the row is read.
--
-- target_kind and target_id are the binding the row was built under. A plant
-- binding has no target_id.
CREATE TABLE telemetry.channel_rollups (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    channel_id UUID NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('plant', 'inverter', 'string')),
    target_id UUID,
    resolution TEXT NOT NULL CHECK (resolution IN ('fifteen_minute', 'hour', 'day', 'month', 'year')),
    bucket_start BIGINT NOT NULL,
    bucket_end BIGINT NOT NULL,
    point_count BIGINT NOT NULL CHECK (point_count > 0),
    value_sum BIGINT NOT NULL,
    value_min BIGINT NOT NULL,
    value_max BIGINT NOT NULL,
    value_first BIGINT NOT NULL,
    value_last BIGINT NOT NULL,
    -- Growth of a counter within the bucket. Absent for other channels and
    -- for a bucket whose first value has no predecessor.
    counter_delta BIGINT,
    calculated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, channel_id, resolution, bucket_start),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, channel_id)
        REFERENCES account_data.channel_definitions(account_id, id) ON DELETE CASCADE,
    CHECK (bucket_end > bucket_start),
    CHECK ((target_kind = 'plant') = (target_id IS NULL))
);

CREATE INDEX channel_rollups_target_idx
    ON telemetry.channel_rollups(account_id, system_id, target_kind, target_id, resolution, bucket_start);

-- How far a channel's values are aggregated: the receive time of the newest
-- reading that was included, and how many values of the channel were received
-- at that very instant. Both are absent while nothing is built, which is also
-- the state a changed binding returns a channel to.
ALTER TABLE account_data.channel_definitions
    ADD COLUMN rollup_watermark BIGINT,
    ADD COLUMN rollup_watermark_values BIGINT;
