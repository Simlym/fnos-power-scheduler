<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import UiIcon from './components/UiIcon.vue'
import TimePicker from './components/TimePicker.vue'
import DatePicker from './components/DatePicker.vue'
import { version } from '../package.json'
type Task = {id:string;name:string;action:'wake'|'shutdown';repeat:'once'|'daily'|'weekly';time:string;date:string;weekdays:number[];timezone:string;enabled:boolean}
type RecordItem = {taskId:string;name:string;at:string;status:string;message:string}
type State = {tasks:Task[];records:RecordItem[];next:Record<string,string|null>;pending:null|{name:string;deadline:string};power:{mode:string;available:boolean;alarm:number;message:string};protectionUntil:string;now:string;error:string}
const state=ref<State|null>(null), error=ref(''), refreshError=ref(''), busy=ref(false), editor=ref(false), loading=ref(true), currentTab=ref('overview')
const operation=ref({label:'',taskId:''})
let stateRevision=0, refreshSequence=0
const weekdays=[{n:1,label:'一'},{n:2,label:'二'},{n:3,label:'三'},{n:4,label:'四'},{n:5,label:'五'},{n:6,label:'六'},{n:0,label:'日'}]
const empty=():Task=>({id:'',name:'',action:'shutdown',repeat:'weekly',time:'23:00',date:'',weekdays:[1,2,3,4,5],timezone:Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai',enabled:true})
const draft=ref<Task>(empty())
watch(() => draft.value.repeat, repeat => {
 if (repeat === 'once' && !draft.value.date) {
  const parts = new Intl.DateTimeFormat('en-US', { timeZone: draft.value.timezone || 'Asia/Shanghai', year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(new Date())
  const part = (type: string) => parts.find(p => p.type === type)!.value
  draft.value.date = part('year') + '-' + part('month') + '-' + part('day')
 }
})
const apiBase=location.pathname.startsWith('/app/fnos-power-scheduler')?'/app/fnos-power-scheduler/':''
async function request(path:string,method='GET',body?:unknown){const response=await fetch(apiBase+'api/'+path,{method,signal:method==='GET'?AbortSignal.timeout(12000):undefined,headers:{'Content-Type':'application/json','X-Power-Scheduler':'1'},body:body===undefined?undefined:JSON.stringify(body)});if(!response.ok){let message=await response.text();try{message=JSON.parse(message).error||message}catch{}throw new Error(message)}return response.json()}
async function refresh(){
 const revision=stateRevision, sequence=++refreshSequence
 const current=()=>!stopped&&revision===stateRevision&&sequence===refreshSequence
 try{const result=await request('state');if(current()){state.value=result;refreshError.value=''}}
 catch(e){if(current())refreshError.value=String(e)}
 finally{if(current())loading.value=false}
}
async function mutate(path:string,method:string,body?:unknown,label='正在应用计划…',taskId=''){
 if(busy.value)return false
 busy.value=true;operation.value={label,taskId};error.value='';stateRevision++
 try{await request(path,method,body);return true}
 catch(e){error.value=String(e);return false}
 finally{stateRevision++;await refresh();busy.value=false;operation.value={label:'',taskId:''}}
}
function edit(task?:Task){error.value='';draft.value=task?{...task,weekdays:[...task.weekdays]}:empty();editor.value=true}
async function save(){if(await mutate('tasks','POST',draft.value,'正在保存并应用计划…',draft.value.id))editor.value=false}
async function toggle(task:Task){await mutate('tasks','POST',{...task,enabled:!task.enabled},task.enabled?'正在停用计划…':'正在启用计划…',task.id)}
async function remove(task:Task){if(confirm(`删除“${task.name}”的计划？`))await mutate('tasks/'+encodeURIComponent(task.id),'DELETE',undefined,'正在删除并应用计划…',task.id)}
const nextOf=(action:string)=>{if(!state.value)return null;const dates=state.value.tasks.filter(t=>t.enabled&&t.action===action).map(t=>state.value!.next[t.id]).filter((x):x is string=>!!x).sort();return dates[0]||null}
const nextWake=computed(()=>nextOf('wake')),nextShutdown=computed(()=>nextOf('shutdown'))
const enabledTasks=computed(()=>state.value?.tasks.filter(t=>t.enabled)||[])
const localTimezone=Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai'
const timelineEvents=(tasks:Task[])=>[...tasks].sort((a,b)=>a.time.localeCompare(b.time)||a.action.localeCompare(b.action)||a.name.localeCompare(b.name)||a.id.localeCompare(b.id))
type PlanWarning={time:string;kind:string;message:string}
type TimelinePoint={task:Task;minute:number;day:number}
const timeMinutes=(time:string)=>Number(time.slice(0,2))*60+Number(time.slice(3,5))
function planWarnings(points:TimelinePoint[],day:number,period=0):PlanWarning[]{
 const warnings:PlanWarning[]=[]
 points.forEach((point,i)=>{
  if(point.day!==day)return
  const previous=points[i-1]||(period&&points.length>1?points[points.length-1]:undefined)
  if(!previous)return
  const gap=point.minute-previous.minute+(i===0?period:0)
  const action=point.task.action==='wake'?'开机':'关机'
  const previousTime=previous.day===day?previous.task.time:`${previous.task.repeat==='once'?previous.task.date:'周'+weekdays[previous.day].label} ${previous.task.time}`
  if(gap===0){
   warnings.push({time:point.task.time,kind:previous.task.action===point.task.action?'重复计划':'同时间开关机',message:previous.task.action===point.task.action?`${point.task.time} 有重复的${action}计划，请检查是否重复添加。`:`${point.task.time} 同时安排了开机和关机，请调整执行时间。`})
  }else if(previous.task.action===point.task.action){
   const before=`${previousTime} 和 ${point.task.time}`
   warnings.push({time:point.task.time,kind:`连续${action}`,message:`${before} 连续安排${action}，中间没有${action==='开机'?'关机':'开机'}计划，请确认是否需要。`})
  }else if(previous.task.action==='shutdown'&&point.task.action==='wake'&&gap<=5){
   warnings.push({time:point.task.time,kind:'开关机间隔过短',message:`${previousTime} 关机到 ${point.task.time} 开机仅间隔 ${gap} 分钟，建议留出更充裕的关机时间。`})
  }
 })
 return warnings.filter((warning,index)=>warnings.findIndex(w=>w.message===warning.message)===index)
}
const weeklyOverview=computed(()=>{
 const zones=Array.from(new Set(enabledTasks.value.filter(t=>t.repeat!=='once').map(t=>t.timezone))).sort()
 if(!zones.length)zones.push(localTimezone)
 return zones.flatMap(timezone=>{
  const days=weekdays.map(day=>timelineEvents(enabledTasks.value.filter(t=>t.timezone===timezone&&(t.repeat==='daily'||(t.repeat==='weekly'&&t.weekdays.includes(day.n))))))
  const points=days.flatMap((events,day)=>events.map(task=>({task,day,minute:day*1440+timeMinutes(task.time)})))
  return days.map((events,index)=>({label:'周'+weekdays[index].label,timezone,events,warnings:planWarnings(points,index,7*1440)}))
 })
})
const onceOverview=computed(()=>{
 const tasks=enabledTasks.value.filter(t=>t.repeat==='once')
 const zones=Array.from(new Set(tasks.map(t=>t.timezone))).sort()
 return zones.flatMap(timezone=>{
  const dates=Array.from(new Set(tasks.filter(t=>t.timezone===timezone).map(t=>t.date))).sort()
  const days=dates.map(date=>timelineEvents(tasks.filter(t=>t.timezone===timezone&&t.date===date)))
  const points=days.flatMap((events,day)=>events.map(task=>({task,day,minute:Date.parse(task.date+'T00:00:00Z')/60000+timeMinutes(task.time)})))
  return days.map((events,day)=>({label:dates[day],timezone,events,warnings:planWarnings(points,day)}))
 }).sort((a,b)=>a.label.localeCompare(b.label)||a.timezone.localeCompare(b.timezone))
})
const warningCount=computed(()=>[...weeklyOverview.value,...onceOverview.value].reduce((count,day)=>count+day.warnings.length,0))
const format=(at:string|null)=>at?new Date(at).toLocaleString('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}):'尚未安排'
const formatUpcoming=(at:string|null)=>{
 if(!at)return '尚未安排'
 const date=new Date(at)
 const day=date.toLocaleDateString('zh-CN',{month:'2-digit',day:'2-digit'})
 const weekday='周'+['日','一','二','三','四','五','六'][date.getDay()]
 const time=date.toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit',hour12:false})
 return `${day} ${weekday} ${time}`
}
const repeatText=(t:Task)=>t.repeat==='once'?t.date:t.repeat==='daily'?'每天':'每周 '+weekdays.filter(d=>t.weekdays.includes(d.n)).map(d=>d.label).join('、')
const pendingSeconds=computed(()=>state.value?.pending?Math.max(0,Math.ceil((Date.parse(state.value.pending.deadline)-Date.parse(state.value.now))/1000)):0)
const labels:Record<string,string>={countdown:'倒计时',requested:'已请求',cancelled:'已取消',skipped:'已跳过',failed:'失败'}
let timer:ReturnType<typeof setTimeout>|undefined,stopped=false
async function poll(){await refresh();if(!stopped)timer=setTimeout(poll,2000)}
onMounted(poll);onUnmounted(()=>{stopped=true;if(timer)clearTimeout(timer)})
</script>

<template>
 <main>
  <header><div class="brand"><div><h1>电源计划</h1></div></div><div class="header-actions"><span v-if="loading||!state||state.power.mode==='simulation'" class="badge">{{loading?'连接中':!state?'未连接':'模拟模式'}}</span><button class="primary" @click="edit()" :disabled="loading||busy"><UiIcon name="plus" />新建计划</button></div></header>
  <div v-if="error" class="notice danger" role="alert">{{error}}</div>
  <div v-if="refreshError" class="notice danger" role="alert">连接中断，显示的数据可能已过期：{{refreshError}}</div>
  <div v-if="state?.error&&!busy" class="notice danger">计划尚未完全应用：{{state.error}}。请检查设备后重新保存计划。</div>
  <div v-if="state?.power.mode==='simulation'" class="notice">当前为模拟模式。任务和倒计时正常运行，不会实际关机或修改 RTC。</div>
  <div v-if="state?.pending" class="notice countdown" role="alert"><div><strong>{{state.pending.name}} · {{pendingSeconds}} 秒后请求关机</strong><p>取消只影响本次执行，后续计划继续生效。</p></div><button @click="mutate('cancel','POST',{},'正在取消本次关机…')" :disabled="busy">{{busy&&operation.label==='正在取消本次关机…'?'取消中…':'取消本次关机'}}</button></div>
  <section class="overview" aria-label="计划概览"><article><span class="icon"><UiIcon name="sun" /></span><div><p>下一次开机</p><h3>{{loading?'—':formatUpcoming(nextWake)}}</h3></div></article><article><span class="icon moon"><UiIcon name="moon" /></span><div><p>下一次关机</p><h3>{{loading?'—':formatUpcoming(nextShutdown)}}</h3></div></article><article><span class="icon count"><UiIcon name="list" /></span><div><p :title="state?`共 ${state.tasks.length} 项，已停用 ${state.tasks.length-enabledTasks.length} 项`:'正在读取计划'">已启用计划</p><h3>{{state?enabledTasks.length:'—'}} <span class="unit">项</span></h3></div></article></section>
  <section class="workspace"><div class="tabs"><button :class="{active:currentTab==='overview'}" @click="currentTab='overview'">计划总览</button><button :class="{active:currentTab==='tasks'}" @click="currentTab='tasks'">计划列表 <span>{{state?.tasks.length||0}}</span></button><button :class="{active:currentTab==='records'}" @click="currentTab='records'">执行记录</button></div>
   <div v-if="loading" class="empty">正在连接电源计划…</div>
   <template v-else-if="currentTab==='overview'">
    <div class="schedule-overview">
     <div class="schedule-heading"><h2>每周安排</h2><p>仅显示已启用计划</p><span v-if="warningCount" class="warning-caption">有计划提醒</span></div>
     <div v-for="section in [{title:'每周安排',rows:weeklyOverview},{title:'单次安排',rows:onceOverview}]" :key="section.title" class="timeline-section">
      <template v-if="section.rows.length">
       <div v-if="section.title==='单次安排'" class="schedule-heading once-heading"><h2>单次安排</h2><p>仅在指定日期执行一次。</p></div>
       <div class="timeline-table"><div class="timeline-table-heading"><span>{{section.title==='每周安排'?'星期':'日期'}}</span><span>执行顺序</span></div>
        <div v-for="(day,index) in section.rows" :key="index" class="timeline-row">
         <div class="timeline-day"><strong>{{day.label}}</strong><small v-if="day.timezone!==localTimezone">{{day.timezone}}</small></div>
         <div class="timeline-content"><ol v-if="day.events.length" class="timeline-events" :aria-label="day.label+'执行顺序'">
          <li v-for="(event,i) in day.events" :key="event.id||i"><span v-if="i" class="timeline-arrow" aria-hidden="true">→</span><span class="timeline-event" :class="[event.action,{warned:day.warnings.some(w=>w.time===event.time)}]" :title="event.timezone"><time>{{event.time}}</time><span class="timeline-action">{{event.action==='wake'?'开机':'关机'}}</span></span></li>
         </ol>
         <span v-else class="no-schedule">无计划</span>
         <details v-if="day.warnings.length" class="plan-warning-details"><summary :title="Array.from(new Set(day.warnings.map(w=>w.kind))).join('、')">{{day.warnings.length}} 项提醒</summary><ul class="plan-warnings" aria-label="计划提醒"><li v-for="warning in day.warnings" :key="warning.message"><span aria-hidden="true">!</span>{{warning.message}}</li></ul></details></div>
        </div>
       </div>
      </template>
     </div>
     <div v-if="!enabledTasks.length" class="overview-empty"><p>{{state?.tasks.length?'所有计划均已停用，可在计划列表中启用。':'还没有电源计划，添加开机或关机时间即可开始。'}}</p><button v-if="!state?.tasks.length" @click="edit()" :disabled="busy"><UiIcon name="plus" />创建计划</button><button v-else @click="currentTab='tasks'">查看计划列表</button></div>
    </div>
   </template>
   <template v-else-if="currentTab==='tasks'">
    <div v-if="!state?.tasks.length" class="empty"><div class="empty-icon"><UiIcon name="clock" /></div><h3>还没有电源计划</h3><p>添加开机或关机时间，支持按工作日重复。</p><button @click="edit()" :disabled="busy"><UiIcon name="plus" />创建计划</button></div>
    <div v-for="task in state?.tasks" :key="task.id" class="task-row" :class="{disabled:!task.enabled}"><div class="task-symbol" :class="task.action"><UiIcon :name="task.action==='wake'?'sun':'power'" /></div><div class="task-name"><strong>{{task.name}}</strong><p>{{task.action==='wake'?'开机':'关机'}} · {{repeatText(task)}} · {{task.time}} <span class="timezone">{{task.timezone}}</span></p></div><div class="next-time" :aria-busy="busy&&operation.taskId===task.id"><span class="task-operation" role="status" aria-live="polite" :title="busy&&operation.taskId===task.id?operation.label:undefined"><span v-if="busy&&operation.taskId===task.id" class="progress-spinner" aria-hidden="true"></span>{{busy&&operation.taskId===task.id?operation.label:'下次执行'}}</span><strong>{{task.enabled?format(state?.next[task.id]||null):'已停用'}}</strong></div><button class="switch" role="switch" :aria-checked="task.enabled" :aria-label="(task.enabled?'停用 ':'启用 ')+task.name" :class="{on:task.enabled}" :disabled="busy" @click="toggle(task)"><span></span></button><button class="quiet" :disabled="busy" @click="edit(task)">编辑</button><button class="quiet delete" :disabled="busy" @click="remove(task)">删除</button></div>
   </template>
   <template v-else><div v-if="!state?.records.length" class="empty"><h3>还没有执行记录</h3><p>计划到期后，会在这里显示倒计时、取消或请求结果。</p></div><div v-for="(record,i) in state?.records" :key="i" class="record-row"><span class="record-status" :class="record.status">{{labels[record.status]||record.status}}</span><div><strong>{{record.name}}</strong><p>{{record.message}}</p></div><time>{{format(record.at)}}</time></div></template>
  </section>
  <footer><span class="status-dot" :class="{bad:!state?.power.available}"></span><span>{{state?.power.message||'正在读取设备状态'}}</span><details class="usage-help"><summary>使用说明</summary><div><p>关闭页面或停止应用后，已应用的计划仍会执行。需要暂停时，请先禁用计划；停止应用会取消当前关机倒计时。</p><p>关机前有 60 秒倒计时，时间按计划设置的时区执行。</p><p>提醒仅依据已启用计划；手动开关机不在检查范围内。重复计划与单次计划分别检查，同一时区内比较。</p></div></details><span class="footer-time">v{{version}}</span></footer>
  <div v-if="editor" class="overlay" @click.self="!busy&&(editor=false)"><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="editor-title"><div class="dialog-heading"><h2 id="editor-title">{{draft.id?'编辑计划':'新建计划'}}</h2><button class="quiet" :disabled="busy" @click="editor=false" aria-label="关闭"><UiIcon name="close" /></button></div><form @submit.prevent="save"><label>计划名称<input v-model="draft.name" required maxlength="40" placeholder="例如：工作日晚间关机"></label><div class="form-grid"><label>电源操作<select v-model="draft.action"><option value="shutdown">关机</option><option value="wake">开机</option></select></label><div class="time-field"><span>执行时间</span><TimePicker v-model="draft.time" /></div></div><label>重复方式<select v-model="draft.repeat"><option value="weekly">每周指定日期</option><option value="daily">每天</option><option value="once">仅执行一次</option></select></label><fieldset v-if="draft.repeat==='weekly'"><legend>星期</legend><div class="days"><label v-for="day in weekdays" :key="day.n"><input type="checkbox" v-model="draft.weekdays" :value="day.n"><span>{{day.label}}</span></label></div></fieldset><label v-if="draft.repeat==='once'">执行日期<DatePicker v-model="draft.date" /></label><label>时区<input v-model="draft.timezone" required placeholder="Asia/Shanghai"></label><p class="form-note">关机前会有 60 秒倒计时。错过的关机不会补执行；开机计划需要设备支持 RTC 唤醒。</p><div v-if="error" class="notice danger">{{error}}</div><div class="dialog-actions"><button type="button" :disabled="busy" @click="editor=false">取消</button><button class="primary" :disabled="busy" type="submit">{{busy?'保存中…':'保存计划'}}</button></div></form></section></div>
 </main>
</template>
