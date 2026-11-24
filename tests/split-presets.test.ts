import assert from "node:assert/strict";
import test from "node:test";
import {
  parseSplitPresets,
  validOutputNamePattern,
  type SplitPreset,
} from "../src/lib/split-presets.ts";
import { compatibleEncoding } from "../src/lib/export-options.ts";

test("saved Split settings round-trip and unreadable entries are ignored", () => {
  const preset: SplitPreset = {
    id: "mix",
    name: "Mezclas",
    mode: "duration",
    duration: 180,
    snapToPause: true,
    pauseWindow: 5,
    snapMinSilence: 0.3,
    thresholdDb: -35,
    minSilence: 1.5,
    minSegment: 20,
    fadeIn: 0.5,
    fadeOut: 1.25,
    normalization: "after",
    loudness: -14,
    truePeak: -1.5,
    format: "flac",
    outputNamePattern: "Pista {numero} - {archivo}",
  };
  assert.deepEqual(parseSplitPresets(JSON.stringify([preset, { ...preset, pauseWindow: 80 }, null])), [preset]);
  assert.deepEqual(parseSplitPresets("{"), []);
});

test("split export names stay in the destination and require a segment number", () => {
  assert.equal(validOutputNamePattern(" "), true);
  assert.equal(validOutputNamePattern("Pista {numero} - {archivo}"), true);
  for (const pattern of ["../{numero}", "a\\{numero}", "{archivo}", "{numero}_{otro}", "a\u0000{numero}"]) {
    assert.equal(validOutputNamePattern(pattern), false, pattern);
  }
});

test("a preset uses only the options supported by the selected format", () => {
  const requested = { sampleRate: 96000, channels: 2, pcmBitDepth: 24, audioBitrateKbps: 160 };
  assert.deepEqual(compatibleEncoding("aiff", requested), {
    sampleRate: 96000, channels: 2, pcmBitDepth: 24,
  });
  assert.deepEqual(compatibleEncoding("opus", requested), {
    channels: 2, audioBitrateKbps: 160,
  });
});
