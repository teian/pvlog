SELECT measured_at,
       generation_power_watts,
       consumption_power_watts,
       quality_flags
FROM telemetry.hot_observations
WHERE account_id = $1
  AND system_id = $2
  AND measured_at >= $3
  AND measured_at < $4
ORDER BY measured_at;
