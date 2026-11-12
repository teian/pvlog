//! Which daily value a system reports when a source's own summary and the
//! value computed from interval readings both exist.

use serde::Serialize;

/// Share of the expected readings of a day from which the computed value is
/// trusted over a summary the source has not yet marked complete.
pub const SUFFICIENT_DAILY_COVERAGE_BASIS_POINTS: u16 = 8_000;

/// The energy of one local day from one origin.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEnergy {
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
}

/// A day as computed from interval readings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComputedDay {
    pub energy: DayEnergy,
    pub coverage_basis_points: u16,
}

/// A day as a source summarized it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuppliedDay {
    pub energy: DayEnergy,
    /// True once the source states that the day will not change any more.
    pub complete: bool,
}

/// What the reported value of a day is based on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayBasis {
    /// The source's summary, marked complete.
    SuppliedComplete,
    /// Interval readings.
    Computed,
    /// The source's summary, not yet marked complete.
    SuppliedIncomplete,
}

impl DayBasis {
    /// True when the value comes from the source's summary.
    #[must_use]
    pub const fn is_supplied(self) -> bool {
        matches!(self, Self::SuppliedComplete | Self::SuppliedIncomplete)
    }
}

/// Chooses the value a day reports.
///
/// A complete supplied summary wins, because the source's own daily counter is
/// more accurate than integrated samples. Otherwise interval readings win when
/// they cover the day sufficiently. Otherwise an incomplete supplied summary is
/// used, and failing that whatever was computed.
#[must_use]
pub fn effective_day(
    computed: Option<ComputedDay>,
    supplied: Option<SuppliedDay>,
) -> Option<(DayEnergy, DayBasis)> {
    match (computed, supplied) {
        (_, Some(supplied)) if supplied.complete => {
            Some((supplied.energy, DayBasis::SuppliedComplete))
        }
        (Some(computed), Some(_))
            if computed.coverage_basis_points >= SUFFICIENT_DAILY_COVERAGE_BASIS_POINTS =>
        {
            Some((computed.energy, DayBasis::Computed))
        }
        (_, Some(supplied)) => Some((supplied.energy, DayBasis::SuppliedIncomplete)),
        (Some(computed), None) => Some((computed.energy, DayBasis::Computed)),
        (None, None) => None,
    }
}

/// Tariff bands a source may split its imported energy into.
pub const IMPORT_BANDS: [&str; 4] = ["peak", "offPeak", "shoulder", "highShoulder"];

/// Longest weather condition a daily summary may carry, counted in characters.
pub const MAXIMUM_WEATHER_CONDITION_CHARACTERS: usize = 40;

/// A daily summary as a source submits it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SuppliedDailySummary {
    /// Local calendar date in the system's timezone, as `YYYY-MM-DD`.
    pub date: String,
    pub generation_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    /// Lifetime generation counter at the end of the day.
    pub generation_lifetime_wh: Option<i64>,
    /// True once the source states that the day will not change any more.
    pub complete: bool,
    pub peak_power_watts: Option<i64>,
    /// Local time of the peak as `HH:MM`.
    pub peak_time: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature_min_millidegrees_celsius: Option<i64>,
    pub temperature_max_millidegrees_celsius: Option<i64>,
    /// Imported energy in watt-hours keyed by one of [`IMPORT_BANDS`].
    pub import_energy_wh_by_band: std::collections::BTreeMap<String, i64>,
}

impl SuppliedDailySummary {
    /// Checks the summary before it is stored.
    ///
    /// # Errors
    ///
    /// Returns the wire name of the first member that is out of range.
    pub fn validate(&self) -> Result<(), &'static str> {
        if chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").is_err()
            || self.date.len() != 10
        {
            return Err("date");
        }
        for (value, field) in [
            (self.generation_energy_wh, "generationEnergyWh"),
            (self.export_energy_wh, "exportEnergyWh"),
            (self.consumption_energy_wh, "consumptionEnergyWh"),
            (self.generation_lifetime_wh, "generationLifetimeWh"),
            (self.peak_power_watts, "peakPowerWatts"),
        ] {
            if value.is_some_and(|value| value < 0) {
                return Err(field);
            }
        }
        if self.peak_time.as_deref().is_some_and(|time| {
            time.len() != 5 || chrono::NaiveTime::parse_from_str(time, "%H:%M").is_err()
        }) {
            return Err("peakTime");
        }
        if self
            .weather_condition
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAXIMUM_WEATHER_CONDITION_CHARACTERS)
        {
            return Err("weatherCondition");
        }
        if let (Some(minimum), Some(maximum)) = (
            self.temperature_min_millidegrees_celsius,
            self.temperature_max_millidegrees_celsius,
        ) && minimum > maximum
        {
            return Err("temperatureMaxMillidegreesCelsius");
        }
        if self
            .import_energy_wh_by_band
            .iter()
            .any(|(band, value)| !IMPORT_BANDS.contains(&band.as_str()) || *value < 0)
        {
            return Err("importEnergyWhByBand");
        }
        Ok(())
    }
}
