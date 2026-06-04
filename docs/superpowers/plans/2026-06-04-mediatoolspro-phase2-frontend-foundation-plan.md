# MediaToolsPro Phase 2 Frontend Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tested frontend foundation and begin decomposing the downloader workspace without changing visible behavior.

**Architecture:** First add a Vitest harness and extract pure logic that can be tested without Tauri. Then add design tokens and reusable UI primitives. Finally split the large downloader workspace by responsibility in small behavior-preserving slices, keeping `pnpm build` green after every slice.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Vite, Vitest, Tailwind CSS v4, Tauri 2.

---

## Files

- Create: `src/design/tokens.css`
- Create: `src/components/ui/BaseButton.vue`
- Create: `src/components/ui/BaseCard.vue`
- Create: `src/components/ui/BaseField.vue`
- Create: `src/components/ui/BaseInput.vue`
- Create: `src/components/ui/BaseSelect.vue`
- Create: `src/components/ui/BaseToggle.vue`
- Create: `src/components/ui/BaseProgressBar.vue`
- Create: `src/components/ui/BaseSegmentedControl.vue`
- Create: `src/components/ui/BaseModal.vue`
- Create: `src/components/ui/BaseToast.vue`
- Create: `src/utils/time.ts`
- Create: `src/utils/time.test.ts`
- Create: `src/composables/useSettings.ts`
- Create: `src/composables/useSettings.test.ts`
- Create: `src/composables/useQueue.ts`
- Create: `src/composables/useQueue.test.ts`
- Modify: `src/main.ts`
- Modify: `package.json`
- Modify: `vite.config.ts`
- Later split: `src/views/Downloader.vue` into `DownloadView.vue`, `StudioView.vue`, `QueueView.vue`, and composables.

## Task 1: Vitest Harness

- [ ] Add `vitest`, `@vue/test-utils`, and `jsdom` as dev dependencies.
- [ ] Add script `"test": "vitest run"` to `package.json`.
- [ ] Add `test: { environment: "jsdom" }` to `vite.config.ts`.
- [ ] Create `src/utils/time.ts` with `formatDisplayTime`, `parseTimeInput`, `clamp`, and `normalizeTrimBounds`.
- [ ] Create tests in `src/utils/time.test.ts` for plain seconds, `mm:ss`, invalid input, and minimum trim gap.
- [ ] Run `pnpm test` and `pnpm build`.
- [ ] Commit `test(frontend): add vitest harness and time utility tests`.

## Task 2: Design Tokens

- [ ] Create `src/design/tokens.css` with CSS variables for surfaces, text, borders, accent, status colors, spacing, radius, shadow, and focus ring in light and dark.
- [ ] Import `./design/tokens.css` from `src/main.ts` before `style.css`.
- [ ] Run `pnpm build`.
- [ ] Commit `feat(design): add shared design tokens`.

## Task 3: UI Primitives

- [ ] Create UI primitives under `src/components/ui/` using native buttons, labels, inputs, selects, checkboxes, progress roles, dialogs, and slots.
- [ ] Keep components style-only and behavior-light so they can be adopted incrementally.
- [ ] Create one smoke test that mounts representative primitives and verifies labels/roles render.
- [ ] Run `pnpm test` and `pnpm build`.
- [ ] Commit `feat(ui): add reusable primitive components`.

## Task 4: Settings Composable

- [ ] Create `src/composables/useSettings.ts` with the storage keys, defaults, `settingsKeyForMode`, `legacySettingsKeyForMode`, `defaultSettingsForMode`, `sanitizeSettings`, `restorePersistedSettings`, and `savePersistedSettings`.
- [ ] Create tests for audio defaults, video defaults, legacy fallback, and invalid persisted values.
- [ ] Wire `Downloader.vue` to use these pure helpers while leaving its refs and template in place.
- [ ] Run `pnpm test` and `pnpm build`.
- [ ] Commit `refactor(frontend): extract settings persistence helpers`.

## Task 5: Queue Composable

- [ ] Create `src/composables/useQueue.ts` with `isActiveStatus`, `isTerminalStatus`, `queueStats`, `restoreJobRequest`, `restoreQueueItem`, and `applyJobProgress`.
- [ ] Create tests for active/terminal states, restoring active items as cancelled for Phase 2 parity, and progress updates.
- [ ] Wire `Downloader.vue` to use the pure helpers while leaving queue rendering unchanged.
- [ ] Run `pnpm test` and `pnpm build`.
- [ ] Commit `refactor(frontend): extract queue state helpers`.

## Task 6: View Split Preparation

- [ ] Rename `src/views/Downloader.vue` to `src/views/ToolsView.vue` and update `src/App.vue`.
- [ ] Create empty `DownloadView.vue`, `StudioView.vue`, and `QueueView.vue` wrappers only when their props/context are ready; do not duplicate state.
- [ ] Run `pnpm build`.
- [ ] Commit `refactor(frontend): rename downloader workspace view`.

## Acceptance

- `pnpm test` passes.
- `pnpm build` passes.
- Existing tabbed UI still renders through the same App shell.
- Settings and queue pure logic have automated coverage.
- The next Phase 2 continuation can split visual sections with lower risk because the state helpers are already tested.
