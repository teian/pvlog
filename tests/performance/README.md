# Performance tests

Place reproducible ingestion, retention, rollup, and chart-query benchmarks here. Large generated
datasets belong in ignored work directories; deterministic generators belong in `tests/support/`.

`chart-query-harness.mjs` validates a versioned measurement report and fails when the 30-day chart
p95 reaches 500 ms or the 25-year chart p95 reaches 1,000 ms. The committed report is a deterministic
CI regression fixture, not a capacity certification. Pass a report captured from a real deployment
as the first argument when running the harness for release certification.
