/**
 * A simulated system with a hybrid inverter, a battery and a grid meter, as
 * smalog would report it through the ingestion API. Its generation follows the
 * real generation of the base system on the same calendar day, scaled to its
 * own size; its household, battery and grid follow from that. Counters and
 * the battery's charge carry over in a state object, so that a simulation can
 * stop and continue later where it stopped.
 */

import { householdPowerW } from "./household.mjs";
import { localTime } from "./localTime.mjs";
import { seeded } from "./random.mjs";

/** What the simulated system consists of. */
export const LIVE_SYSTEM = {
  name: "Demo Live",
  household: 2,
  inverter: {
    id: "3014000001",
    kind: "inverter",
    familyId: 9483,
    configuredName: "Hybrid",
    reportedName: "SN: 3014000001",
    model: "STP 8.0-3SE-40",
    firmware: "4.1.12.R",
    transport: "ethernet",
  },
  /** Installed capacity per tracker in watts peak. */
  trackers: [
    {
      number: 1,
      capacityW: 4800,
      share: 0.6,
      name: "Süd",
      orientation: 180,
      tilt: 35,
    },
    {
      number: 2,
      capacityW: 3200,
      share: 0.4,
      name: "West",
      orientation: 260,
      tilt: 30,
    },
  ],
  maximumAcW: 8000,
  battery: {
    capacityWh: 10_000,
    maximumW: 4600,
    efficiency: 0.95,
    reservePermille: 50,
  },
};

/** A state before the first simulated instant. */
export function initialState(at) {
  return {
    lastAt: at,
    generationLifetimeWh: 1_250_000,
    consumptionLifetimeWh: 2_100_000,
    batteryChargeWh: LIVE_SYSTEM.battery.capacityWh * 0.5,
    operatingTimeS: 6_400_000,
    feedInTimeS: 5_900_000,
    day: null,
  };
}

/** Generation of the base system at an instant, interpolated between its five-minute slots. */
function baseGenerationW(profile, epochSeconds) {
  const { monthDay, minuteOfDay } = localTime(epochSeconds);
  const day =
    profile[monthDay] ?? profile[monthDay === "02-29" ? "02-28" : monthDay];
  if (!day) return 0;
  const slot = minuteOfDay / 5;
  const low = Math.floor(slot);
  const high = Math.min(low + 1, day.length - 1);
  return day[low] + (day[high] - day[low]) * (slot - low);
}

function newDay(date) {
  return {
    date,
    energyWh: 0,
    exportWh: 0,
    consumptionWh: 0,
    peakW: 0,
    peakTime: null,
  };
}

/**
 * Simulates one instant and advances the state.
 *
 * @param {object} state - Mutated: counters, battery charge and the running day.
 * @param {number} at - Epoch seconds of the measurement.
 * @param {{ profile: Record<string, number[]>, scale: number }} base
 * @returns {{ measurement: object, finishedDay: object | null }}
 */
export function simulate(state, at, base) {
  const seconds = Math.max(0, at - state.lastAt);
  const hours = seconds / 3600;
  const random = seeded(`live:${at}`);
  const { date, minuteOfDay } = localTime(at);
  let finishedDay = null;
  if (state.day?.date !== date) {
    finishedDay = state.day;
    state.day = newDay(date);
  }

  const pvW = Math.min(
    LIVE_SYSTEM.maximumAcW,
    Math.max(
      0,
      Math.round(
        baseGenerationW(base.profile, at) *
          base.scale *
          (0.97 + random() * 0.06),
      ),
    ),
  );
  const loadW = householdPowerW(at, LIVE_SYSTEM.household);

  // The battery takes the surplus and covers the shortfall within its limits.
  const { capacityWh, maximumW, efficiency, reservePermille } =
    LIVE_SYSTEM.battery;
  const reserveWh = (capacityWh * reservePermille) / 1000;
  let batteryW = 0;
  const surplus = pvW - loadW;
  if (surplus > 0 && hours > 0) {
    const room = (capacityWh - state.batteryChargeWh) / efficiency / hours;
    batteryW = Math.max(0, Math.min(surplus, maximumW, room));
  } else if (surplus < 0 && hours > 0) {
    const available =
      ((state.batteryChargeWh - reserveWh) * efficiency) / hours;
    batteryW = -Math.max(0, Math.min(-surplus, maximumW, available));
  }
  batteryW = Math.round(batteryW);
  const gridW = loadW + batteryW - pvW;
  const importW = Math.max(0, gridW);
  const exportW = Math.max(0, -gridW);

  const pvWh = pvW * hours;
  const loadWh = loadW * hours;
  state.batteryChargeWh +=
    batteryW > 0
      ? batteryW * hours * efficiency
      : (batteryW * hours) / efficiency;
  state.batteryChargeWh = Math.min(
    capacityWh,
    Math.max(0, state.batteryChargeWh),
  );
  state.generationLifetimeWh += pvWh;
  state.consumptionLifetimeWh += loadWh;
  if (pvW > 0) {
    state.operatingTimeS += seconds;
    state.feedInTimeS += seconds;
  }
  state.lastAt = at;
  const day = state.day;
  day.energyWh += pvWh;
  day.exportWh += exportW * hours;
  day.consumptionWh += loadWh;
  if (pvW > day.peakW) {
    day.peakW = pvW;
    day.peakTime = `${String(Math.floor(minuteOfDay / 60)).padStart(2, "0")}:${String(minuteOfDay % 60).padStart(2, "0")}`;
  }

  const soc = Math.round((state.batteryChargeWh / capacityWh) * 1000);
  const voltage = () => 229_000 + Math.round(random() * 3500);
  const phases = [1, 2, 3].map((phase) => {
    const voltageMv = voltage();
    const powerW = Math.round(pvW / 3);
    return {
      phase,
      powerW,
      currentMa: Math.round((powerW / voltageMv) * 1_000_000),
      voltageMv,
    };
  });
  const trackers = LIVE_SYSTEM.trackers.map((tracker) => {
    const powerW = Math.round((pvW / 0.97) * tracker.share);
    const voltageMv = pvW > 0 ? 330_000 + Math.round(random() * 80_000) : 0;
    return {
      number: tracker.number,
      powerW,
      currentMa:
        voltageMv > 0 ? Math.round((powerW / voltageMv) * 1_000_000) : 0,
      voltageMv,
    };
  });
  const batteryVoltageMv = 48_000 + soc * 6;
  const measurement = {
    measuredAt: at,
    system: {
      consumptionPowerW: loadW,
      consumptionLifetimeWh: Math.round(state.consumptionLifetimeWh),
      batteryPowerW: batteryW,
      gridImportEnergyWh: Math.round(importW * hours),
      gridExportEnergyWh: Math.round(exportW * hours),
    },
    devices: [
      {
        id: LIVE_SYSTEM.inverter.id,
        ac: phases,
        gridFrequencyMhz: 49_960 + Math.round(random() * 80),
        gridImportPowerW: importW,
        gridExportPowerW: exportW,
        energyTodayWh: Math.round(day.energyWh),
        energyLifetimeWh: Math.round(state.generationLifetimeWh),
        operatingTimeS: Math.round(state.operatingTimeS),
        feedInTimeS: Math.round(state.feedInTimeS),
        statusCode: pvW > 0 ? 307 : 303,
        gridRelayStatusCode: pvW > 0 ? 51 : 311,
        temperatureMillicelsius:
          18_000 + Math.round(pvW * 3.5 + random() * 2000),
        trackers,
        battery: {
          stateOfChargePermille: soc,
          voltageMv: batteryVoltageMv,
          currentMa: Math.round((batteryW / batteryVoltageMv) * 1_000_000),
          temperatureMillicelsius: 21_000 + Math.round(random() * 3000),
        },
      },
    ],
  };
  return { measurement, finishedDay };
}

/** The daily summary the inverter reports for a day. */
export function daySummary(day, complete) {
  const summary = {
    deviceId: LIVE_SYSTEM.inverter.id,
    date: day.date,
    energyWh: Math.round(day.energyWh),
    complete,
    exportEnergyWh: Math.round(day.exportWh),
    consumptionEnergyWh: Math.round(day.consumptionWh),
    peakPowerW: Math.round(day.peakW),
  };
  if (day.peakTime) summary.peakTime = day.peakTime;
  return summary;
}
