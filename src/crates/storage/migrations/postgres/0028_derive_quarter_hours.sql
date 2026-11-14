-- Channel quarter-hours are worked out from the values when they are read.
DELETE FROM telemetry.channel_rollups WHERE resolution = 'fifteen_minute';
