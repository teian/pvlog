# PV yield model v1

Model `pv-yield-v1`, revision `1`, uses UTC and confirmed installation geometry. Solar position
follows the public NOAA fractional-year approximation. Longitude is positive east; azimuth is
clockwise from true north. IEEE-754 binary64 intermediates are rounded to 0.001 degree.

Provider plane-of-array irradiance is retained. Otherwise v1 requires GHI, DNI, and DHI and uses
the isotropic-sky model with ground albedo 0.20. Negative incidence is clamped to zero and output
is rounded to whole W/m2. Calculated plane-of-array irradiance is zero below the horizon.

Reference fixtures are committed in `tests/rust/domain/yield_model.rs`. Changing a rounded fixture
requires a model revision.

Module temperature is ambient temperature plus a fixed 25 C rise at 800 W/m2. String DC power is
nameplate multiplied by POA irradiance, the confirmed peak-power temperature coefficient when
available, each configured loss factor multiplicatively, and the signed calibration factor. Each
fixed-point division rounds to the nearest base unit. Negative output is clamped to zero and the
physical safety cap is 125% of confirmed DC nameplate.

Interval energy integrates the interval power over the exact half-open UTC duration and rounds to
whole Wh. Forecast basis accepts only forecast weather. Historical expected-generation basis
accepts only observed or reanalysis weather; forecast input is never relabeled as observation.

Generation performance is actual divided by historical expected energy. Forecast realization is
actual divided by the previously issued forecast. Both require configured actual-coverage and
quality thresholds plus a positive modeled denominator. Ratios are calculated only at an exact
measurement scope; aggregate actual energy is never allocated down to an inverter or string.

Nameplate capacity is confirmed effective DC module capacity, not a modeled or measured energy
value. Forecast-ready capacity is reported separately from total effective capacity whenever an
input gap excludes a string. Inverter efficiency remains AC output divided by DC input and must not
be labeled as either generation performance or forecast realization.

Lower and upper yield values retain propagated input uncertainty and use the same unit, interval,
configuration digest, and model revision as the central value. Loss and calibration factors are
effective-dated modeling inputs; they do not mutate equipment identity or actual telemetry.

Normalized provider provenance includes adapter, source URL, fetch time, attribution, and license
identifier. A policy-permitted stale run keeps its original issue and fetch times. Provider outage
may make modeled output unavailable, but cannot block ingestion or convert forecast weather into
observed weather.
