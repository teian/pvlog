-- Quarter-hours are worked out from the readings when they are read: at the
-- usual cadence of a few minutes they hold barely fewer rows than the
-- readings themselves.
DELETE FROM telemetry_rollups WHERE resolution = 'fifteen_minute';
DELETE FROM channel_rollups WHERE resolution = 'fifteen_minute';
