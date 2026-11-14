import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { ArrowUpRight, GripHorizontal, Mic, MicOff, Send, Volume2, VolumeX, X } from 'lucide-react'
import { backend, type Provider } from '../lib/api'
import { WireFace } from './LivingAssistant'
import './FloatingCompanion.css'

type Mode='idle'|'thinking'|'speaking'|'listening'
type MiniMessage={role:'user'|'assistant';content:string}
type Recognizer={lang:string;interimResults:boolean;continuous:boolean;onresult:((event:{results:ArrayLike<{isFinal:boolean;0:{transcript:string}}>})=>void)|null;onend:(()=>void)|null;onerror:((event:{error:string})=>void)|null;start:()=>void;stop:()=>void}
type BrowserSpeech=Window & {SpeechRecognition?:new()=>Recognizer;webkitSpeechRecognition?:new()=>Recognizer}
type Props={providers:Provider[];freeOnly:boolean;autoFallback:boolean;onExpand:()=>void}
const critical=/^(?:abra|acesse|execute|instale|configure|apague|preencha|navegue|diagnostique|reinicie|formate|exclua|atualize)\b/i
const system='Você é o Assistente Vivo do TRT. Responda em português brasileiro com clareza, independência de julgamento e atenção à segurança. Prefira soluções gratuitas leves ARM64. Neste painel flutuante você apenas conversa: não afirme ter executado ações. Para executar tarefas reais, oriente a abrir a Central e autorizar o plano.'
export default function FloatingCompanion({providers,freeOnly,autoFallback,onExpand}:Props){
 const [mode,setMode]=useState<Mode>('idle')
 const [input,setInput]=useState('')
 const [messages,setMessages]=useState<MiniMessage[]>([{role:'assistant',content:'Estou aqui na sua tela. Pode falar comigo ou digitar. Para executar ações, abra a Central e autorize um plano.'}])
 const [error,setError]=useState('')
 const [voice,setVoice]=useState(true)
 const [mic,setMic]=useState(false)
 const [busy,setBusy]=useState(false)
 const [sessionId]=useState(()=>crypto.randomUUID())
 const recognizer=useRef<Recognizer|null>(null)
 const bottom=useRef<HTMLDivElement>(null)
 const live=useRef(true)
 const voiceRef=useRef(voice)
 const inFlight=useRef(false)
 const eligible=providers.find(p=>p.enabled&&!p.id.startsWith('coarena-')&&(!freeOnly||p.costStatus==='free'))
 useEffect(()=>{voiceRef.current=voice;if(!voice)window.speechSynthesis?.cancel()},[voice])
 useEffect(()=>{bottom.current?.scrollIntoView({block:'end'})},[messages,error])
 useEffect(()=>()=>{live.current=false;recognizer.current?.stop();window.speechSynthesis?.cancel()},[])
 function speak(text:string){
  if(!voiceRef.current||!('speechSynthesis' in window)){setMode('idle');return}
  window.speechSynthesis.cancel()
  const u=new SpeechSynthesisUtterance(text.replace(/[*#_~]/g,'').replace(/https?:\/\/\S+/g,'endereço').slice(0,1200))
  u.lang='pt-BR';u.rate=1.03
  const pt=window.speechSynthesis.getVoices().find(x=>x.lang.toLowerCase()==='pt-br')
  if(pt)u.voice=pt
  u.onstart=()=>{if(live.current)setMode('speaking')}
  u.onend=()=>{if(live.current)setMode('idle')}
  u.onerror=()=>{if(live.current)setMode('idle')}
  window.speechSynthesis.speak(u)
 }
 async function send(text?:string){
  const value=(text??input).trim()
  if(!value||inFlight.current)return
  setInput('');setError('');inFlight.current=true;setBusy(true);setMode('thinking')
  window.speechSynthesis?.cancel()
  const preceding=messages.slice(-12).map(x=>({...x}))
  setMessages(m=>[...m,{role:'user' as const,content:value}].slice(-24))
  try{
   if(critical.test(value)){
    const response='Para executar essa ação no Windows com segurança, abra a Central pelo botão de expansão. Lá eu preparo o plano e peço sua autorização.'
    setMessages(m=>[...m,{role:'assistant' as const,content:response}].slice(-24));speak(response)
   } else if(!eligible){
    throw new Error('Nenhum provedor gratuito ativo. Abra a Central e selecione um provedor.')
   } else {
    const result=await backend.chat({sessionId,prompt:value,history:preceding,agentPrompt:system,providerId:eligible.id,autoFallback,freeOnly,browserCommands:false})
    setMessages(m=>[...m,{role:'assistant' as const,content:result.content}].slice(-24));speak(result.content)
   }
  }catch(e){setError(String(e));setMode('idle')}
  finally{setBusy(false);inFlight.current=false}
 }
 function microphone(){
  if(mic){recognizer.current?.stop();setMic(false);setMode('idle');return}
  const Host=(window as BrowserSpeech).SpeechRecognition||(window as BrowserSpeech).webkitSpeechRecognition
  if(!Host){setError('Reconhecimento de voz não disponível nesta WebView; use o campo de texto.');return}
  try{
   const r=new Host();r.lang='pt-BR';r.interimResults=false;r.continuous=false
   r.onresult=e=>{const words=Array.from(e.results).filter(x=>x.isFinal).map(x=>x[0].transcript).join(' ');if(words){r.stop();void send(words)}}
   r.onend=()=>{setMic(false);setMode(x=>x==='listening'?'idle':x)}
   r.onerror=e=>{setMic(false);setMode('idle');setError('Microfone: '+e.error)}
   recognizer.current=r;r.start();setMic(true);setMode('listening')
  }catch(e){setError(String(e))}
 }
 function drag(){void invoke('drag_companion_window').catch(()=>{})}
 function close(){window.speechSynthesis?.cancel();void invoke('set_floating_mode',{floating:false}).then(onExpand).catch(e=>setError(String(e)))}
 return <main className="float-shell">
  <header className="float-header">
   <button className="float-drag" onMouseDown={drag} aria-label="Arrastar assistente"><GripHorizontal size={18}/><strong>TRT ASSISTENTE VIVO</strong><span>●</span></button>
   <button title="Expandir e abrir Central" onClick={close}><ArrowUpRight size={17}/></button>
   <button title="Fechar modo flutuante" onClick={close}><X size={17}/></button>
  </header>
  <section className="float-face"><WireFace mode={mode}/><div className="float-status">{mode==='thinking'?'Pensando...':mode==='speaking'?'Falando...':mode==='listening'?'Ouvindo...':'Pronto para ajudar'}</div></section>
  <nav className="float-buttons" aria-label="Controles de voz">
   <button onClick={microphone} className={mic?'on':''} disabled={busy} title={mic?'Desligar microfone':'Falar'}>{mic?<MicOff size={19}/>:<Mic size={19}/>}</button>
   <button onClick={()=>setVoice(v=>!v)} className={voice?'on':''} title={voice?'Silenciar':'Ativar voz'}>{voice?<Volume2 size={19}/>:<VolumeX size={19}/>}</button>
   <button onClick={close} title="Abrir chat e tarefas na Central"><ArrowUpRight size={19}/></button>
  </nav>
  <section className="float-transcript" aria-label="Conversa">{messages.slice(-4).map((m,i)=><p key={i} className={'float-message '+m.role}>{m.content}</p>)}{error&&<p role="alert" className="float-error">{error}</p>}<div ref={bottom}/></section>
  <form className="float-input" onSubmit={e=>{e.preventDefault();void send()}}><input value={input} onChange={e=>setInput(e.target.value)} placeholder={busy?'Respondendo...':'Fale ou digite aqui...'} disabled={busy}/><button disabled={!input.trim()||busy} aria-label="Enviar mensagem"><Send size={18}/></button></form>
  <footer className="float-foot">Ações no Windows exigem autorização na Central.</footer>
 </main>
}
