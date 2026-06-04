import { beforeEach, describe, expect, it } from "vitest";
import {
  LEGACY_SETTINGS_STORAGE_KEY,
  SETTINGS_STORAGE_KEY,
  defaultSettingsForMode,
  legacySettingsKeyForMode,
  restorePersistedSettings,
  sanitizeSettings,
  savePersistedSettings,
  settingsKeyForMode,
} from "./useSettings";

describe("settings persistence helpers", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("returns mode-specific defaults", () => {
    expect(defaultSettingsForMode("audio")).toMatchObject({
      format: "mp3",
      batchFormat: "mp3",
      normalizeAudio: true,
    });
    expect(defaultSettingsForMode("video")).toMatchObject({
      format: "mp4",
      batchFormat: "mp4",
      normalizeAudio: false,
    });
  });

  it("sanitizes invalid persisted values", () => {
    const settings = sanitizeSettings(
      {
        format: "exe",
        batchFormat: "flac",
        loudnessTargetLufs: 20,
        downloadAudioTarget: "azuracast",
        eq: { bass: 4 },
        fadeInDuration: -2,
        downloadPipelineSplitMode: "manual",
      },
      "audio",
    );

    expect(settings.format).toBe("mp3");
    expect(settings.batchFormat).toBe("flac");
    expect(settings.loudnessTargetLufs).toBe(-6);
    expect(settings.downloadAudioTarget).toBe("azuracast");
    expect(settings.eq).toEqual({ bass: 4, mid: 0, treble: 0 });
    expect(settings.fadeInDuration).toBe(0);
    expect(settings.downloadPipelineSplitMode).toBe("none");
  });

  it("restores from the legacy key when the current key is absent", () => {
    window.localStorage.setItem(
      legacySettingsKeyForMode("video"),
      JSON.stringify({ format: "webm", batchFormat: "mkv", quality: "720" }),
    );

    const restored = restorePersistedSettings("video");
    expect(restored.format).toBe("webm");
    expect(restored.batchFormat).toBe("mkv");
    expect(restored.quality).toBe("720");
  });

  it("saves mode settings and active mode metadata", () => {
    const settings = defaultSettingsForMode("audio");
    savePersistedSettings("audio", settings, "video");

    expect(window.localStorage.getItem(settingsKeyForMode("audio"))).toBe(JSON.stringify(settings));
    expect(window.localStorage.getItem(SETTINGS_STORAGE_KEY)).toBe(JSON.stringify({ activeMode: "video" }));
    expect(window.localStorage.getItem(LEGACY_SETTINGS_STORAGE_KEY)).toBeNull();
  });
});
