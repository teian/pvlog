-- Temperature and voltage become measurements with their own columns and
-- aggregates. Until now a reading kept them only in its provenance document.

ALTER TABLE telemetry_rollups ADD COLUMN voltage_mean_millivolts INTEGER;

-- Readings stored before this migration get their columns from the provenance.
-- A column that already holds a value is left alone, so running these
-- statements again changes nothing.
UPDATE telemetry_hot
   SET temperature_millidegrees_c = json_extract(provenance_json, '$.temperatureMilliCelsius')
 WHERE temperature_millidegrees_c IS NULL
   AND json_type(provenance_json, '$.temperatureMilliCelsius') = 'integer';

UPDATE telemetry_hot
   SET voltage_millivolts = json_extract(provenance_json, '$.voltageMillivolts')
 WHERE voltage_millivolts IS NULL
   AND json_type(provenance_json, '$.voltageMillivolts') = 'integer';
