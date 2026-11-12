-- Temperature and voltage become measurements with their own columns and
-- aggregates. Until now a reading kept them only in its provenance document.

ALTER TABLE telemetry.rollups ADD COLUMN voltage_mean_millivolts BIGINT;

-- Readings stored before this migration get their columns from the provenance.
-- A column that already holds a value is left alone, so running these
-- statements again changes nothing.
UPDATE telemetry.hot_observations
   SET temperature_millidegrees_c = (provenance ->> 'temperatureMilliCelsius')::bigint
 WHERE temperature_millidegrees_c IS NULL
   AND jsonb_typeof(provenance -> 'temperatureMilliCelsius') = 'number';

UPDATE telemetry.hot_observations
   SET voltage_millivolts = (provenance ->> 'voltageMillivolts')::bigint
 WHERE voltage_millivolts IS NULL
   AND jsonb_typeof(provenance -> 'voltageMillivolts') = 'number';
