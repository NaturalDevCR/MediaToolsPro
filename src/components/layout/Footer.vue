<script setup lang="ts">
defineProps<{
  status?: string;
}>();
</script>

<template>
  <footer class="h-8 flex items-center px-4 text-xs
    bg-white/50 dark:bg-[#08111f]/80 backdrop-blur-xl
    border-t border-black/5 dark:border-white/10
    text-gray-500 dark:text-slate-400 transition-colors duration-300">
    <div class="flex-1 truncate">
      {{ status || 'Ready' }}
    </div>
    <div class="flex items-center gap-2">
       <span>v0.1.0</span>
    </div>
  </footer>
</template>
