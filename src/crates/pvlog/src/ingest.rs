//! Storage-backed ingestion of what a logger delivers in one request.
//!
//! The values of devices become measurement channels under the device key
//! scheme, and the system's own values are derived from them where the logger
//! did not state them. Readings, samples, summaries and events are stored by
//! what they describe, so a request that is sent again changes nothing, and
//! the marks tell the logger how far its data has arrived.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use async_trait::async_trait;
use pvlog_api::{
    IngestApiError, IngestApiUseCases, IngestCounts, IngestMark, IngestMarks, IngestResponse,
    IngestWriter,
};
use pvlog_application::{
    ChannelReading, ChannelTyping, Clock, DecimalValue, IngestMeasurement, IngestRejection,
    IngestRequest, LifetimeCounters, ReportedDevice, channel_typing_for, derive_generation,
    device_channel, device_channel_readings, device_totals, scaled_channel_value,
};
use pvlog_domain::{AccountId, ChannelId, DataSourceId, ObservationId, QualityFlags, SystemId};
use pvlog_storage::{
    ChannelRegistration, DataSourceRepository, DatabaseTarget, DeviceEventDetails,
    DeviceEventRecord, DeviceEventRepository, DiscoveredDeviceRecord, ENERGY_SAMPLE_ORIGIN,
    EnergySampleRecord, INGESTION_SOURCE_KIND, IngestRepository, IngestionMarkRecord,
    ManagementRepository, ObservationUpsert, ReportedDeviceRecord, SourceDailySummaryRecord,
    StoredMeasurements, StoredObservation, StoredReading, TelemetryRepository, UpsertOutcome,
};
use uuid::Uuid;

/// Serves the ingestion API from account storage.
pub struct StorageIngestApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

/// The repositories of one account that an ingestion request writes to.
struct Repositories {
    ingest: Box<dyn IngestRepository>,
    telemetry: Box<dyn TelemetryRepository>,
    data_sources: Box<dyn DataSourceRepository>,
    events: Box<dyn DeviceEventRepository>,
}

/// Where the data of a request comes from.
struct Source {
    data_source_id: Option<DataSourceId>,
    /// Stable name of the source: its data source when it has one, and the
    /// principal otherwise, so that regenerating a key starts no second series.
    key: String,
}

/// A registered measurement channel with the definition its values must fit.
struct Channel {
    id: ChannelId,
    typing: ChannelTyping,
}

/// Counters the readings of a source carried last, which the next reading's
/// interval energy is measured against.
#[derive(Clone, Default)]
struct Baseline {
    /// Lifetime counters of the devices of the previous reading that had any.
    devices: Option<LifetimeCounters>,
    generation_lifetime_wh: Option<i64>,
    consumption_lifetime_wh: Option<i64>,
}

fn unavailable<E>(_: E) -> IngestApiError {
    IngestApiError::Unavailable
}

/// Converts the epoch seconds of the wire into the stored epoch milliseconds.
const fn millis(seconds: i64) -> i64 {
    seconds.saturating_mul(1_000)
}

impl StorageIngestApi {
    /// Creates the ingestion API over the given database target, management repository, and clock.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            management,
            clock,
        }
    }

    /// Opens the repositories of the system's account; an unregistered system is `NotFound`.
    async fn repositories(&self, system_id: SystemId) -> Result<Repositories, IngestApiError> {
        let account_id = self
            .management
            .system_registry(system_id)
            .await
            .map_err(unavailable)?
            .ok_or(IngestApiError::NotFound)?
            .account_id;
        self.account_repositories(account_id)
            .await
            .ok_or(IngestApiError::Unavailable)
    }

    async fn account_repositories(&self, account_id: AccountId) -> Option<Repositories> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = pvlog_storage::SqliteAccountPoolRouter::shared(
                        management_path.clone(),
                        accounts_dir.clone(),
                    )
                    .ok()?
                    .route(account_id)
                    .await
                    .ok()?;
                    Some(Repositories {
                        ingest: Box::new(pvlog_storage::SqliteIngestRepository::new(
                            account.clone(),
                        )),
                        telemetry: Box::new(pvlog_storage::SqliteTelemetryRepository::new(
                            account.clone(),
                        )),
                        data_sources: Box::new(pvlog_storage::SqliteDataSourceRepository::new(
                            account.clone(),
                        )),
                        events: Box::new(pvlog_storage::SqliteDeviceEventRepository::new(account)),
                    })
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id);
                    None
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Some(Repositories {
                        ingest: Box::new(pvlog_storage::PostgresIngestRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        telemetry: Box::new(pvlog_storage::PostgresTelemetryRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        data_sources: Box::new(pvlog_storage::PostgresDataSourceRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        events: Box::new(pvlog_storage::PostgresDeviceEventRepository::new(
                            url.clone(),
                            account_id,
                        )),
                    })
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id);
                    None
                }
            }
        }
    }

    /// Finds the data source the writer's credential belongs to.
    async fn source(
        repositories: &Repositories,
        system_id: SystemId,
        writer: &IngestWriter,
    ) -> Result<Source, IngestApiError> {
        let data_source_id = match writer.credential_id {
            Some(credential_id) => repositories
                .data_sources
                .data_source_by_credential(credential_id)
                .await
                .map_err(unavailable)?
                // A credential is restricted to its system; a source of
                // another system must never be credited.
                .filter(|record| record.system_id == system_id)
                .map(|record| record.id),
            None => None,
        };
        Ok(Source {
            data_source_id,
            key: data_source_id.map_or_else(|| writer.identity.clone(), |id| id.to_string()),
        })
    }

    /// Registers the measurement channels a request writes to and proposes
    /// the devices whose channels are new.
    async fn register_channels(
        &self,
        system_id: SystemId,
        request: &IngestRequest,
        received_at: i64,
    ) -> Result<BTreeMap<String, Channel>, IngestApiError> {
        let mut readings: BTreeMap<String, ChannelReading> = BTreeMap::new();
        for device in request
            .measurements
            .iter()
            .flat_map(|measurement| &measurement.devices)
        {
            for (key, reading) in device_channel_readings(device) {
                readings.entry(key).or_insert(reading);
            }
        }
        for sample in &request.energy_samples {
            if let Some(lifetime) = sample.energy_lifetime_wh {
                readings
                    .entry(lifetime_key(&sample.device_id))
                    .or_insert_with(|| integer_reading(lifetime));
            }
        }
        if readings.is_empty() {
            return Ok(BTreeMap::new());
        }
        let registrations: Vec<ChannelRegistration> = readings
            .iter()
            .map(|(key, reading)| {
                let typing = channel_typing_for(key, reading);
                ChannelRegistration {
                    channel_key: key.clone(),
                    display_name: key.clone(),
                    data_type: typing.value_type.name().to_owned(),
                    unit: typing.unit,
                    scale: i32::from(typing.scale),
                    aggregation: typing.aggregation.name().to_owned(),
                }
            })
            .collect();
        let channels =
            crate::channels::channel_repository(&self.target, self.management.as_ref(), system_id)
                .await
                .ok_or(IngestApiError::Unavailable)?;
        let records = channels
            .register_channels(system_id, &registrations, received_at)
            .await
            .map_err(unavailable)?;
        let mut new_devices = BTreeSet::new();
        let mut registered = BTreeMap::new();
        for (key, record) in readings.keys().zip(records) {
            if record.discovered_at == Some(received_at)
                && record.last_value_at.is_none()
                && let Some(device) = device_channel(key)
            {
                new_devices.insert(device.target.serial().to_owned());
            }
            let typing = ChannelTyping {
                value_type: pvlog_application::ChannelValueType::from_name(&record.data_type)
                    .ok_or(IngestApiError::Unavailable)?,
                unit: record.unit,
                scale: u8::try_from(record.scale).map_err(unavailable)?,
                aggregation: pvlog_application::ChannelAggregation::from_name(&record.aggregation)
                    .ok_or(IngestApiError::Unavailable)?,
            };
            registered.insert(
                key.clone(),
                Channel {
                    id: record.id,
                    typing,
                },
            );
        }
        self.record_devices(system_id, request, &new_devices, received_at)
            .await?;
        Ok(registered)
    }

    /// Keeps the identity of the devices a request describes, and proposes
    /// every inverter among them and every device with new channels that
    /// matches no inverter of the system.
    async fn record_devices(
        &self,
        system_id: SystemId,
        request: &IngestRequest,
        new_devices: &BTreeSet<String>,
        received_at: i64,
    ) -> Result<(), IngestApiError> {
        if request.devices.is_empty() && new_devices.is_empty() {
            return Ok(());
        }
        let discovery = crate::discovery::discovery_repository(
            &self.target,
            self.management.as_ref(),
            system_id,
        )
        .await
        .ok_or(IngestApiError::Unavailable)?;
        let reported: Vec<ReportedDevice> = request.devices.iter().map(Into::into).collect();
        for device in &reported {
            discovery
                .record_device_identity(
                    system_id,
                    &ReportedDeviceRecord {
                        device_serial: device.id.clone(),
                        kind: device.kind.clone(),
                        family_id: device.family_id.clone(),
                        configured_name: device.configured_name.clone(),
                        reported_name: device.reported_name.clone(),
                        model: device.model.clone(),
                        firmware: device.firmware.clone(),
                        transport: device.transport.clone(),
                    },
                    received_at,
                )
                .await
                .map_err(unavailable)?;
        }
        let inverters = reported
            .iter()
            .filter(|device| device.kind.as_deref().is_none_or(|kind| kind == "inverter"))
            .map(|device| device.id.as_str());
        let serials: BTreeSet<&str> = new_devices
            .iter()
            .map(String::as_str)
            .chain(inverters)
            .collect();
        for serial in serials {
            let identity = reported.iter().find(|device| device.id == serial);
            discovery
                .record_discovered_device(
                    system_id,
                    &DiscoveredDeviceRecord {
                        device_serial: serial.to_owned(),
                        reported_name: identity.and_then(|device| {
                            device
                                .reported_name
                                .clone()
                                .or_else(|| device.configured_name.clone())
                        }),
                        model: identity.and_then(|device| device.model.clone()),
                        firmware: identity.and_then(|device| device.firmware.clone()),
                    },
                    received_at,
                )
                .await
                .map_err(unavailable)?;
        }
        Ok(())
    }
}

fn lifetime_key(device_id: &str) -> String {
    format!("inv.{device_id}.energy.lifetime")
}

fn integer_reading(value: i64) -> ChannelReading {
    ChannelReading {
        value: Some(DecimalValue::integer(value)),
        ..ChannelReading::default()
    }
}

/// Derives interval energy from a lifetime counter the logger stated for the
/// system. Returns the energy and whether the counter went back.
fn counter_energy(lifetime: Option<i64>, baseline: Option<i64>) -> (Option<i64>, bool) {
    match lifetime.zip(baseline) {
        Some((now, before)) if now >= before => (Some(now - before), false),
        Some(_) => (None, true),
        None => (None, false),
    }
}

/// What is measured at one time: a measurement of the request, or the energy
/// samples of a time for which the request has no measurement.
#[derive(Default)]
struct Moment<'a> {
    /// Position of the measurement in the request.
    measurement: Option<(usize, &'a IngestMeasurement)>,
    /// Lifetime counter and power of the samples, by device.
    samples: BTreeMap<&'a str, (Option<i64>, Option<i64>)>,
}

/// Builds the reading of one moment and moves the baseline on to it.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn reading(
    system_id: SystemId,
    source: &Source,
    measured_at: i64,
    received_at: i64,
    moment: &Moment<'_>,
    channels: &BTreeMap<String, Channel>,
    baseline: &mut Baseline,
    rejected: &mut Vec<IngestRejection>,
) -> ObservationUpsert {
    let mut values: BTreeMap<ChannelId, i64> = BTreeMap::new();
    let mut counters = LifetimeCounters::new();
    let mut measurements = StoredMeasurements::default();
    let (mut generation_power, mut generation_energy) = (None, None);
    let (mut consumption_power, mut consumption_energy) = (None, None);
    let mut flags = QualityFlags::NONE;
    let fills_gap = moment.measurement.is_none();
    if let Some((index, measurement)) = moment.measurement {
        for device in &measurement.devices {
            if let Some(lifetime) = device.energy_lifetime_wh {
                counters.insert(device.id.clone(), lifetime);
            }
            for (key, reading) in device_channel_readings(device) {
                let Some(channel) = channels.get(&key) else {
                    continue;
                };
                match scaled_channel_value(&channel.typing, &reading) {
                    Ok(value) => {
                        values.insert(channel.id, value);
                    }
                    // A value that contradicts its channel is left out and
                    // named; everything else of the measurement is kept.
                    Err(error) => rejected.push(IngestRejection {
                        category: "measurements",
                        index,
                        member: key,
                        reason: error.code(),
                    }),
                }
            }
        }
        let totals = device_totals(&measurement.devices);
        let system = measurement.system.clone().unwrap_or_default();
        generation_power = system.generation_power_w.or(totals.generation_power_w);
        generation_energy = system.generation_energy_wh;
        consumption_power = system.consumption_power_w;
        consumption_energy = system.consumption_energy_wh;
        measurements = StoredMeasurements {
            generation_lifetime_wh: system.generation_lifetime_wh,
            consumption_lifetime_wh: system.consumption_lifetime_wh,
            grid_import_power_watts: system.grid_import_power_w.or(totals.grid_import_power_w),
            grid_export_power_watts: system.grid_export_power_w.or(totals.grid_export_power_w),
            grid_import_energy_wh: system.grid_import_energy_wh,
            grid_export_energy_wh: system.grid_export_energy_wh,
            net_grid_power_watts: None,
            battery_power_watts: system.battery_power_w,
            battery_energy_wh: system.battery_energy_wh,
            battery_state_basis_points: system
                .battery_state_of_charge_permille
                .and_then(|permille| i32::try_from(permille * 10).ok())
                .or(totals.battery_state_basis_points),
            temperature_millidegrees_c: system.temperature_millicelsius,
            voltage_millivolts: system.voltage_mv,
            message: system.message,
        };
    } else {
        for (device, (lifetime, power)) in &moment.samples {
            if let Some(lifetime) = lifetime {
                counters.insert((*device).to_owned(), *lifetime);
                if let Some(channel) = channels.get(&lifetime_key(device)) {
                    values.insert(channel.id, *lifetime);
                }
            }
            if let Some(power) = power {
                generation_power = Some(generation_power.unwrap_or(0) + power);
            }
        }
    }
    // The net flow is only known when both directions are.
    measurements.net_grid_power_watts = measurements
        .grid_import_power_watts
        .zip(measurements.grid_export_power_watts)
        .map(|(import, export)| import - export);

    let derivation = Derivation {
        generation: generation_energy.is_none(),
        consumption: consumption_energy.is_none(),
        generation_counter: if measurements.generation_lifetime_wh.is_none() {
            GenerationCounter::Devices
        } else {
            GenerationCounter::System
        },
    };
    let energy = derive_energy(
        derivation,
        &counters,
        &measurements,
        (generation_energy, consumption_energy),
        baseline,
    );
    measurements.generation_lifetime_wh = energy.generation_lifetime_wh;
    generation_energy = energy.generation_wh;
    consumption_energy = energy.consumption_wh;
    flags = flags | energy.flags;
    advance(baseline, counters, &measurements);

    let channel_values: Vec<(ChannelId, i64)> = values.into_iter().collect();
    let mut provenance = serde_json::json!({ "sourceKind": INGESTION_SOURCE_KIND });
    if fills_gap {
        provenance["origin"] = ENERGY_SAMPLE_ORIGIN.into();
    }
    provenance[DERIVATION] = derivation.to_json();
    let mut observation = StoredObservation {
        id: ObservationId::new(),
        system_id,
        measured_at,
        received_at,
        source_kind: INGESTION_SOURCE_KIND.to_owned(),
        source_identity: source.key.clone(),
        idempotency_identity: None,
        quality_flags: i32::from(flags.bits()),
        generation_power_watts: generation_power,
        generation_energy_wh: generation_energy,
        consumption_power_watts: consumption_power,
        consumption_energy_wh: consumption_energy,
        provenance,
        canonical_hash: [0; 32],
        version: 1,
        data_source_id: source.data_source_id,
        measurements,
    };
    observation.canonical_hash = reading_hash(&observation, &channel_values, fills_gap);
    ObservationUpsert {
        observation,
        channel_values,
        fills_gap,
    }
}

/// Provenance member that records how the interval energy of a reading is
/// derived, so that it can be derived again when earlier readings arrive later.
const DERIVATION: &str = "energyDerivation";

/// The counter the generated energy of a reading is derived from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GenerationCounter {
    /// The lifetime counters of the devices.
    Devices,
    /// The lifetime counter the logger stated for the system.
    System,
}

/// Which interval energy of a reading the logger left to be derived, and from
/// which counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Derivation {
    generation: bool,
    consumption: bool,
    generation_counter: GenerationCounter,
}

impl Derivation {
    fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "generation": self.generation,
            "consumption": self.consumption,
            "generationCounter": match self.generation_counter {
                GenerationCounter::Devices => "devices",
                GenerationCounter::System => "system",
            },
        })
    }

    /// Reads what a stored reading recorded; a reading without the record is
    /// never derived again.
    fn of(provenance: &serde_json::Value) -> Option<Self> {
        let record = provenance.get(DERIVATION)?;
        Some(Self {
            generation: record.get("generation")?.as_bool()?,
            consumption: record.get("consumption")?.as_bool()?,
            generation_counter: match record.get("generationCounter")?.as_str()? {
                "devices" => GenerationCounter::Devices,
                "system" => GenerationCounter::System,
                _ => return None,
            },
        })
    }
}

/// The energy values of a reading after derivation.
struct DerivedEnergy {
    generation_lifetime_wh: Option<i64>,
    generation_wh: Option<i64>,
    consumption_wh: Option<i64>,
    /// `DERIVED` and `COUNTER_RESET` as the derivation found them.
    flags: QualityFlags,
}

/// Derives the interval energy the logger did not state from the counters of
/// the reading and those of the reading before. What the logger states for
/// the system wins; otherwise the counters of the devices say what the system
/// generated since the reading before.
fn derive_energy(
    derivation: Derivation,
    counters: &LifetimeCounters,
    measurements: &StoredMeasurements,
    (generation_wh, consumption_wh): (Option<i64>, Option<i64>),
    baseline: &Baseline,
) -> DerivedEnergy {
    let mut energy = DerivedEnergy {
        generation_lifetime_wh: measurements.generation_lifetime_wh,
        generation_wh,
        consumption_wh,
        flags: QualityFlags::NONE,
    };
    match derivation.generation_counter {
        GenerationCounter::Devices => {
            let derived = derive_generation(counters, baseline.devices.as_ref());
            energy.generation_lifetime_wh = derived.lifetime_wh;
            if derivation.generation {
                energy.generation_wh = derived.energy_wh;
                if derived.energy_wh.is_some() {
                    energy.flags = energy.flags | QualityFlags::DERIVED;
                }
            }
            if derived.counter_reset {
                energy.flags = energy.flags | QualityFlags::COUNTER_RESET;
            }
        }
        GenerationCounter::System if derivation.generation => {
            let (derived, reset) = counter_energy(
                measurements.generation_lifetime_wh,
                baseline.generation_lifetime_wh,
            );
            energy.generation_wh = derived;
            energy.flags = energy.flags | derivation_flags(derived, reset);
        }
        GenerationCounter::System => {}
    }
    if derivation.consumption {
        let (derived, reset) = counter_energy(
            measurements.consumption_lifetime_wh,
            baseline.consumption_lifetime_wh,
        );
        energy.consumption_wh = derived;
        energy.flags = energy.flags | derivation_flags(derived, reset);
    }
    energy
}

fn derivation_flags(energy: Option<i64>, reset: bool) -> QualityFlags {
    match (energy, reset) {
        (Some(_), _) => QualityFlags::DERIVED,
        (None, true) => QualityFlags::COUNTER_RESET,
        (None, false) => QualityFlags::NONE,
    }
}

/// Moves the baseline on to a reading.
fn advance(baseline: &mut Baseline, counters: LifetimeCounters, measurements: &StoredMeasurements) {
    if !counters.is_empty() {
        baseline.devices = Some(counters);
    }
    baseline.generation_lifetime_wh = measurements
        .generation_lifetime_wh
        .or(baseline.generation_lifetime_wh);
    baseline.consumption_lifetime_wh = measurements
        .consumption_lifetime_wh
        .or(baseline.consumption_lifetime_wh);
}

/// The hash of what a reader can see of a reading, so that a value that
/// matured replaces its predecessor and an unchanged one does not.
fn reading_hash(
    observation: &StoredObservation,
    channel_values: &[(ChannelId, i64)],
    fills_gap: bool,
) -> [u8; 32] {
    let measurements = &observation.measurements;
    let mut channels: Vec<(ChannelId, i64)> = channel_values.to_vec();
    channels.sort_unstable();
    *blake3::hash(
        serde_json::json!({
            "generationPowerWatts": observation.generation_power_watts,
            "generationEnergyWh": observation.generation_energy_wh,
            "consumptionPowerWatts": observation.consumption_power_watts,
            "consumptionEnergyWh": observation.consumption_energy_wh,
            "generationLifetimeWh": measurements.generation_lifetime_wh,
            "consumptionLifetimeWh": measurements.consumption_lifetime_wh,
            "grid": [
                measurements.grid_import_power_watts,
                measurements.grid_export_power_watts,
                measurements.grid_import_energy_wh,
                measurements.grid_export_energy_wh,
            ],
            "battery": [
                measurements.battery_power_watts,
                measurements.battery_energy_wh,
                measurements.battery_state_basis_points.map(i64::from),
            ],
            "temperature": measurements.temperature_millidegrees_c,
            "voltage": measurements.voltage_millivolts,
            "message": measurements.message,
            "qualityFlags": observation.quality_flags,
            "fillsGap": fills_gap,
            "channels": channels
                .iter()
                .map(|(channel, value)| (channel.to_string(), *value))
                .collect::<Vec<_>>(),
        })
        .to_string()
        .as_bytes(),
    )
    .as_bytes()
}

/// The lifetime counters of a stored reading, by device.
fn stored_counters(reading: &StoredReading) -> LifetimeCounters {
    reading
        .device_lifetime_wh
        .iter()
        .filter_map(|(key, value)| Some((device_channel(key)?.target.serial().to_owned(), *value)))
        .collect()
}

/// Derives the interval energy of a stored reading again against the baseline
/// and moves the baseline on to it. Returns the revised reading when its
/// energy changed. Energy the logger stated is kept as it is.
fn rederive(reading: &StoredReading, baseline: &mut Baseline) -> Option<ObservationUpsert> {
    let stored = &reading.observation;
    let counters = stored_counters(reading);
    let derivation = Derivation::of(&stored.provenance);
    let revised = derivation.and_then(|derivation| {
        let energy = derive_energy(
            derivation,
            &counters,
            &stored.measurements,
            (
                stored
                    .generation_energy_wh
                    .filter(|_| !derivation.generation),
                stored
                    .consumption_energy_wh
                    .filter(|_| !derivation.consumption),
            ),
            baseline,
        );
        let derived_bits = i32::from((QualityFlags::DERIVED | QualityFlags::COUNTER_RESET).bits());
        let quality_flags = (stored.quality_flags & !derived_bits) | i32::from(energy.flags.bits());
        if energy.generation_wh == stored.generation_energy_wh
            && energy.consumption_wh == stored.consumption_energy_wh
            && quality_flags == stored.quality_flags
        {
            return None;
        }
        let fills_gap = stored
            .provenance
            .get("origin")
            .and_then(serde_json::Value::as_str)
            == Some(ENERGY_SAMPLE_ORIGIN);
        let mut observation = StoredObservation {
            generation_energy_wh: energy.generation_wh,
            consumption_energy_wh: energy.consumption_wh,
            quality_flags,
            ..stored.clone()
        };
        observation.canonical_hash = reading_hash(&observation, &reading.channel_values, fills_gap);
        Some(ObservationUpsert {
            observation,
            channel_values: reading.channel_values.clone(),
            fills_gap,
        })
    });
    advance(baseline, counters, &stored.measurements);
    revised
}

/// Counts what became of the items of a list.
fn counts(total: usize, stored: u64) -> IngestCounts {
    let total = u64::try_from(total).unwrap_or(u64::MAX);
    IngestCounts {
        stored,
        unchanged: total.saturating_sub(stored),
        rejected: Vec::new(),
    }
}

/// Collects the furthest time or date per device.
#[derive(Default)]
struct Marks(BTreeMap<String, IngestionMarkRecord>);

impl Marks {
    fn of(&mut self, device: &str) -> &mut IngestionMarkRecord {
        self.0
            .entry(device.to_owned())
            .or_insert_with(|| IngestionMarkRecord {
                device_key: device.to_owned(),
                ..IngestionMarkRecord::default()
            })
    }
}

fn later(mark: &mut Option<i64>, at: i64) {
    *mark = Some(mark.map_or(at, |held| held.max(at)));
}

/// Renders stored marks for the API, with times in epoch seconds.
fn marks_response(records: Vec<IngestionMarkRecord>) -> IngestMarks {
    let mut marks = IngestMarks::default();
    for record in records {
        let mark = IngestMark {
            device_id: (!record.device_key.is_empty()).then_some(record.device_key),
            measured_at: record.measured_at.map(|at| at / 1_000),
            sample_at: record.sample_at.map(|at| at / 1_000),
            summary_date: record.summary_date,
            event_at: record.event_at.map(|at| at / 1_000),
        };
        if mark.device_id.is_some() {
            marks.devices.push(mark);
        } else {
            marks.system = mark;
        }
    }
    marks
}

#[async_trait]
impl IngestApiUseCases for StorageIngestApi {
    #[allow(clippy::too_many_lines)]
    async fn ingest(
        &self,
        system_id: SystemId,
        writer: IngestWriter,
        request: IngestRequest,
    ) -> Result<IngestResponse, IngestApiError> {
        let repositories = self.repositories(system_id).await?;
        let source = Self::source(&repositories, system_id, &writer).await?;
        let received_at = i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default();
        let channels = self
            .register_channels(system_id, &request, received_at)
            .await?;
        let mut marks = Marks::default();

        // Measurements and the samples of times without one, in order of time.
        let mut moments: BTreeMap<i64, Moment<'_>> = BTreeMap::new();
        for (index, measurement) in request.measurements.iter().enumerate() {
            let at = millis(measurement.measured_at);
            moments.entry(at).or_default().measurement = Some((index, measurement));
            later(&mut marks.of("").measured_at, at);
            for device in &measurement.devices {
                later(&mut marks.of(&device.id).measured_at, at);
            }
        }
        for sample in &request.energy_samples {
            let at = millis(sample.measured_at);
            moments.entry(at).or_default().samples.insert(
                &sample.device_id,
                (sample.energy_lifetime_wh, sample.power_w),
            );
            later(&mut marks.of(&sample.device_id).sample_at, at);
        }
        let mut response = IngestResponse::default();
        if let Some(first) = moments.keys().next().copied() {
            let mut baseline = Baseline::default();
            if let Some(counters) = repositories
                .ingest
                .device_counters_before(system_id, &source.key, first)
                .await
                .map_err(unavailable)?
            {
                baseline.devices = Some(
                    counters
                        .lifetime_wh
                        .iter()
                        .filter_map(|(key, value)| {
                            Some((device_channel(key)?.target.serial().to_owned(), *value))
                        })
                        .collect(),
                );
            }
            let latest = repositories
                .telemetry
                .latest_counters(system_id, source.data_source_id, &source.key, first)
                .await
                .map_err(unavailable)?;
            baseline.generation_lifetime_wh = latest.generation.map(|(_, value)| value);
            baseline.consumption_lifetime_wh = latest.consumption.map(|(_, value)| value);

            // Readings stored earlier between and right after the moments of
            // the request are measured against the moments before them, so
            // that history delivered after live readings neither leaves the
            // first live reading without energy nor counts a gap twice.
            let last = moments.keys().next_back().copied().unwrap_or(first);
            let stored: BTreeMap<i64, StoredReading> = repositories
                .ingest
                .readings_through(system_id, &source.key, first, last)
                .await
                .map_err(unavailable)?
                .into_iter()
                .map(|reading| (reading.observation.measured_at, reading))
                .collect();
            let times: BTreeSet<i64> = moments.keys().chain(stored.keys()).copied().collect();
            let mut rejected = Vec::new();
            let mut readings = Vec::with_capacity(moments.len());
            let mut revisions = Vec::new();
            for at in times {
                let held = stored.get(&at);
                let Some(moment) = moments.get(&at) else {
                    revisions.extend(held.and_then(|held| rederive(held, &mut baseline)));
                    continue;
                };
                let live = held.filter(|held| {
                    held.observation
                        .provenance
                        .get("origin")
                        .and_then(serde_json::Value::as_str)
                        != Some(ENERGY_SAMPLE_ORIGIN)
                });
                match live {
                    // Samples never replace a live reading, which therefore
                    // stays what the next reading is measured against.
                    Some(live) if moment.measurement.is_none() => {
                        readings.push(reading(
                            system_id,
                            &source,
                            at,
                            received_at,
                            moment,
                            &channels,
                            &mut baseline.clone(),
                            &mut rejected,
                        ));
                        revisions.extend(rederive(live, &mut baseline));
                    }
                    _ => readings.push(reading(
                        system_id,
                        &source,
                        at,
                        received_at,
                        moment,
                        &channels,
                        &mut baseline,
                        &mut rejected,
                    )),
                }
            }
            let count = readings.len();
            readings.extend(revisions);
            let outcomes = repositories
                .ingest
                .upsert_observations(&readings)
                .await
                .map_err(unavailable)?;
            // Only measurements are counted; a reading built from samples is
            // counted with its samples, and a revised reading not at all.
            for (reading, outcome) in readings.iter().zip(outcomes).take(count) {
                if reading.fills_gap {
                    continue;
                }
                match outcome {
                    UpsertOutcome::Inserted | UpsertOutcome::Replaced => {
                        response.measurements.stored += 1;
                    }
                    UpsertOutcome::Unchanged | UpsertOutcome::Kept => {
                        response.measurements.unchanged += 1;
                    }
                }
            }
            response.measurements.rejected = rejected;
        }

        let samples: Vec<EnergySampleRecord> = request
            .energy_samples
            .iter()
            .map(|sample| EnergySampleRecord {
                system_id,
                device_key: sample.device_id.clone(),
                measured_at: millis(sample.measured_at),
                energy_lifetime_wh: sample.energy_lifetime_wh,
                power_watts: sample.power_w,
                data_source_id: source.data_source_id,
                received_at,
            })
            .collect();
        if !samples.is_empty() {
            let stored = repositories
                .ingest
                .save_energy_samples(&samples)
                .await
                .map_err(unavailable)?;
            response.energy_samples = counts(samples.len(), stored);
        }

        let summaries = request
            .daily_summaries
            .iter()
            .map(|summary| {
                let device = summary.device_id.clone().unwrap_or_default();
                let held = &mut marks.of(&device).summary_date;
                if held.as_ref().is_none_or(|held| *held < summary.date) {
                    *held = Some(summary.date.clone());
                }
                Ok(SourceDailySummaryRecord {
                    system_id,
                    source_key: source.key.clone(),
                    device_key: device,
                    local_date: summary.date.clone(),
                    data_source_id: source.data_source_id,
                    generation_energy_wh: summary.energy_wh,
                    export_energy_wh: summary.export_energy_wh,
                    consumption_energy_wh: summary.consumption_energy_wh,
                    generation_lifetime_wh: summary.energy_lifetime_wh,
                    complete: summary.complete,
                    peak_power_watts: summary.peak_power_w,
                    peak_time: summary.peak_time.clone(),
                    weather_condition: summary.weather_condition.clone(),
                    temperature_min_millidegrees_c: summary.temperature_min_millicelsius,
                    temperature_max_millidegrees_c: summary.temperature_max_millicelsius,
                    import_energy_by_band: serde_json::to_value(&summary.import_energy_wh_by_band)
                        .map_err(unavailable)?,
                    received_at,
                })
            })
            .collect::<Result<Vec<_>, IngestApiError>>()?;
        if !summaries.is_empty() {
            let stored = repositories
                .ingest
                .save_daily_summaries(&summaries)
                .await
                .map_err(unavailable)?;
            response.daily_summaries = counts(summaries.len(), stored);
        }

        let events: Vec<DeviceEventRecord> = request
            .events
            .iter()
            .map(|event| {
                let occurred_at = millis(event.occurred_at);
                later(&mut marks.of(&event.device_id).event_at, occurred_at);
                DeviceEventRecord {
                    id: Uuid::now_v7(),
                    system_id,
                    device_key: event.device_id.clone(),
                    event_key: crate::device_events::event_key(
                        event.event_id.as_deref(),
                        occurred_at,
                        event.code,
                    ),
                    data_source_id: source.data_source_id,
                    occurred_at,
                    code: event.code,
                    message: event.message.clone(),
                    details: DeviceEventDetails {
                        event_type: event.event_type.clone(),
                        category: event.category.clone(),
                        group: event.group.clone(),
                        old_value: event.old_value.clone(),
                        new_value: event.new_value.clone(),
                        user_group: event.user_group.clone(),
                    },
                    received_at,
                }
            })
            .collect();
        if !events.is_empty() {
            let stored = repositories
                .events
                .save_device_events(&events)
                .await
                .map_err(unavailable)?;
            response.events = counts(events.len(), stored);
        }

        let advanced: Vec<IngestionMarkRecord> = marks.0.into_values().collect();
        if !advanced.is_empty() {
            repositories
                .ingest
                .advance_marks(system_id, &source.key, &advanced)
                .await
                .map_err(unavailable)?;
        }
        response.marks = marks_response(
            repositories
                .ingest
                .marks(system_id, &source.key)
                .await
                .map_err(unavailable)?,
        );
        Ok(response)
    }

    async fn state(
        &self,
        system_id: SystemId,
        writer: IngestWriter,
    ) -> Result<IngestMarks, IngestApiError> {
        let repositories = self.repositories(system_id).await?;
        let source = Self::source(&repositories, system_id, &writer).await?;
        Ok(marks_response(
            repositories
                .ingest
                .marks(system_id, &source.key)
                .await
                .map_err(unavailable)?,
        ))
    }
}
