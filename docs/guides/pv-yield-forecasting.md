# PV yield forecasting and performance

PVLog keeps installation capacity, modeled energy, measured energy, and
conversion efficiency as separate concepts. This prevents a weather-model
result from being presented as telemetry and prevents unlike ratios from being
compared.

## Values and ratios

- **DC nameplate capacity** is the sum of the confirmed module peak powers that
  are effective for an interval. Forecast-ready capacity excludes strings with
  missing location, geometry, module, or settings inputs and is always shown
  beside total effective capacity.
- **Forecast generation** is power and energy calculated from a weather run
  issued before the modeled interval. It describes likely future production.
- **Historical expected generation** uses observed or reanalysis weather for a
  past interval. Forecast weather is never relabeled as an observation.
- **Actual generation** is measured AC energy. Missing or insufficient-quality
  telemetry remains unavailable; it is not substituted with zero.
- **Forecast realization** is actual generation divided by the forecast issued
  before the interval. It combines forecast accuracy and operational outcome.
- **Generation performance** is actual generation divided by historical
  expected generation. It compares the system with weather-adjusted potential.
- **Inverter efficiency** is AC output divided by DC input at an inverter. It is
  a conversion ratio and is not forecast realization or generation performance.

Ratios require a positive modeled denominator and sufficient actual/model
coverage. PVLog calculates them only where actual energy is measured at the
same scope; system measurements are not allocated down to inverters or strings.

## Losses, calibration, and uncertainty

Soiling, shading, mismatch, wiring, and unavailability losses are bounded
fixed-point factors applied multiplicatively. Calibration is a bounded signed
factor used to correct a persistent, evidenced model bias. Neither setting
changes confirmed equipment identity or historical telemetry. Settings are
effective-dated, versioned, protected by ETags, and cause only intersecting
modeled intervals to be recalculated.

Every forward point carries central, lower, and upper power/energy values. The
range expresses uncertainty supplied or derived from the weather/model inputs;
it is not a service guarantee. Partial-capacity results show included and total
nameplate capacity, and stale results retain their original issue time and
provider provenance.

## Provider attribution, licensing, and outages

Administrators supply a provider-neutral normalized-weather adapter. The UI and
exports retain provider attribution, source URL, adapter, fetch time, and
license identifier. Operators are responsible for choosing a provider license
that permits storage, calculation, display, and export. PVLog does not bundle
restricted credentials or licensed weather datasets.

When a provider or calculation service is unavailable, policy may permit an
explicitly stale immutable run for a bounded age. Otherwise forecast resources
return an unavailable state. Weather failure never disables telemetry ingestion
or rewrites forecast data as observed data. Historical comparisons remain
unavailable until the required weather and actual-coverage inputs exist.

## Operator workflow

1. Confirm system location and effective inverter/string/module configuration.
2. Configure loss and calibration settings with an effective boundary.
3. Enable the adapter only after verifying its license and secret reference.
4. Review forecast readiness and excluded capacity before trusting aggregates.
5. Use generation performance for weather-adjusted system assessment, forecast
   realization for forecast evaluation, and inverter efficiency only for
   DC-to-AC conversion analysis.

The deterministic equations and rounding rules are documented in
[PV yield model v1](../reference/pv-yield-model-v1.md). Runtime controls and
failure behavior are documented in the
[Compose guide](../../deploy/compose/README.md#pv-yield-forecasting).

## Bounded recalculation commands

Preview a deterministic single-account, single-system range before writing any
invalidation or job. Epoch ranges are half-open and limited to 366 days:

```sh
pvlog forecast recalculate \
  --account-id 019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70 \
  --system-id 019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71 \
  --start-epoch-millis 1780000000000 \
  --end-epoch-millis 1780086400000 \
  --dry-run
```

Remove `--dry-run` to insert an idempotent model-version invalidation and a
coalesced rebuild job. Repeating the same account, system, and range returns the
same durable work rather than duplicating it. The JSON response contains the
job ID used by the remaining commands:

```sh
pvlog forecast progress --account-id ACCOUNT_UUID --job-id JOB_UUID
pvlog forecast cancel --account-id ACCOUNT_UUID --job-id JOB_UUID
pvlog forecast retry --account-id ACCOUNT_UUID --job-id JOB_UUID
```

Progress reports the durable state and `attempt/max-attempts`. Cancellation is
cooperative: it removes queued work and prevents a leased job from recording
completion, but an external weather request already in flight may finish.
Retry is allowed only for failed, dead-lettered, or cancelled work and resets
the attempt counter.

Recalculation never mutates actual telemetry or deletes an earlier immutable
calculation run. To roll back a bad settings/model deployment, restore the prior
effective settings or model revision, preview the affected range, enqueue a new
recalculation, and verify the active projection before retention removes
unreferenced working runs. Issued/referenced forecasts remain preserved for
historical audit and performance comparison.
