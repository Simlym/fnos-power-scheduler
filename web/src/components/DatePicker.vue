<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import UiIcon from './UiIcon.vue'

const model = defineModel<string>({ required: true })
const root = ref<HTMLElement>()
const input = ref<HTMLInputElement>()
const trigger = ref<HTMLButtonElement>()
const open = ref(false)
const month = ref(new Date())
const format = (date: Date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
function parse(value: string) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null
  const date = new Date(`${value}T00:00:00`)
  return !Number.isNaN(date.getTime()) && format(date) === value ? date : null
}
function validate() {
  input.value?.setCustomValidity(model.value && !parse(model.value) ? '请输入有效日期，例如 2026-10-11' : '')
}
function toggle() {
  open.value = !open.value
  if (open.value) {
    const date = parse(model.value) || new Date()
    month.value = new Date(date.getFullYear(), date.getMonth(), 1)
  }
}
function move(offset: number) {
  month.value = new Date(month.value.getFullYear(), month.value.getMonth() + offset, 1)
}
const days = computed(() => {
  const year = month.value.getFullYear(), m = month.value.getMonth()
  const offset = (new Date(year, m, 1).getDay() + 6) % 7
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(year, m, index - offset + 1)
    return { value: format(date), day: date.getDate(), muted: date.getMonth() !== m }
  })
})
function close() { open.value = false; trigger.value?.focus() }
function select(value: string) { model.value = value; input.value?.setCustomValidity(''); close() }
function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) open.value = false
}
function focusOut(event: FocusEvent) {
  if (!root.value?.contains(event.relatedTarget as Node)) open.value = false
}
onMounted(() => { validate(); document.addEventListener('pointerdown', outside) })
onUnmounted(() => document.removeEventListener('pointerdown', outside))
</script>

<template>
  <div ref="root" class="date-picker" @focusout="focusOut" @keydown.esc.stop.prevent="close">
    <div class="picker-input">
      <input ref="input" v-model="model" aria-label="执行日期" placeholder="YYYY-MM-DD" required maxlength="10" pattern="[0-9]{4}-[0-9]{2}-[0-9]{2}" title="请输入有效日期，例如 2026-10-11" @input="validate" @focus="open = false">
      <button ref="trigger" type="button" class="picker-toggle" aria-label="选择执行日期" aria-haspopup="dialog" :aria-expanded="open" @click="toggle"><UiIcon name="calendar" /></button>
    </div>
    <div v-if="open" class="date-popover" role="dialog" aria-label="选择执行日期">
      <div class="calendar-heading"><button type="button" aria-label="上个月" @click="move(-1)">‹</button><strong>{{ month.getFullYear() }}年{{ month.getMonth() + 1 }}月</strong><button type="button" aria-label="下个月" @click="move(1)">›</button></div>
      <div class="calendar-grid"><span v-for="day in ['一','二','三','四','五','六','日']" :key="day">{{ day }}</span>
        <button v-for="day in days" :key="day.value" type="button" :class="{ muted: day.muted, selected: day.value === model }" :aria-label="day.value" :aria-pressed="day.value === model" :aria-current="day.value === format(new Date()) ? 'date' : undefined" @click="select(day.value)">{{ day.day }}</button>
      </div>
      <button type="button" class="time-confirm" @click="select(format(new Date()))">今天</button>
    </div>
  </div>
</template>
