/**
 * Delivers the simulated system's measurements for a span of time through the
 * ingestion API, in batches, and keeps the state only once a batch arrived.
 */

import { daySummary, LIVE_SYSTEM, simulate } from "./liveSystem.mjs";
import { ingest } from "./pvlogClient.mjs";

const BATCH = 2000;

/**
 * Simulates and delivers every `stepSeconds` after the state's last instant up
 * to `until`.
 *
 * @param {{ baseUrl: string, systemId: string, apiKey: string }} target
 * @param {object} state - Advanced in place.
 * @param {{ profile: Record<string, number[]>, scale: number }} base
 * @param {{ until: number, stepSeconds: number, onBatch?: (state: object, delivered: number) => void }} options
 * @returns {Promise<number>} Measurements delivered.
 */
export async function deliverLive(
  target,
  state,
  base,
  { until, stepSeconds, onBatch },
) {
  let delivered = 0;
  let next = (Math.floor(state.lastAt / stepSeconds) + 1) * stepSeconds;
  while (next <= until) {
    const before = structuredClone(state);
    const measurements = [];
    const dailySummaries = [];
    for (; next <= until && measurements.length < BATCH; next += stepSeconds) {
      const { measurement, finishedDay } = simulate(state, next, base);
      if (finishedDay) dailySummaries.push(daySummary(finishedDay, true));
      measurements.push(measurement);
    }
    // The running day matures with every delivery.
    dailySummaries.push(daySummary(state.day, false));
    try {
      await ingest(target, {
        source: { name: "pvlog-demo-simulator", version: "1" },
        devices: [LIVE_SYSTEM.inverter],
        measurements,
        dailySummaries,
      });
    } catch (error) {
      Object.assign(state, before);
      throw error;
    }
    delivered += measurements.length;
    onBatch?.(state, delivered);
  }
  return delivered;
}
