<script setup lang="ts">
import { ref } from "vue";

const input = ref("");
</script>

<template>
  <div class="mx-auto max-w-2xl space-y-6 p-6">
    <form class="flex gap-2" @submit.prevent>
      <input v-model="input" class="input" placeholder="@username or instagram.com/p/… link" spellcheck="false" />
      <button class="btn-primary shrink-0" type="submit" disabled>Fetch</button>
    </form>
    <div class="card flex items-center justify-center p-12 text-sm text-slate-500">
      Paste a profile or post link to start downloading.
    </div>
  </div>
</template>
