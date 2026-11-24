import { test } from "node:test";
import assert from "node:assert/strict";
import * as waveform from "../src/lib/waveform.ts";
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

test("a two-hour recording can zoom to a five-second viewport", () => {
  const duration = 2 * 3600 + 16 * 60;
  const width = 1000;
  const spans = waveform.waveformZoomSpans(duration, width);
  assert.equal(spans[0], duration);
  assert.ok(spans.includes(60));
  assert.ok(spans.includes(15));
  assert.equal(spans.at(-1), 5);
  const trackWidth = waveform.waveformTrackWidth(duration, 5, width);
  assert.deepEqual(waveform.waveformVisibleRange(duration, trackWidth, width, 0), {
    startSeconds: 0,
    endSeconds: 5,
  });
});

test("very long recordings stay within the scroll width limit", () => {
  const duration = 24 * 3600;
  const width = 1920;
  const span = waveform.waveformZoomSpans(duration, width).at(-1)!;
  const trackWidth = waveform.waveformTrackWidth(duration, span, width);
  assert.ok(span > 5);
  assert.equal(trackWidth, 4_000_000);
  const offset = waveform.waveformScrollOffset(duration, width / 2, duration, trackWidth, width);
  const visible = waveform.waveformVisibleRange(duration, trackWidth, width, offset);
  assert.equal(visible.endSeconds, duration);
  assert.ok(Math.abs(duration - visible.startSeconds - span) < 0.001);
});

test("zoom keeps the chosen time at its viewport position", () => {
  const width = 1000;
  const trackWidth = waveform.waveformTrackWidth(8000, 5, width);
  const offset = waveform.waveformScrollOffset(7200, 250, 8000, trackWidth, width);
  assert.deepEqual(waveform.waveformVisibleRange(8000, trackWidth, width, offset), {
    startSeconds: 7198.75,
    endSeconds: 7203.75,
  });
});

test("a zoomed ruler uses absolute timestamps and shows fractional seconds", () => {
  const ruler = waveform.waveformRulerTicks(7198.75, 7203.75, 1000);
  assert.equal(ruler.step, 0.5);
  assert.equal(ruler.seconds[0], 7199);
  assert.ok(ruler.seconds.includes(7200.5));
  assert.equal(ruler.seconds.at(-1), 7203.5);
});

test("a pixel shows the highest peak and mean RMS power in its time interval", () => {
  const envelope = waveform.createWaveformEnvelope({
    intervalSeconds: 0.02,
    peaksDb: [-5, -35, -8, -20],
    rmsDb: [-10, -40, -20, -30],
  });
  const pixel = envelope(0.02, 0.06);
  assert.equal(pixel.peakDb, -8);
  assert.ok(Math.abs(pixel.rmsDb - 10 * Math.log10((0.0001 + 0.01) / 2)) < 1e-12);
  assert.deepEqual(envelope(0.08, 0.09), { peakDb: -120, rmsDb: -120 });
});

test("cached waveform blocks preserve narrow peaks over a large pixel interval", () => {
  const peaksDb = Array.from({ length: 192 }, (_, i) => i === 100 ? -3 : -40);
  const rmsDb = peaksDb.map((peak) => peak - 3);
  const envelope = waveform.createWaveformEnvelope({ intervalSeconds: 0.02, peaksDb, rmsDb });
  assert.equal(envelope(0.02, 3.5).peakDb, -3);
  assert.equal(envelope(2.02, 3.5).peakDb, -40);
});
