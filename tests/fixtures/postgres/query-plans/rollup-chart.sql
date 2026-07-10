SELECT bucket_start,
       bucket_end,
       generation_energy_sum_wh,
       coverage_basis_points,
       quality_flags
FROM telemetry.rollups
WHERE account_id = $1
  AND system_id = $2
  AND resolution = $3
  AND bucket_start >= $4
  AND bucket_start < $5
ORDER BY bucket_start;
