<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import UiIcon from './UiIcon.vue'

const model = defineModel<string>({ required: true })
const open = ref(false)
const root = ref<HTMLElement>()
const trigger = ref<HTMLButtonElement>()
const hours = Array.from({ length: 24 }, (_, n) => String(n).padStart(2, '0'))
const minutes = Array.from({ length: 60 }, (_, n) => String(n).padStart(2, '0'))
const parts = computed(() => model.value.split(':'))

async function toggle() {
  open.value = !open.value
  if (open.value) {
    await nextTick()
    root.value?.querySelectorAll<HTMLElement>('[aria-selected="true"]').forEach(option => {
      const list = option.parentElement!
      list.scrollTop = option.offsetTop - (list.clientHeight - option.offsetHeight) / 2
    })
    root.value?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus({ preventScroll: true })
  }
}

function select(column: number, value: string) {
  model.value = column === 0 ? `${value}:${parts.value[1]}` : `${parts.value[0]}:${value}`
}

function navigate(event: KeyboardEvent, column: number, index: number) {
  const values = column === 0 ? hours : minutes
  let target = index
  if (event.key === 'ArrowDown') target = (index + 1) % values.length
  else if (event.key === 'ArrowUp') target = (index + values.length - 1) % values.length
  else if (event.key === 'Home') target = 0
  else if (event.key === 'End') target = values.length - 1
  else return
  event.preventDefault()
  select(column, values[target]!)
  const list = (event.currentTarget as HTMLElement).parentElement!
  ;(list.children[target] as HTMLElement).focus()
}

function close() {
  open.value = false
  trigger.value?.focus()
}

function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) open.value = false
}

function focusOut(event: FocusEvent) {
  if (!root.value?.contains(event.relatedTarget as Node)) open.value = false
}

onMounted(() => document.addEventListener('pointerdown', outside))
onUnmounted(() => document.removeEventListener('pointerdown', outside))
</script>

<template>
  <div ref="root" class="time-picker" @focusout="focusOut" @keydown.esc.stop.prevent="close">
    <button ref="trigger" type="button" class="time-trigger" aria-label="执行时间" aria-haspopup="dialog" :aria-expanded="open" @click="toggle">
      <span>{{ model }}</span><UiIcon name="clock" />
    </button>
    <div v-if="open" class="time-popover" role="dialog" aria-label="选择执行时间">
      <div class="time-columns">
        <div v-for="(values, column) in [hours, minutes]" :key="column" class="time-column">
          <div class="time-column-title">{{ column === 0 ? '时' : '分' }}</div>
          <div class="time-options" role="listbox" :aria-label="column === 0 ? '小时' : '分钟'">
            <button v-for="(value, index) in values" :key="value" type="button" role="option" :aria-selected="parts[column] === value" :tabindex="parts[column] === value ? 0 : -1" @click="select(column, value)" @keydown="navigate($event, column, index)">{{ value }}</button>
          </div>
        </div>
      </div>
      <button type="button" class="time-confirm" @click="close">确定</button>
    </div>
  </div>
</template>
