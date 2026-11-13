/**
 * A household's electricity use over the day: a base load with a cycling
 * fridge, morning and evening peaks, more light in winter, and appliances
 * (washing machine, dishwasher, oven) that run on some days at varying times.
 * Every value follows from the instant and a seed, so it can be produced in
 * any order and again later.
 */

import { localTime } from "./localTime.mjs";
import { seeded } from "./random.mjs";

/** Bell curve around a minute of the day. */
function bell(minute, centre, width) {
  const distance = (minute - centre) / width;
  return Math.exp(-0.5 * distance * distance);
}

const dayPlans = new Map();

/** Which appliances run on a day, and when. */
function dayPlan(date, weekend, seed) {
  const key = `${seed}:${date}`;
  const cached = dayPlans.get(key);
  if (cached) return cached;
  const random = seeded(key);
  const runs = [];
  const run = (probability, watts, earliest, latest, minutes) => {
    if (random() >= probability) return;
    const start = earliest + Math.floor(random() * (latest - earliest));
    runs.push({ start, end: start + minutes, watts });
  };
  run(weekend ? 0.7 : 0.35, 2000, 8 * 60, 17 * 60, 90); // washing machine
  run(0.25, 2200, 10 * 60, 20 * 60, 75); // dryer
  run(0.65, 1800, 19 * 60, 22 * 60, 60); // dishwasher
  run(weekend ? 0.6 : 0.3, 2400, 11 * 60, 18 * 60 + 30, 45); // oven
  run(0.15, 2500, 6 * 60, 9 * 60, 20); // kettle and coffee
  const plan = { runs, mood: 0.85 + random() * 0.3 };
  if (dayPlans.size > 5000) dayPlans.clear();
  dayPlans.set(key, plan);
  return plan;
}

/**
 * Power the household draws at an instant, in watts.
 *
 * @param {number} epochSeconds
 * @param {number} [seed]
 * @returns {number}
 */
export function householdPowerW(epochSeconds, seed = 1) {
  const { date, minuteOfDay, weekend, month } = localTime(epochSeconds);
  const plan = dayPlan(date, weekend, seed);
  // Darker months need more light and heating pumps.
  const winter = Math.cos(((month - 1) / 12) * 2 * Math.PI) * 0.5 + 0.5;
  let watts = 170 + 60 * winter;
  // The fridge runs about a third of the time.
  if (Math.floor(epochSeconds / 60) % 45 < 16) watts += 90;
  watts +=
    (weekend ? 250 : 550) *
    bell(minuteOfDay, weekend ? 9 * 60 : 6 * 60 + 50, 40);
  watts += (weekend ? 500 : 250) * bell(minuteOfDay, 12 * 60 + 30, 45);
  watts += (450 + 350 * winter) * bell(minuteOfDay, 19 * 60 + 30, 90);
  watts += 150 * winter * bell(minuteOfDay, 17 * 60, 60);
  for (const run of plan.runs)
    if (minuteOfDay >= run.start && minuteOfDay < run.end) watts += run.watts;
  // A little jitter so that two days never look the same.
  const jitter = seeded(`${seed}:${epochSeconds}`)() * 0.2 - 0.1;
  return Math.max(80, Math.round(watts * plan.mood * (1 + jitter)));
}
