<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle, XCircle, Download, Trash2, RotateCw, AlertCircle, ArrowUpCircle } from 'lucide-vue-next';

interface BinaryStatus {
  ytdlp: boolean;
  ytdlp_version: string | null;
  ffmpeg: boolean;
  ffmpeg_version: string | null;
}

interface BinaryUpdateStatus {
  ytdlp_update_available: boolean;
  ytdlp_latest_version: string | null;
  ffmpeg_update_available: boolean;
  ffmpeg_latest_version: string | null;
}

const status = ref<BinaryStatus>({
  ytdlp: false,
  ytdlp_version: null,
  ffmpeg: false,
  ffmpeg_version: null,
});

const updateStatus = ref<BinaryUpdateStatus>({
  ytdlp_update_available: false,
  ytdlp_latest_version: null,
  ffmpeg_update_available: false,
  ffmpeg_latest_version: null,
});

const loading = ref<{ [key: string]: boolean }>({});
const checking = ref(true);
const checkingUpdates = ref(false);
const updatesChecked = ref(false);

async function checkStatus() {
  checking.value = true;
  try {
    const result = await invoke<BinaryStatus>('check_binaries');
    status.value = result;
  } catch (e) {
    console.error('Failed to check binaries', e);
  } finally {
    checking.value = false;
  }
}

async function checkUpdates() {
  checkingUpdates.value = true;
  try {
    const result = await invoke<BinaryUpdateStatus>('check_binary_updates');
    updateStatus.value = result;
    updatesChecked.value = true;
  } catch (e) {
    console.error('Failed to check for updates', e);
  } finally {
    checkingUpdates.value = false;
  }
}

async function install(binary: 'ytdlp' | 'ffmpeg') {
  loading.value[binary] = true;
  try {
    if (binary === 'ytdlp') await invoke('install_ytdlp');
    if (binary === 'ffmpeg') await invoke('install_ffmpeg');
    updateStatus.value = {
      ytdlp_update_available: false,
      ytdlp_latest_version: null,
      ffmpeg_update_available: false,
      ffmpeg_latest_version: null,
    };
    updatesChecked.value = false;
    await checkStatus();
  } catch (e) {
    console.error(`Failed to install ${binary}`, e);
    alert(`Failed to install: ${e}`);
  } finally {
    loading.value[binary] = false;
  }
}

async function remove(binary: 'ytdlp' | 'ffmpeg') {
  if (!confirm(`Are you sure you want to remove ${binary}?`)) return;

  loading.value[binary] = true;
  try {
    if (binary === 'ytdlp') await invoke('delete_ytdlp');
    if (binary === 'ffmpeg') await invoke('delete_ffmpeg');
    await checkStatus();
  } catch (e) {
    console.error(`Failed to delete ${binary}`, e);
  } finally {
    loading.value[binary] = false;
  }
}

onMounted(() => {
  checkStatus();
});
</script>

<template>
  <div class="p-8 h-full flex flex-col gap-8 max-w-4xl mx-auto">
    <div class="flex items-center justify-between">
      <h1 class="text-3xl font-light text-gray-800 dark:text-gray-100 flex items-center gap-3">
        Settings & Binaries
        <button @click="checkStatus" class="p-2 rounded-full hover:bg-gray-200 dark:hover:bg-gray-800 transition-colors" title="Refresh Status">
          <RotateCw class="w-5 h-5 text-gray-500" :class="{ 'animate-spin': checking }" />
        </button>
      </h1>
      <button
        @click="checkUpdates"
        :disabled="checkingUpdates"
        class="flex items-center gap-2 px-4 py-2 text-sm font-medium text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors disabled:opacity-50"
      >
        <RotateCw v-if="checkingUpdates" class="w-4 h-4 animate-spin" />
        <AlertCircle v-else class="w-4 h-4" />
        {{ checkingUpdates ? 'Checking...' : 'Check for updates' }}
      </button>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">

      <!-- YT-DLP Card -->
      <div class="bg-white/40 dark:bg-white/5 backdrop-blur-xl border border-white/20 dark:border-white/10 p-6 rounded-2xl shadow-sm flex flex-col gap-4 transition-all hover:shadow-md">
        <div class="flex items-center justify-between">
          <h2 class="text-xl font-medium text-gray-700 dark:text-gray-200">yt-dlp</h2>
          <div class="flex items-center gap-2">
            <template v-if="checking">
              <RotateCw class="w-5 h-5 text-blue-500 animate-spin" />
              <span class="text-sm font-medium text-blue-500">Checking...</span>
            </template>
            <template v-else>
              <CheckCircle v-if="status.ytdlp" class="w-5 h-5 text-green-500" />
              <XCircle v-else class="w-5 h-5 text-red-500" />
              <span :class="status.ytdlp ? 'text-green-600 dark:text-green-400' : 'text-red-500'" class="text-sm font-medium">
                {{ status.ytdlp ? 'Installed' : 'Missing' }}
              </span>
            </template>
          </div>
        </div>

        <div class="flex flex-col gap-1.5 min-h-[3em]">
          <div class="text-sm text-gray-500 dark:text-gray-400 flex items-center gap-2">
            <span class="text-gray-400 dark:text-gray-500 text-xs">Version</span>
            <span v-if="status.ytdlp_version" class="text-gray-700 dark:text-gray-300">{{ status.ytdlp_version }}</span>
            <span v-else-if="status.ytdlp" class="text-gray-400 dark:text-gray-500">N/A</span>
            <span v-else class="text-gray-400 dark:text-gray-500">&mdash;</span>
          </div>

          <div v-if="updatesChecked && status.ytdlp" class="text-sm flex items-center gap-1.5">
            <template v-if="updateStatus.ytdlp_update_available">
              <ArrowUpCircle class="w-4 h-4 text-amber-500" />
              <span class="text-amber-600 dark:text-amber-400 font-medium">Update available: {{ updateStatus.ytdlp_latest_version }}</span>
            </template>
            <template v-else>
              <CheckCircle class="w-3.5 h-3.5 text-green-500" />
              <span class="text-green-600 dark:text-green-400">Up to date</span>
            </template>
          </div>
        </div>

        <div class="mt-auto flex gap-2">
          <button
            v-if="!status.ytdlp"
            @click="install('ytdlp')"
            :disabled="loading.ytdlp"
            class="flex-1 flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RotateCw v-if="loading.ytdlp" class="w-4 h-4 animate-spin" />
            <Download v-else class="w-4 h-4" />
            Install
          </button>
          <button
            v-else-if="updateStatus.ytdlp_update_available"
            @click="install('ytdlp')"
            :disabled="loading.ytdlp"
            class="flex-1 flex items-center justify-center gap-2 bg-amber-500 hover:bg-amber-600 text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
            <RotateCw v-if="loading.ytdlp" class="w-4 h-4 animate-spin" />
            <Download v-else class="w-4 h-4" />
            Update to {{ updateStatus.ytdlp_latest_version }}
          </button>
          <button
            v-else
            @click="install('ytdlp')"
            :disabled="loading.ytdlp"
            class="flex-1 flex items-center justify-center gap-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-800 dark:text-gray-200 px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
            <RotateCw v-if="loading.ytdlp" class="w-4 h-4 animate-spin" />
            <span v-else>Reinstall</span>
          </button>

          <button
            v-if="status.ytdlp"
            @click="remove('ytdlp')"
            :disabled="loading.ytdlp"
            class="p-2 text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors disabled:opacity-50"
            title="Uninstall"
          >
            <Trash2 class="w-5 h-5" />
          </button>
        </div>
      </div>

      <!-- FFmpeg Card -->
      <div class="bg-white/40 dark:bg-white/5 backdrop-blur-xl border border-white/20 dark:border-white/10 p-6 rounded-2xl shadow-sm flex flex-col gap-4 transition-all hover:shadow-md">
        <div class="flex items-center justify-between">
          <h2 class="text-xl font-medium text-gray-700 dark:text-gray-200">FFmpeg</h2>
          <div class="flex items-center gap-2">
            <template v-if="checking">
              <RotateCw class="w-5 h-5 text-blue-500 animate-spin" />
              <span class="text-sm font-medium text-blue-500">Checking...</span>
            </template>
            <template v-else>
              <CheckCircle v-if="status.ffmpeg" class="w-5 h-5 text-green-500" />
              <XCircle v-else class="w-5 h-5 text-red-500" />
              <span :class="status.ffmpeg ? 'text-green-600 dark:text-green-400' : 'text-red-500'" class="text-sm font-medium">
                {{ status.ffmpeg ? 'Installed' : 'Missing' }}
              </span>
            </template>
          </div>
        </div>

        <div class="flex flex-col gap-1.5 min-h-[3em]">
          <div class="text-sm text-gray-500 dark:text-gray-400 flex items-center gap-2">
            <span class="text-gray-400 dark:text-gray-500 text-xs">Version</span>
            <span v-if="status.ffmpeg_version" class="text-gray-700 dark:text-gray-300">{{ status.ffmpeg_version }}</span>
            <span v-else-if="status.ffmpeg" class="text-gray-400 dark:text-gray-500">N/A</span>
            <span v-else class="text-gray-400 dark:text-gray-500">&mdash;</span>
          </div>

          <div v-if="updatesChecked && status.ffmpeg" class="text-sm flex items-center gap-1.5">
            <template v-if="updateStatus.ffmpeg_update_available">
              <ArrowUpCircle class="w-4 h-4 text-amber-500" />
              <span class="text-amber-600 dark:text-amber-400 font-medium">Update available</span>
            </template>
            <template v-else>
              <CheckCircle class="w-3.5 h-3.5 text-green-500" />
              <span class="text-green-600 dark:text-green-400">Up to date</span>
            </template>
          </div>
        </div>

        <div class="mt-auto flex gap-2">
          <button
            v-if="!status.ffmpeg"
            @click="install('ffmpeg')"
            :disabled="loading.ffmpeg"
            class="flex-1 flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RotateCw v-if="loading.ffmpeg" class="w-4 h-4 animate-spin" />
            <Download v-else class="w-4 h-4" />
            Install
          </button>
          <button
            v-else
            @click="install('ffmpeg')"
            :disabled="loading.ffmpeg"
            class="flex-1 flex items-center justify-center gap-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-800 dark:text-gray-200 px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
            <RotateCw v-if="loading.ffmpeg" class="w-4 h-4 animate-spin" />
            <span v-else>Reinstall</span>
          </button>

          <button
            v-if="status.ffmpeg"
            @click="remove('ffmpeg')"
            :disabled="loading.ffmpeg"
            class="p-2 text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors disabled:opacity-50"
            title="Uninstall"
          >
            <Trash2 class="w-5 h-5" />
          </button>
        </div>
      </div>
    </div>

  </div>
</template>
