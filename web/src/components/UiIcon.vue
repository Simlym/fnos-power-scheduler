<script setup lang="ts">
defineProps<{ name: string }>()
const paths: Record<string, string> = {
 power: 'M12 3v9 M6.3 5.7a9 9 0 1 0 11.4 0',
 sun: 'M12 2v2 M12 20v2 M2 12h2 M20 12h2 M4.9 4.9l1.4 1.4 M17.7 17.7l1.4 1.4 M4.9 19.1l1.4-1.4 M17.7 6.3l1.4-1.4 M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0',
 moon: 'M20.5 13.2A8.5 8.5 0 0 1 10.8 3.5a8.5 8.5 0 1 0 9.7 9.7Z',
 list: 'M9 6h12 M9 12h12 M9 18h12 M3 6h1 M3 12h1 M3 18h1',
 clock: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0 M12 7v5l3 2',
 plus: 'M12 5v14 M5 12h14',
 close: 'm6 6 12 12 M6 18 18 6',
}
</script>

<template><svg class="ui-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path :d="paths[name] || paths.clock" /></svg></template>
