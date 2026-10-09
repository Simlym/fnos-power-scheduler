<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
type Task = {id:string;name:string;action:'wake'|'shutdown';repeat:'once'|'daily'|'weekly';time:string;date:string;weekdays:number[];timezone:string;enabled:boolean}
type RecordItem = {taskId:string;name:string;at:string;status:string;message:string}
type State = {tasks:Task[];records:RecordItem[];next:Record<string,string|null>;pending:null|{name:string;deadline:string};power:{mode:string;available:boolean;alarm:number;message:string};protectionUntil:string;now:string;error:string}
const state=ref<State|null>(null), error=ref(''), refreshError=ref(''), busy=ref(false), editor=ref(false), loading=ref(true), currentTab=ref('tasks')
const weekdays=[{n:1,label:'一'},{n:2,label:'二'},{n:3,label:'三'},{n:4,label:'四'},{n:5,label:'五'},{n:6,label:'六'},{n:0,label:'日'}]
const empty=():Task=>({id:'',name:'',action:'shutdown',repeat:'weekly',time:'23:00',date:'',weekdays:[1,2,3,4,5],timezone:Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai',enabled:true})
const draft=ref<Task>(empty())
const apiBase=location.pathname.startsWith('/app/fnos-power-scheduler')?'/app/fnos-power-scheduler/':''
async function request(path:string,method='GET',body?:unknown){const response=await fetch(apiBase+'api/'+path,{method,headers:{'Content-Type':'application/json','X-Power-Scheduler':'1'},body:body===undefined?undefined:JSON.stringify(body)});if(!response.ok){let message=await response.text();try{message=JSON.parse(message).error||message}catch{}throw new Error(message)}return response.json()}
async function refresh(){try{state.value=await request('state');refreshError.value=''}catch(e){refreshError.value=String(e)}finally{loading.value=false}}
async function mutate(path:string,method:string,body?:unknown){busy.value=true;error.value='';try{await request(path,method,body);await refresh();return true}catch(e){error.value=String(e);return false}finally{busy.value=false}}
function edit(task?:Task){error.value='';draft.value=task?{...task,weekdays:[...task.weekdays]}:empty();editor.value=true}
async function save(){if(await mutate('tasks','POST',draft.value))editor.value=false}
async function toggle(task:Task){await mutate('tasks','POST',{...task,enabled:!task.enabled})}
async function remove(task:Task){if(confirm(`删除“${task.name}”的计划？`))await mutate('tasks/'+encodeURIComponent(task.id),'DELETE')}
const nextOf=(action:string)=>{if(!state.value)return null;const dates=state.value.tasks.filter(t=>t.enabled&&t.action===action).map(t=>state.value!.next[t.id]).filter((x):x is string=>!!x).sort();return dates[0]||null}
const nextWake=computed(()=>nextOf('wake')),nextShutdown=computed(()=>nextOf('shutdown'))
const format=(at:string|null)=>at?new Date(at).toLocaleString('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}):'尚未安排'
const repeatText=(t:Task)=>t.repeat==='once'?t.date:t.repeat==='daily'?'每天':'每周 '+weekdays.filter(d=>t.weekdays.includes(d.n)).map(d=>d.label).join('、')
const pendingSeconds=computed(()=>state.value?.pending?Math.max(0,Math.ceil((Date.parse(state.value.pending.deadline)-Date.parse(state.value.now))/1000)):0)
const labels:Record<string,string>={countdown:'倒计时',requested:'已请求',cancelled:'已取消',skipped:'已跳过',failed:'失败'}
let timer:ReturnType<typeof setTimeout>|undefined,stopped=false
async function poll(){await refresh();if(!stopped)timer=setTimeout(poll,2000)}
onMounted(poll);onUnmounted(()=>{stopped=true;if(timer)clearTimeout(timer)})
</script>

<template>
 <main>
  <header><div class="brand"><div class="logo">⏻</div><div><div class="eyebrow">FNOS POWER SCHEDULER</div><h1>电源计划</h1></div></div><span class="badge" :class="{live:state?.power.mode==='hardware'}">{{state?.power.mode==='hardware'?'设备模式':'模拟模式'}}</span></header>
  <section class="intro"><div><h2>让设备按你的节奏运行。</h2><p>安排开机与关机，把每一天交给计划。</p></div><button class="primary" @click="edit()" :disabled="loading||busy">＋ 新建计划</button></section>
  <div v-if="error" class="notice danger" role="alert">{{error}}</div>
  <div v-if="refreshError" class="notice danger" role="alert">连接中断，显示的数据可能已过期：{{refreshError}}</div>
  <div v-if="state?.error" class="notice danger">RTC 设置未完成：{{state.error}}。请检查设备后再安排关机。</div>
  <div v-if="state?.power.mode==='simulation'" class="notice">当前为模拟模式。任务和倒计时正常运行，不会实际关机或修改 RTC。</div>
  <div v-if="state?.pending" class="notice countdown" role="alert"><div><strong>{{state.pending.name}} · {{pendingSeconds}} 秒后请求关机</strong><p>取消只影响本次执行，后续计划继续生效。</p></div><button @click="mutate('cancel','POST',{})" :disabled="busy">取消本次关机</button></div>
  <section class="overview"><article><span class="icon">☀</span><p>下一次开机</p><h3>{{format(nextWake)}}</h3><small>由设备 RTC 闹钟唤醒</small></article><article><span class="icon moon">◔</span><p>下一次关机</p><h3>{{format(nextShutdown)}}</h3><small>执行前保留 60 秒倒计时</small></article><article><span class="icon count">≡</span><p>已启用计划</p><h3>{{state?.tasks.filter(t=>t.enabled).length ?? '—'}} <span class="unit">项</span></h3><small>单次、每天或按星期重复</small></article></section>
  <section class="workspace"><div class="tabs"><button :class="{active:currentTab==='tasks'}" @click="currentTab='tasks'">计划列表 <span>{{state?.tasks.length||0}}</span></button><button :class="{active:currentTab==='records'}" @click="currentTab='records'">执行记录</button></div>
   <div v-if="loading" class="empty">正在连接电源计划…</div>
   <template v-else-if="currentTab==='tasks'">
    <div v-if="!state?.tasks.length" class="empty"><div class="empty-icon">◷</div><h3>从你的第一条计划开始</h3><p>例如：工作日晚上 23:00 关机，次日早上 08:00 开机。</p><button @click="edit()">创建计划</button></div>
    <div v-for="task in state?.tasks" :key="task.id" class="task-row"><div class="task-symbol" :class="task.action">{{task.action==='wake'?'☀':'⏻'}}</div><div class="task-name"><strong>{{task.name}}</strong><p>{{task.action==='wake'?'开机':'关机'}} · {{repeatText(task)}} · {{task.time}} <span class="timezone">{{task.timezone}}</span></p></div><div class="next-time"><span>下次执行</span><strong>{{format(state?.next[task.id]||null)}}</strong></div><button class="switch" role="switch" :aria-checked="task.enabled" :aria-label="'启用 '+task.name" :class="{on:task.enabled}" :disabled="busy" @click="toggle(task)"><span></span></button><button class="quiet" :disabled="busy" @click="edit(task)">编辑</button><button class="quiet delete" :disabled="busy" @click="remove(task)">删除</button></div>
   </template>
   <template v-else><div v-if="!state?.records.length" class="empty"><h3>还没有执行记录</h3><p>计划到期后，会在这里显示倒计时、取消或请求结果。</p></div><div v-for="(record,i) in state?.records" :key="i" class="record-row"><span class="record-status" :class="record.status">{{labels[record.status]||record.status}}</span><div><strong>{{record.name}}</strong><p>{{record.message}}</p></div><time>{{format(record.at)}}</time></div></template>
  </section>
  <footer><span class="status-dot" :class="{bad:!state?.power.available}"></span><span>{{state?.power.message||'正在读取设备状态'}}</span><span class="footer-time">时间按任务指定的时区执行 · v0.1.0</span></footer>
  <div v-if="editor" class="overlay" @click.self="editor=false"><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="editor-title"><div class="dialog-heading"><h2 id="editor-title">{{draft.id?'编辑计划':'新建计划'}}</h2><button class="quiet" @click="editor=false" aria-label="关闭">✕</button></div><form @submit.prevent="save"><label>计划名称<input v-model="draft.name" required maxlength="40" placeholder="例如：工作日晚间关机"></label><div class="form-grid"><label>电源操作<select v-model="draft.action"><option value="shutdown">关机</option><option value="wake">开机</option></select></label><label>执行时间<input type="time" v-model="draft.time" required></label></div><label>重复方式<select v-model="draft.repeat"><option value="weekly">每周指定日期</option><option value="daily">每天</option><option value="once">仅执行一次</option></select></label><fieldset v-if="draft.repeat==='weekly'"><legend>星期</legend><div class="days"><label v-for="day in weekdays" :key="day.n"><input type="checkbox" v-model="draft.weekdays" :value="day.n"><span>{{day.label}}</span></label></div></fieldset><label v-if="draft.repeat==='once'">执行日期<input type="date" v-model="draft.date" required></label><label>时区<input v-model="draft.timezone" required placeholder="Asia/Shanghai"></label><p class="form-note">关机前会有 60 秒倒计时。错过的关机不会补执行；开机计划需要设备支持 RTC 唤醒。</p><div v-if="error" class="notice danger">{{error}}</div><div class="dialog-actions"><button type="button" @click="editor=false">取消</button><button class="primary" :disabled="busy" type="submit">{{busy?'保存中…':'保存计划'}}</button></div></form></section></div>
 </main>
</template>
