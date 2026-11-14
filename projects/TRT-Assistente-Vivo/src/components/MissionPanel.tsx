import { useState } from 'react'
import { ArrowRight, Bot, ClipboardCopy, Layers, ShieldCheck, Sparkles, Workflow, XCircle } from 'lucide-react'
import { backend, errorMessage, type Provider } from '../lib/api'
import { agents } from '../lib/agents'

type Stage={name:string;description:string;result:string;status:'pending'|'running'|'complete'|'failed'}
const createStages=():Stage[]=>[
{name:'1. Planejamento',description:'Dividir a tarefa em etapas claras e verificar limites.',result:'',status:'pending'},
{name:'2. Especialista',description:'Desenvolver a solução com base no plano.',result:'',status:'pending'},
{name:'3. Revisão',description:'Revisar, identificar riscos e preparar a resposta final.',result:'',status:'pending'},
]
export default function MissionPanel({providers,openChat}:{providers:Provider[];openChat:(id:string)=>void}){
 const [task,setTask]=useState('')
 const [expert,setExpert]=useState('code')
 const [provider,setProvider]=useState('')
 const [freeOnly,setFreeOnly]=useState(true)
 const [stages,setStages]=useState<Stage[]>(createStages)
 const [session,setSession]=useState('')
 const [running,setRunning]=useState(false)
 const [error,setError]=useState('')
 const [ready,setReady]=useState(false)
 const available=providers.filter(p=>p.enabled&&(!freeOnly||p.costStatus==='free'))
 const specialist=agents.find(a=>a.id===expert)||agents[0]
 const update=(index:number,patch:Partial<Stage>)=>setStages(prev=>prev.map((s,i)=>i===index?{...s,...patch}:s))
 async function start(){
  const description=task.trim()
  if(running||description.length<8){setError('Descreva o objetivo com pelo menos oito caracteres.');return}
  if(!available.length){setError('Ative ao menos um provedor elegível na Central de Provedores.');return}
  if(!window.confirm('Executar missão com até TRÊS chamadas de IA ao provedor selecionado? Isso consome cota e pode estar sujeito às regras do serviço. Nenhuma ferramenta do Windows ou MCP será executada automaticamente.'))return
  setRunning(true);setReady(false);setError('');setStages(createStages())
  const missionId=crypto.randomUUID();setSession(missionId)
  let plan='',solution=''
  const steps=[
   {role:'Você é o planejador de missões. Crie um plano técnico ou pedagógico objetivo, sem alegar ações executadas.',prompt:'Objetivo:\n'+description+'\n\nProduza um plano de ação verificável.'},
   {role:specialist.prompt,prompt:'Objetivo original:\n'+description+'\n\nPlano proposto:\n'+plan},
   {role:'Você é um revisor técnico independente. Verifique coerência, limitações, riscos e ausência de evidências. Apresente uma resposta final consolidada em português.',prompt:''}
  ]
  try{
   for(let i=0;i<3;i++){
    update(i,{status:'running'})
    if(i===1)steps[i].prompt='Objetivo:\n'+description+'\n\nPlano validado:\n'+plan+'\n\nDesenvolva a solução completa, com detalhes aplicáveis.'
    if(i===2)steps[i].prompt='Objetivo:\n'+description+'\n\nPlano:\n'+plan+'\n\nSolução do especialista:\n'+solution+'\n\nRevise e entregue a versão final, sem inventar execução nem testes.'
    const response=await backend.chat({
      sessionId:missionId,prompt:steps[i].prompt.slice(0,60000),
      history:[],agentPrompt:steps[i].role,
      providerId:provider||undefined,autoFallback:true,freeOnly
    })
    update(i,{status:'complete',result:response.content})
    if(i===0)plan=response.content.slice(0,16000)
    if(i===1)solution=response.content.slice(0,26000)
   }
   setReady(true)
  }catch(e){
   setError('A missão foi interrompida: '+errorMessage(e)+'. Etapas já concluídas permanecem no histórico.')
   setStages(prev=>prev.map(s=>s.status==='running'?{...s,status:'failed'}:s))
  }finally{setRunning(false)}
 }
 const textOutput=stages.map(s=>'## '+s.name+'\n'+s.result).filter(x=>x.length>30).join('\n\n')
 return <div className="standard-page">
  <div className="page-title"><div><div className="eyebrow">TRT MULTIAGENT FLOW</div><h1>Orquestrador de Missões</h1><p>Três perfis de inteligência trabalhando em sequência, com planejamento, especialização e revisão.</p></div><span className="tag active"><Workflow size={15}/> Supervisão humana</span></div>
  <div className="info-banner"><ShieldCheck size={19}/><p>Este fluxo realiza três chamadas sucessivas à API. Não executa automaticamente shell, navegador, MCP, publicação ou operações sobre arquivos. Planos e revisões são respostas de IA, não ações verificadas.</p></div>
  <div className="mission-layout"><section className="panel mission-form"><div className="panel-head"><div><h2><Sparkles size={18}/> Defina sua missão</h2><p>Descreva o resultado esperado e selecione o especialista.</p></div></div>
    <label>Objetivo</label><textarea rows={6} value={task} onChange={e=>setTask(e.target.value)} placeholder="Ex.: criar um roteiro de aula sobre SIG, com exercícios e gabarito" disabled={running}/>
    <label>Especialista</label><select value={expert} disabled={running} onChange={e=>setExpert(e.target.value)}>{agents.map(a=><option key={a.id} value={a.id}>{a.name}</option>)}</select>
    <label>Provedor principal</label><select value={provider} disabled={running} onChange={e=>setProvider(e.target.value)}><option value="">Automático (prioridade)</option>{available.map(p=><option value={p.id} key={p.id}>{p.name} · {p.model}</option>)}</select>
    <label className="browser-approval"><input type="checkbox" checked={freeOnly} disabled={running} onChange={e=>{setFreeOnly(e.target.checked);setProvider('')}}/> Restringir a modelos classificados como gratuitos</label>
    <button className="button primary" disabled={running||task.trim().length<8||!available.length} onClick={()=>void start()}><Bot size={17}/>{running?'Executando etapas...':'Confirmar e iniciar missão'}</button>
    {error&&<div className="browser-feedback" role="alert"><XCircle size={17}/><span>{error}</span></div>}
  </section>
  <section className="panel mission-timeline"><div className="panel-head"><div><h2><Workflow size={18}/> Etapas da missão</h2><p>Resultados apresentados conforme cada agente responde.</p></div></div>
    {stages.map((stage,i)=><div className={'mission-stage '+stage.status} key={stage.name}><div className="mission-stage-head"><span className="mission-step-num">{i+1}</span><div><strong>{stage.name}</strong><small>{stage.description}</small></div><span className="tag">{stage.status==='complete'?'Concluída':stage.status==='running'?'Executando':stage.status==='failed'?'Falhou':'Pendente'}</span></div>{stage.result&&<div className="mission-stage-summary">{stage.result.slice(0,1100)}{stage.result.length>1100?'…':''}</div>}</div>)}
    {ready&&<div className="mission-actions"><button className="button secondary" onClick={()=>navigator.clipboard.writeText(textOutput)}><ClipboardCopy size={16}/> Copiar resultados</button><button className="button primary" onClick={()=>openChat(session)}>Abrir histórico <ArrowRight size={16}/></button></div>}
  </section></div>
  <p className="muted mission-footer"><Layers size={14}/> Execução supervisionada, sem ações de ferramentas automáticas. Confirme informações importantes antes de utilizar os resultados.</p>
 </div>
}
