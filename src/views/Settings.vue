<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle, XCircle, Download, Trash2, RotateCw } from 'lucide-vue-next';

interface BinaryStatus {
  ytdlp: boolean;
  ytdlp_version: string | null;
  ffmpeg: boolean;
  ffmpeg_version: string | null;
}

const status = ref<BinaryStatus>({
  ytdlp: false,
  ytdlp_version: null,
  ffmpeg: false,
  ffmpeg_version: null,
});

const loading = ref<{ [key: string]: boolean }>({});
const checking = ref(true);
const initialized = ref(false);

async function checkStatus() {
  if (!initialized.value) checking.value = true;
  
  try {
    // 1. Fast check
    const fastStatus = await invoke<BinaryStatus>('check_binaries');
    status.value = {
        ...status.value,
        ytdlp: fastStatus.ytdlp,
        ffmpeg: fastStatus.ffmpeg
    };
    initialized.value = true;
    
    // checking.value remains true here to indicate background work (spinner)
    
    // 2. Slow check
    try {
        const versions = await invoke<{ ytdlp_version: string | null, ffmpeg_version: string | null }>('get_binary_versions');
        status.value.ytdlp_version = versions.ytdlp_version;
        status.value.ffmpeg_version = versions.ffmpeg_version;
    } catch (ve) {
        console.error('Failed to get versions', ve);
    }
  } catch (e) {
    console.error('Failed to check binaries', e);
  } finally {
     checking.value = false;
  }
}

async function install(binary: 'ytdlp' | 'ffmpeg') {
  loading.value[binary] = true;
  try {
    if (binary === 'ytdlp') await invoke('install_ytdlp');
    if (binary === 'ffmpeg') await invoke('install_ffmpeg');
    await checkStatus(); // refresh status
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
    <h1 class="text-3xl font-light text-gray-800 dark:text-gray-100 flex items-center gap-3">
      Settings & Binaries
      <button @click="checkStatus" class="p-2 rounded-full hover:bg-gray-200 dark:hover:bg-gray-800 transition-colors" title="Refresh Status">
        <RotateCw class="w-5 h-5 text-gray-500" :class="{ 'animate-spin': checking }" />
      </button>
    </h1>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      
      <!-- YT-DLP Card -->
      <div class="bg-white/40 dark:bg-white/5 backdrop-blur-xl border border-white/20 dark:border-white/10 p-6 rounded-2xl shadow-sm flex flex-col gap-4 transition-all hover:shadow-md">
        <div class="flex items-center justify-between">
          <h2 class="text-xl font-medium text-gray-700 dark:text-gray-200">yt-dlp</h2>
          <div class="flex items-center gap-2">
            <!-- Show checking ONLY if not initialized yet (first load blank state prevention) -->
            <template v-if="checking && !initialized">
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
        
        <div class="text-sm text-gray-500 dark:text-gray-400 min-h-[1.5em] flex items-center gap-2">
          <span>Version:</span>
          <span v-if="status.ytdlp_version">{{ status.ytdlp_version }}</span>
          <span v-else-if="checking && status.ytdlp" class="flex items-center gap-2 text-blue-500">
             <RotateCw class="w-3 h-3 animate-spin" /> Verifying...
          </span>
          <span v-else>N/A</span>
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
             v-else
             @click="install('ytdlp')" 
             :disabled="loading.ytdlp"
             class="flex-1 flex items-center justify-center gap-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-800 dark:text-gray-200 px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
             <RotateCw v-if="loading.ytdlp" class="w-4 h-4 animate-spin" />
             <span v-else>Update</span>
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
             <template v-if="checking && !initialized">
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
        
        <div class="text-sm text-gray-500 dark:text-gray-400 min-h-[1.5em] flex items-center gap-2">
           <span>Version:</span>
           <span v-if="status.ffmpeg_version">{{ status.ffmpeg_version }}</span>
           <span v-else-if="checking && status.ffmpeg" class="flex items-center gap-2 text-blue-500">
             <RotateCw class="w-3 h-3 animate-spin" /> Verifying...
           </span>
           <span v-else>N/A</span>
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
             <span v-else>Update</span>
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
