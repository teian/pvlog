-- The key already orders a scope's buckets.
DROP INDEX IF EXISTS telemetry.rollups_scope_idx;
-- A target's channel rollups are found through the channels bound to it.
DROP INDEX IF EXISTS telemetry.channel_rollups_target_idx;
