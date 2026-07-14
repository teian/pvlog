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
