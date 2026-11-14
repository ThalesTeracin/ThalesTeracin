import { useEffect, useRef, useState } from 'react'
import { Activity, AlertTriangle, CheckCircle2, ClipboardList, Mic, MicOff, Monitor, Send, ShieldCheck, Sparkles, Square, Volume2, VolumeX, Zap } from 'lucide-react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { backend, errorMessage, localTasks, type Provider, type TaskPreview, type TaskReport } from '../lib/api'
import './LivingAssistant.css'

type Mode = 'idle' | 'listening' | 'thinking' | 'speaking' | 'acting'
type Turn = { role: 'user' | 'assistant'; content: string }
type RecEvent = { resultIndex: number; results: ArrayLike<{ isFinal: boolean; 0: { transcript: string } }> }
type Recognizer = {
 lang: string; interimResults: boolean; continuous: boolean
 onresult: ((event: RecEvent) => void) | null
 onerror: ((event: { error: string }) => void) | null
 onend: (() => void) | null
 start: () => void; stop: () => void
}
type SpeechHost = Window & { SpeechRecognition?: new () => Recognizer; webkitSpeechRecognition?: new () => Recognizer }
type Props = {
 providers: Provider[]; preferredProvider: string; freeOnly: boolean; autoFallback: boolean
 onOpenAutomation: () => void; onOpenProviders: () => void
}
const assistantInstructions = [
 'Você é o TRT Assistente Vivo, assistente técnico pessoal em português brasileiro.',
 'Seja franco, crítico, cordial, preciso e sucinto. Questione configurações arriscadas e diga quando algo está errado, justificando.',
 'Priorize Windows 11 ARM64, soluções gratuitas, baixo consumo de memória e preservação de configurações existentes.',
 'Você pode recomendar e explicar tarefas. NÃO possui acesso direto ao Windows ou browser por meio do modelo.',
 'Jamais diga que executou, abriu, instalou, verificou ou alterou algo sem resultado explícito de ferramenta.',
 'As únicas ações de Windows são aquelas que o aplicativo oferece em prévia auditável e com autorização.',
 'Alterar BIOS, apagar arquivos, instalar software, alterar contas, submeter formulários ou executar shell arbitrário NÃO é permitido neste módulo.',
 'Ofereça alternativa segura quando a tarefa pedida não estiver disponível.'
].join(' ')
const actionIntent = /^\s*(?:por favor[,\s]+)?(?:abra|abre|abrir|inicie|iniciar|execute|executar|acesse|acessa|visite|visitar|navegue|navegar|entre|ir\s|vá\s|va\s|rode|rodar|diagnostique|diagn[oó]stico|verifique (?:o|a|meu|minha)|fa[çc]a diagn[oó]stico|inspecione|preencha|preencher|atualize cadastro|atualizar cadastro|configure|configurar|reconfigure|reconfigurar|instale|instalar|desinstale|apague|exclua|delete|reinicie|formate)\b/i
const hints = ['Diagnóstico do Windows', 'Abra calculadora', 'O que está consumindo minha RAM?', 'Como configurar um programa sem comprometer o Windows?']

export function WireFace({mode}:{mode:Mode}) {
 const curves = Array.from({length:15},(_,i)=>i)
 const latitudes = Array.from({length:20},(_,i)=>i)
 return <svg className={'living-face living-face-'+mode} viewBox="0 0 380 410" role="img" aria-label="Rosto digital com malha geométrica reativa à conversa">
  <defs>
   <radialGradient id="faceAura"><stop stopColor="#22e0c3" stopOpacity=".26"/><stop offset=".6" stopColor="#34adcf" stopOpacity=".06"/><stop offset="1" stopColor="#060d18" stopOpacity="0"/></radialGradient>
   <linearGradient id="faceStroke" x1="0" x2="1" y1="0" y2="1"><stop stopColor="#e4efff"/><stop offset=".55" stopColor="#9cdecf"/><stop offset="1" stopColor="#7692b8"/></linearGradient>
   <clipPath id="faceMask"><path d="M190 49 C111 45 84 115 92 223 C96 314 139 369 190 384 C246 370 290 301 292 213 C302 121 264 47 190 49Z"/></clipPath>
  </defs>
  <circle cx="190" cy="196" r="182" fill="url(#faceAura)"/>
  <g className="living-wire" fill="none" stroke="url(#faceStroke)" strokeWidth="1.12" opacity=".86">
   <path className="living-head" d="M190 49 C111 45 84 115 92 223 C96 314 139 369 190 384 C246 370 290 301 292 213 C302 121 264 47 190 49Z" strokeWidth="2"/>
   <g clipPath="url(#faceMask)" strokeWidth=".57" opacity=".63">
    {curves.map(i=><path key={'v'+i} d={'M '+(110+i*11.8)+' 47 C '+(76+i*15.3)+' 143 '+(79+i*15.2)+' 287 '+(132+i*8.3)+' 391'}/>)}
    {latitudes.map(i=><path key={'h'+i} d={'M 73 '+(66+i*15.5)+' Q 189 '+(43+i*16.9)+' 308 '+(66+i*15.5)}/>)}
    <path d="M190 35 V394"/>
   </g>
   <path d="M94 205 Q72 198 76 230 Q83 257 107 261 M287 204 Q308 197 306 231 Q297 257 279 260" strokeWidth="2"/>
   <path d="M114 208 Q143 180 165 205 M215 205 Q239 180 266 207" strokeWidth="1.9"/>
   <g className="living-eyes">
    <path d="M113 214 Q140 191 165 214 Q141 225 113 214Z M214 214 Q240 190 267 214 Q240 228 214 214Z" strokeWidth="1.6"/>
    <ellipse cx="141" cy="212" rx="8" ry="10"/><ellipse cx="241" cy="212" rx="8" ry="10"/>
    <circle cx="141" cy="212" r="3" fill="#b3fff0"/><circle cx="241" cy="212" r="3" fill="#b3fff0"/>
   </g>
   <path d="M189 214 Q181 264 173 277 Q189 289 205 276 Q197 272 191 270" strokeWidth="1.4"/>
   <path d="M162 302 Q190 295 217 302 Q194 310 164 303" opacity=".5"/>
   <g className="living-mouth"><path d="M153 321 Q188 310 227 321 Q190 334 153 321Z" strokeWidth="1.8"/><path d="M155 321 Q190 324 226 321" opacity=".8"/></g>
   <path d="M124 285 Q137 309 151 314 M229 315 Q252 308 262 282" opacity=".6"/>
  </g>
  <g className="living-spark" fill="#2efcce"><circle cx="190" cy="114" r="4"/><circle cx="272" cy="175" r="2.5"/><circle cx="112" cy="155" r="2"/></g>
 </svg>
}

function cleanSpeech(text:string) {
 return text.replace(/https?:\/\/\S+/g, 'endereço na internet')
  .replace(/[*_#~\x60>|]/g, '').replace(/\[[^\]]+\]\([^)]+\)/g,'')
  .replace(/\s+/g,' ').trim().slice(0,1600)
}

export default function LivingAssistant({providers,preferredProvider,freeOnly,autoFallback,onOpenAutomation,onOpenProviders}:Props){
 const [sessionId] = useState(()=>crypto.randomUUID())
 const [turns,setTurns] = useState<Turn[]>([{role:'assistant',content:'Olá! Sou seu Assistente Vivo. Posso conversar, avaliar decisões técnicas e preparar ações no Windows para sua aprovação. O microfone e o poder de execução começam desligados.'}])
 const [entry,setEntry] = useState('')
 const [mode,setMode] = useState<Mode>('idle')
 const [power,setPower] = useState(false)
 const [voice,setVoice] = useState(true)
 const [busy,setBusy] = useState(false)
 const [plan,setPlan] = useState<TaskPreview|null>(null)
 const [report,setReport] = useState<TaskReport|null>(null)
 const [error,setError] = useState('')
 const [providerId,setProviderId] = useState(preferredProvider)
 const [usedProvider,setUsedProvider] = useState('')
 const [interim,setInterim] = useState('')
 const [micOn,setMicOn] = useState(false)
 const [micSupported] = useState(()=>Boolean((window as SpeechHost).SpeechRecognition || (window as SpeechHost).webkitSpeechRecognition))
 const recognition=useRef<Recognizer|null>(null)
 const turnsRef=useRef<Turn[]>(turns)
 const endRef=useRef<HTMLDivElement|null>(null)
 const voiceRef=useRef(voice)
 const mounted=useRef(true)
 const busyRef=useRef(false)
 const allowed=providers.filter(p=>p.enabled && (!freeOnly || p.costStatus==='free'))
 const selected=allowed.find(p=>p.id===providerId)||allowed.find(p=>!p.id.startsWith('coarena-'))||allowed[0]

 useEffect(()=>{voiceRef.current=voice;if(!voice)window.speechSynthesis?.cancel()},[voice])
 useEffect(()=>{turnsRef.current=turns;endRef.current?.scrollIntoView({behavior:'smooth',block:'end'})},[turns,plan,error])
 useEffect(()=>()=>{mounted.current=false;recognition.current?.stop();window.speechSynthesis?.cancel()},[])
 function append(turn:Turn){setTurns(prev=>[...prev,turn].slice(-40));turnsRef.current=[...turnsRef.current,turn].slice(-40)}
 function speak(message:string){
  if(!voiceRef.current || !('speechSynthesis' in window)) {setMode('idle');return}
  window.speechSynthesis.cancel()
  const utter=new SpeechSynthesisUtterance(cleanSpeech(message))
  utter.lang='pt-BR';utter.rate=1.02;utter.pitch=1.0
  const voices=window.speechSynthesis.getVoices()
  utter.voice=voices.find(v=>v.lang.toLowerCase()==='pt-br')||voices.find(v=>v.lang.startsWith('pt'))||null
  utter.onstart=()=>{if(mounted.current)setMode('speaking')}
  utter.onend=()=>{if(mounted.current)setMode('idle')}
  utter.onerror=()=>{if(mounted.current)setMode('idle')}
  window.speechSynthesis.speak(utter)
 }
 function answer(message:string){append({role:'assistant',content:message});speak(message)}
 async function send(text?:string){
  const prompt=(text??entry).trim()
  if(!prompt||busyRef.current)return
  window.speechSynthesis?.cancel()
  busyRef.current=true;setBusy(true);setEntry('');setError('');setReport(null)
  append({role:'user',content:prompt});setMode('thinking')
  try{
   if(actionIntent.test(prompt)){
    if(!power){
     answer('O modo **Poder de execução** está desligado. Ative-o somente quando quiser preparar uma tarefa. A execução exige revisão e uma segunda autorização.')
     return
    }
    try{
     const preview=await localTasks.preview(prompt,sessionId)
     setPlan(preview)
     answer('Preparei um plano com '+preview.steps.length+' etapa(s). Confira os detalhes e clique em **Autorizar e executar** se concordar. Nenhuma ação foi executada ainda.')
    }catch(e){
     answer('Não há um executor seguro para essa solicitação nesta versão. '+errorMessage(e)+' Posso orientar a execução manual, mas não vou alegar que configurei algo sem fazer a alteração.')
    }
    return
   }
   if(!selected) {setError('Não há provedor ativo elegível. Ative uma IA gratuita nos Provedores.');setMode('idle');return}
   const result=await backend.chat({
    sessionId,prompt,
    history:turnsRef.current.slice(0,-1).filter(x=>x.content).slice(-12),
    agentPrompt:assistantInstructions,providerId:selected.id,autoFallback,
    freeOnly,browserCommands:false
   })
   setUsedProvider(result.provider+' / '+result.model)
   answer(result.content||'O provedor não enviou uma resposta em texto.')
  }catch(e){setError('Falha na conversa: '+errorMessage(e));setMode('idle')}
  finally{busyRef.current=false;setBusy(false);if(!voiceRef.current)setMode('idle')}
 }
 function toggleMic(){
  if(micOn){recognition.current?.stop();setMicOn(false);setMode('idle');return}
  const Host=(window as SpeechHost).SpeechRecognition||(window as SpeechHost).webkitSpeechRecognition
  if(!Host){setError('Reconhecimento de voz não disponível nesta WebView. Use o chat ou abra a Central em um ambiente compatível.');return}
  try{
   window.speechSynthesis?.cancel()
   const rec=new Host()
   rec.lang='pt-BR';rec.continuous=false;rec.interimResults=true
   rec.onresult=(event)=>{
    let finalText='';let preview=''
    for(let i=event.resultIndex;i<event.results.length;i++){
     const item=event.results[i]
     if(item.isFinal)finalText+=item[0].transcript
     else preview+=item[0].transcript
    }
    if(preview)setInterim(preview)
    if(finalText.trim()){setInterim('');rec.stop();void send(finalText.trim())}
   }
   rec.onerror=(event)=>{setMicOn(false);setMode('idle');setError('Microfone: '+event.error+'. Verifique a permissão do Windows e o dispositivo de entrada.')}
   rec.onend=()=>{setMicOn(false);setInterim('');setMode(old=>old==='listening'?'idle':old)}
   recognition.current=rec;rec.start();setMicOn(true);setMode('listening');setError('')
  }catch(e){setMicOn(false);setMode('idle');setError('Não foi possível iniciar o microfone: '+errorMessage(e))}
 }
 async function approve(){
  if(!plan||busyRef.current||!power)return
  if(!window.confirm('Autorizar a execução real de '+plan.steps.length+' etapa(s) descritas na prévia? Nenhum envio de formulário, alteração de BIOS ou shell arbitrário será feito.'))return
  const current=plan
  setPlan(null);busyRef.current=true;setBusy(true);setError('');setMode('acting')
  try{
   const result=await localTasks.execute(current.id,sessionId)
   setReport(result)
   answer((result.success?'Execução registrada. ':'Execução parcial/interrompida. ')+result.summary)
  }catch(e){setError('Erro durante a execução: '+errorMessage(e));setMode('idle')}
  finally{busyRef.current=false;setBusy(false)}
 }
 async function cancel(){
  if(plan){try{await localTasks.cancel(plan.id)}catch(e){setError(errorMessage(e))}}
  setPlan(null);setMode('idle')
 }
 function halt(){
  recognition.current?.stop();window.speechSynthesis?.cancel()
  setMicOn(false);setMode(busy?'thinking':'idle')
 }
 return <div className="living-page">
  <div className="living-heading"><div><div className="living-eyebrow"><span/> TRT ASSISTENTE VIVO · ARM64</div><h1>Seu assistente, com voz e opinião própria.</h1><p>Conversa por IA, análise crítica e execução supervisionada — integrado à Central TRT-Info.</p></div><span className="living-ram"><Activity size={15}/> Sem modelo pesado carregado no PC</span></div>
  <div className="living-layout">
   <section className="living-avatar-panel" aria-label="Rosto digital do assistente">
    <div className="living-orb"><WireFace mode={mode}/></div>
    <div className="living-identity"><span className={'living-status-dot '+mode}/><strong>{mode==='listening'?'Ouvindo...':mode==='thinking'?'Pensando...':mode==='speaking'?'Falando...':mode==='acting'?'Executando tarefa...':'Pronto para conversar'}</strong></div>
    <p className="living-whisper">{interim||'Toque no microfone para falar em português, ou digite sua mensagem.'}</p>
    <div className="living-controls">
     <button className={micOn?'active':''} title={!micSupported?'Reconhecimento de voz indisponível nesta WebView':micOn?'Parar de ouvir':'Falar comigo'} onClick={toggleMic} disabled={busy||!micSupported} aria-label="Alternar microfone">{micOn?<MicOff size={22}/>:<Mic size={22}/>}</button>
     <button className={voice?'active':''} title={voice?'Desligar minha voz':'Ligar minha voz'} onClick={()=>setVoice(v=>!v)} aria-label="Alternar resposta falada">{voice?<Volume2 size={21}/>:<VolumeX size={21}/>}</button>
     <button title="Interromper áudio" onClick={halt} aria-label="Parar reprodução de áudio"><Square size={18}/></button>
    </div>
    <div className="living-permission">
     <div><ShieldCheck size={20}/><span><strong>Poder de execução</strong><small>{power?'Habilitado para preparar planos; aprovação continua obrigatória':'Desligado. Conversas não executam ações.'}</small></span></div>
     <label className="living-switch" aria-label="Ativar poder de execução"><input type="checkbox" checked={power} onChange={e=>{setPower(e.target.checked);if(!e.target.checked)void cancel()}}/><span/></label>
    </div>
    <div className="living-privacy"><ShieldCheck size={14}/> Microfone somente ao clicar. O reconhecimento pode usar serviço online, conforme o Windows/WebView.</div>
   </section>
   <section className="living-chat-panel">
    <div className="living-chat-head"><div><h2><Sparkles size={18}/> Converse ou peça uma tarefa</h2><small>{usedProvider||selected?.name+' · '+selected?.model||'Selecione um provedor'}</small></div><span className="living-tag">{power?'Poder habilitado':'Apenas conversa'}</span></div>
    <div className="living-chat-log" role="log" aria-label="Conversa com o assistente">
     {turns.map((t,i)=><div className={'living-turn '+t.role} key={i}><span className="living-turn-author">{t.role==='assistant'?'ASSISTENTE':'VOCÊ'}</span><div className="living-turn-text"><ReactMarkdown remarkPlugins={[remarkGfm]}>{t.content}</ReactMarkdown></div></div>)}
     {busy&&<div className="living-typing">● ● ● Processando</div>}
     {error&&<p className="living-error" role="alert"><AlertTriangle size={16}/>{error}</p>}
     {plan&&<div className="living-plan"><h3><ClipboardList size={18}/> Plano aguardando autorização</h3><p>{plan.notice}</p><ol>{plan.steps.map(s=><li key={s.index}>{s.label}</li>)}</ol><div className="living-plan-actions"><button disabled={busy||!power} className="living-primary" onClick={()=>void approve()}><CheckCircle2 size={16}/> Autorizar e executar</button><button disabled={busy} onClick={()=>void cancel()}>Cancelar</button></div></div>}
     {report&&<p className={'living-report '+(report.success?'ok':'warn')}>{report.completed} de {report.steps.length} etapas concluídas.</p>}
     <div ref={endRef}/>
    </div>
    <div className="living-quick">{hints.map(h=><button key={h} onClick={()=>setEntry(h)}>{h}</button>)}</div>
    <div className="living-composer"><textarea aria-label="Mensagem para assistente" rows={2} value={entry} onChange={e=>setEntry(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();void send()}}} placeholder="Diga o que deseja, ou peça meu parecer técnico..." disabled={busy}/><button disabled={busy||!entry.trim()} title="Enviar" onClick={()=>void send()}><Send size={19}/></button></div>
    <div className="living-chat-footer"><label>IA ativa <select value={selected?.id||''} onChange={e=>setProviderId(e.target.value)}>{allowed.map(p=><option key={p.id} value={p.id}>{p.name} · {p.model}</option>)}</select></label><div><button onClick={onOpenProviders}>Gerenciar IAs</button><button onClick={onOpenAutomation}><Monitor size={14}/> Navegador / formulários</button></div></div>
   </section>
  </div>
  <div className="living-note"><Zap size={16}/><span>Autonomia responsável: o assistente pode discordar, explicar riscos e propor planos. Aplicativos, navegação e diagnóstico usam permissões explícitas. Instalações, BIOS, comandos livres e envio de formulários não são automatizados por este módulo.</span></div>
 </div>
}
