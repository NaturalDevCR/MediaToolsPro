import { test } from "node:test";
import assert from "node:assert/strict";
import {
  quietRegions,
  inspectLevel,
  dbHeight,
  suggestThreshold,
} from "../src/lib/waveform.ts";
const levels = {
  intervalSeconds: 0.02,
  peaksDb: [-18, -45, -50, -42, -15, -120],
  rmsDb: [-21, -48, -53, -45, -18, -120],
};
test("inspector reads the clicked 20 ms window, including first and last samples", () => {
  assert.equal(inspectLevel(levels, 0.041)?.peakDb, -50);
  assert.equal(inspectLevel(levels, -1)?.peakDb, -18);
  assert.equal(inspectLevel(levels, 0.12)?.peakDb, -120);
  assert.equal(inspectLevel({ ...levels, peaksDb: [] }, 0), null);
});
test("quiet regions respond to the threshold and keep leading/trailing silence", () => {
  assert.deepEqual(quietRegions(levels, -35, 0.115), [
    { startSeconds: 0.02, endSeconds: 0.08 },
    { startSeconds: 0.1, endSeconds: 0.115 },
  ]);
  assert.deepEqual(quietRegions(levels, -48, 0.12), [
    { startSeconds: 0.04, endSeconds: 0.06 },
    { startSeconds: 0.1, endSeconds: 0.12 },
  ]);
  assert.deepEqual(quietRegions(levels, NaN, 0.12), []);
});
test("waveform and threshold use the same dB scale at any zoom", () => {
  assert.equal(dbHeight(-80), 0);
  assert.equal(dbHeight(-40), 0.5);
  assert.equal(dbHeight(0), 1);
  assert.equal(dbHeight(-120), 0);
  assert.equal(dbHeight(6), 1);
});
test("threshold suggestion gives measured peak 3 dB of margin within supported limits", () => {
  assert.equal(suggestThreshold(-43.8), -40);
  assert.equal(suggestThreshold(-120), -80);
  assert.equal(suggestThreshold(-4), -5);
});
