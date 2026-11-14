import { useEffect, useState } from 'react'
import { CheckCircle2, ClipboardCheck, ListChecks, ShieldCheck, XCircle } from 'lucide-react'
import { localTasks, type TaskPreview, type TaskReport } from '../lib/api'

type Progress={taskId:string;index:number;label:string;status:string;detail?:string}
export default function SupervisedTaskPanel({
  sessionId, prompt, onDone, onFormReview
}:{
  sessionId:string;prompt:string;onDone:(text:string)=>void;onFormReview:()=>void
}){
 const [plan,setPlan]=useState<TaskPreview|null>(null)
 const [progress,setProgress]=useState<Progress[]>([])
 const [busy,setBusy]=useState(false)
 const [error,setError]=useState('')
 const [report,setReport]=useState<TaskReport|null>(null)
 useEffect(()=>{
  let remove:undefined|(()=>void)
  void localTasks.progress(p=>{
   setProgress(old=>p.taskId===plan?.id?[...old.filter(x=>x.index!==p.index),p]:old)
  }).then(fn=>{remove=fn})
  return()=>remove?.()
 },[plan?.id])
 async function prepare(){
  if(!prompt.trim())return
  setBusy(true);setError('');setReport(null);setPlan(null);setProgress([])
  try{setPlan(await localTasks.preview(prompt.trim(),sessionId))}
  catch(e){setError(String(e))}
  finally{setBusy(false)}
 }
 async function cancel(){
  if(plan){try{await localTasks.cancel(plan.id)}catch(e){setError(String(e))}}
  setPlan(null);setProgress([])
 }
 async function approve(){
  if(!plan||busy)return
  if(!window.confirm('Executar '+plan.steps.length+' etapa(s) exatamente como descritas? A navegação e os aplicativos serão abertos de verdade. Você poderá ver os resultados e nenhuma BIOS, senha ou formulário será alterado.'))return
  setBusy(true);setError('')
  try{
   const result=await localTasks.execute(plan.id,sessionId)
   setReport(result);onDone(result.summary)
   if(result.containsFormReview)onFormReview()
   setPlan(null)
  }catch(e){setError(String(e));setPlan(null)}
  finally{setBusy(false)}
 }
 return <section className="panel" style={{padding:16,marginTop:12}}>
  <div className="panel-head"><div><h2><ListChecks size={18}/> Executor de tarefas supervisionadas</h2><p>Prévia, aprovação por tarefa e até 5 etapas. Sem shell arbitrário ou alterações de BIOS.</p></div></div>
  <button className="button secondary" disabled={busy||!prompt.trim()} onClick={()=>void prepare()}><ClipboardCheck size={16}/> Preparar plano a partir da mensagem</button>
  {error&&<p role="alert" className="browser-feedback"><XCircle size={16}/>{error}</p>}
  {plan&&<div style={{marginTop:16}}>
   <p><ShieldCheck size={15}/> {plan.notice}</p>
   <ol>{plan.steps.map(s=><li key={s.index}><strong>{s.label}</strong> <span className="muted">({s.kind})</span></li>)}</ol>
   <div className="row-buttons" style={{gap:10,marginTop:14}}>
    <button className="button primary" disabled={busy} onClick={()=>void approve()}><CheckCircle2 size={16}/>{busy?'Executando...':'Autorizar e executar este plano'}</button>
    <button className="button secondary" disabled={busy} onClick={()=>void cancel()}><XCircle size={16}/> Cancelar</button>
   </div>
  </div>}
  {progress.length>0&&<div style={{marginTop:12}} aria-live="polite">{progress.sort((a,b)=>a.index-b.index).map(p=><p key={p.index}>{p.index}. {p.label} — {p.status}{p.detail?' · '+p.detail:''}</p>)}</div>}
  {report&&<div style={{marginTop:12}}><strong>{report.success?'Tarefa concluída':'Tarefa interrompida'}: {report.completed} de {report.steps.length} etapas processadas</strong></div>}
 </section>
}
