import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { backend } from '../lib/api'
import { AlertTriangle, ArrowRight, CheckCircle2, Compass, FileText, Globe2, KeyRound, LockKeyhole, MousePointer2, RefreshCw, ShieldCheck, Square, SquareArrowOutUpRight, XCircle } from 'lucide-react'

type BrowserOption={value:string;label:string}
type BrowserField={
 tag:string;type:string;id:string;name:string;label:string;placeholder:string;
 required:boolean;selector:string;formId:string;autocomplete:string;options:BrowserOption[]
}
type BrowserLink={text:string;url:string}
type BrowserForm={id:string;name:string;action:string;method:string;fields:number;hasPassword:boolean;hasCaptcha:boolean;submitButtons:string[]}
type BrowserResult={
 ok:boolean;title:string;url:string;headings:string[];fields:BrowserField[];
 links:BrowserLink[];forms:BrowserForm[];message:string
}
type BrowserStatus={running:boolean;profile:string;isolation:string;manualSubmit:boolean}
type Permission={active:boolean;origin:string|null;expiresInSeconds:number;remainingFills:number}
const isSensitive=(f:BrowserField)=>{
 const terms=[f.id,f.name,f.label,f.placeholder,f.autocomplete].join(' ')
 return /password|senha|pass(word)?|otp|2fa|token|secret|cvv|cvc|credit|card|cart[aã]o|cpf|ssn|security.code|pix|bank|banc[oá]ri|captcha|documento|passport|rg\b/i.test(terms)
}
function allowedField(field:BrowserField) {
 return !!field.selector && !isSensitive(field) &&
  (field.tag==='select' || (['input','textarea'].includes(field.tag)&&
   ['text','search','email','tel','url','number','date'].includes(field.type)))
}
function describe(e:unknown){return e instanceof Error?e.message:String(e)}
function currentOrigin(url:string){try{return new URL(url).origin}catch{return ''}}
export default function BrowserPanel({onUseContent}:{onUseContent?:(text:string)=>void}){
 const [status,setStatus]=useState<BrowserStatus|null>(null)
 const [page,setPage]=useState<BrowserResult|null>(null)
 const [permission,setPermission]=useState<Permission|null>(null)
 const [address,setAddress]=useState('https://example.org/')
 const [singleSelector,setSingleSelector]=useState('')
 const [singleValue,setSingleValue]=useState('')
 const [draft,setDraft]=useState<Record<string,string>>({})
 const [busy,setBusy]=useState(false)
 const [feedback,setFeedback]=useState('')
 const [allowDomain,setAllowDomain]=useState(false)
 const [aiInstructions,setAiInstructions]=useState('')
 const [aiConsent,setAiConsent]=useState(false)
 const [aiBusy,setAiBusy]=useState(false)

 async function refreshPermissions(){
  const p=await invoke<Permission>('browser_permission_status');setPermission(p)
 }
 useEffect(()=>{
  const t=setTimeout(()=>{
   void invoke<BrowserStatus>('browser_status').then(setStatus).catch(()=>{})
   void invoke<Permission>('browser_permission_status').then(setPermission).catch(()=>{})
  },0);return()=>clearTimeout(t)
 },[])
 async function run(task:()=>Promise<void>){
  setBusy(true);setFeedback('')
  try{await task()}catch(e){setFeedback('Erro: '+describe(e))}
  finally{setBusy(false)}
 }
 function updatePage(result:BrowserResult){
  setPage(result);setAddress(result.url.startsWith('http')?result.url:address)
  setDraft({});setSingleSelector('');setSingleValue('');setAllowDomain(false)
  void refreshPermissions().catch(()=>{})
 }
 function begin(){
  void run(async()=>{
   const result=await invoke<string>('browser_start')
   setStatus(await invoke<BrowserStatus>('browser_status'))
   setFeedback(result)
  })
 }
 function inspect(){
  void run(async()=>{
   const result=await invoke<BrowserResult>('browser_action',{input:{action:'inspect'}})
   updatePage(result);setFeedback(result.message)
  })
 }
 function navigate(target=address){
  void run(async()=>{
   const url=new URL(target)
   if(!['http:','https:'].includes(url.protocol)||url.username||url.password)throw Error('Apenas URLs HTTP(S) sem credenciais.')
   const result=await invoke<BrowserResult>('browser_action',{input:{action:'navigate',url:target}})
   updatePage(result);setFeedback(result.message)
  })
 }
 function grant(){
  if(!page||!allowDomain)return
  if(!window.confirm('Autorizar até 10 preenchimentos durante 15 minutos SOMENTE no site '+currentOrigin(page.url)+'? Você poderá revogar a qualquer momento. Esta autorização NÃO permite enviar formulários.'))return
  void run(async()=>{
   const p=await invoke<{origin:string;remainingFills:number}>('browser_grant',{url:page.url,allowFill:true})
   await refreshPermissions();setAllowDomain(false)
   setFeedback('Permissão temporária concedida a '+p.origin+' ('+p.remainingFills+' operações).')
  })
 }
 function revoke(){
  void run(async()=>{
   await invoke<void>('browser_revoke');await refreshPermissions();setAllowDomain(false)
   setFeedback('Permissão de preenchimento revogada.')
  })
 }
 function fillOne(){
  if(!page||!singleSelector.trim()||!singleValue.trim())return
  if(!window.confirm('Preencher SOMENTE '+singleSelector+' em '+currentOrigin(page.url)+'? Não será enviado nenhum formulário.'))return
  void run(async()=>{
   const result=await invoke<BrowserResult>('browser_action',{input:{
    action:'fill',selector:singleSelector,value:singleValue,expectedUrl:page.url,confirmed:true
   }})
   setPage(result);setSingleValue('');await refreshPermissions()
   setFeedback('Campo preenchido. Revise o resultado no Chrome.')
  })
 }
 function fillBatch(){
  if(!page)return
  const fields=Object.entries(draft).filter(([,value])=>value.trim()).map(([selector,value])=>({selector,value}))
  if(!fields.length){setFeedback('Digite o valor de pelo menos um campo.');return}
  if(fields.length>20){setFeedback('Máximo de 20 campos por lote.');return}
  const summary=fields.map((x,i)=>(i+1)+'. '+x.selector+' = '+x.value.slice(0,90)).join('\n')
  if(!window.confirm('CONFIRMAR PREENCHIMENTO DE CADASTRO\n\nSite: '+page.url+'\nCampos: '+fields.length+'\n\n'+summary.slice(0,2200)+'\n\nOs dados serão digitados no site; nenhum formulário será enviado automaticamente.'))return
  void run(async()=>{
   const result=await invoke<BrowserResult>('browser_action',{input:{
    action:'fill_batch',expectedUrl:page.url,fields,confirmed:true
   }})
   setPage(result);setDraft({});await refreshPermissions()
   setFeedback(fields.length+' campo(s) preenchido(s). Revise e envie manualmente pelo Chrome.')
  })
 }
 async function suggestFieldsWithAI(){
  if(!page||!aiInstructions.trim()||!aiConsent){setFeedback('Digite as instruções e autorize o envio ao modelo externo.');return}
  const allowed=page.fields.filter(allowedField).slice(0,20)
  if(!allowed.length){setFeedback('Esta página não possui campos permitidos para preenchimento assistido.');return}
  if(!window.confirm('Enviar para a API de IA os nomes dos campos e o texto que você forneceu? O provedor pode processar esses dados conforme sua política. Nenhum cadastro será preenchido sem sua revisão posterior.'))return
  setAiBusy(true);setFeedback('')
  try{
   const info=allowed.map(f=>({selector:f.selector,label:f.label,placeholder:f.placeholder,type:f.type,options:f.options}))
   const instruction='Você é assistente de preenchimento de formulários. Gere apenas JSON válido no formato {"fields":[{"selector":"#id","value":"texto"}]}. Use SOMENTE os seletores fornecidos, sem criar fatos ou inventar dados. Não preencha senhas, códigos de autenticação, números de documentos ou dados bancários. Se não houver dados suficientes, omita o campo. Limite 20 campos. Nenhuma ação no navegador é permitida.'
   const response=await backend.chat({
    sessionId:crypto.randomUUID(),prompt:'Campos disponíveis: '+JSON.stringify(info).slice(0,12000)+'\n\nInformações fornecidas pelo operador: '+aiInstructions.slice(0,8000),
    history:[],agentPrompt:instruction,autoFallback:true,freeOnly:true
   })
   const raw=response.content.trim().replace(/^```(?:json)?\s*/i,'').replace(/\s*```$/,'')
   const output:unknown=JSON.parse(raw)
   const fields=(output as {fields?:unknown})?.fields
   if(!Array.isArray(fields))throw Error('A IA não retornou uma lista válida de campos.')
   const known=new Set(allowed.map(f=>f.selector))
   const suggested:Record<string,string>={}
   for(const field of fields.slice(0,20)){
    if(!field||typeof field!=='object')continue
    const obj=field as {selector?:unknown;value?:unknown}
    if(typeof obj.selector==='string'&&known.has(obj.selector)&&typeof obj.value==='string'&&obj.value.length<=8000){
      suggested[obj.selector]=obj.value
    }
   }
   setDraft(suggested);setAiConsent(false)
   setFeedback(Object.keys(suggested).length+' sugestão(ões) preparada(s). Revise cada valor e clique em Revisar e preencher cadastro.')
  }catch(e){setFeedback('Não foi possível preparar as sugestões: '+describe(e))}
  finally{setAiBusy(false)}
 }
 function stop(){
  if(!window.confirm('Encerrar somente o Chrome isolado criado pelo TRT-Info?'))return
  void run(async()=>{
   await invoke<void>('browser_stop')
   setStatus(await invoke<BrowserStatus>('browser_status'));await refreshPermissions()
   setPage(null);setDraft({});setFeedback('Sessão isolada encerrada; permissões revogadas.')
  })
 }
 const grantActive=!!(page&&permission?.active&&permission.origin===currentOrigin(page.url))
 const editableFields=page?.fields.filter(allowedField)||[]
 const changed=Object.entries(draft).filter(([,v])=>v.trim()).length
 return <section className="panel browser-panel">
  <div className="panel-head"><div><h2><Globe2 size={18}/> Navegação e cadastros autorizados</h2><p>Chrome visível e isolado, com permissões limitadas ao site e à sua autorização.</p></div>
   <span className={'tag '+(status?.running?'active':'')}>{status?.running?'Sessão ativa':'Sessão parada'}</span></div>
  <div className="info-banner"><LockKeyhole size={18}/><p>A navegação e a inspeção são iniciadas por você. Para preencher, conceda autorização temporária ao domínio e revise os valores antes de confirmar. Não são acessados o Chrome pessoal, senhas, códigos de autenticação ou dados bancários.</p></div>
  <div className="browser-buttons">
   <button className="button primary" disabled={busy||status?.running} onClick={begin}><Compass size={16}/> Iniciar Chrome isolado</button>
   <button className="button secondary" disabled={busy} onClick={()=>void run(async()=>{setStatus(await invoke<BrowserStatus>('browser_status'));await refreshPermissions()})}><RefreshCw size={15}/> Atualizar</button>
   <button className="button secondary" disabled={busy||!status?.running} onClick={stop}><Square size={15}/> Encerrar</button>
  </div>
  <div className="browser-input-row"><label htmlFor="browser-site">1. Navegar para um site</label><div>
   <input type="url" id="browser-site" value={address} onChange={e=>setAddress(e.target.value)} placeholder="https://site-oficial.com.br"/>
   <button className="button secondary" disabled={busy||!status?.running} onClick={()=>navigate()}><ArrowRight size={16}/> Navegar</button>
   <button className="button secondary" disabled={busy||!status?.running} onClick={inspect}><FileText size={16}/> Inspecionar</button>
  </div></div>
  {page&&<div className="browser-preview">
   <div className="browser-page-title"><div><strong>{page.title||'Página sem título'}</strong><small>{page.url}</small></div><SquareArrowOutUpRight size={18}/></div>
   {onUseContent&&<button className="button secondary" style={{marginTop:15}} onClick={()=>onUseContent('Analise a estrutura desta página, sem afirmar ter executado ações. URL: '+page.url+'\nCabeçalhos: '+page.headings.join('; ')+'\nCampos sem valores: '+page.fields.map(f=>f.label||f.name||f.id).join('; '))}><FileText size={16}/> Analisar página com um agente</button>}
   <div className="browser-preview-grid">
    <div><h3>Cabeçalhos</h3>{page.headings.length?page.headings.map((x,i)=><p key={i}>{x}</p>):<p>Não identificados</p>}</div>
    <div><h3>Links</h3>{page.links.slice(0,10).map((x,i)=><button key={i} className="browser-link" disabled={busy} onClick={()=>navigate(x.url)}>{x.text}<ArrowRight size={13}/></button>)}</div>
   </div>
   <h3>Formulários encontrados ({page.forms?.length||0})</h3>
   {page.forms?.map((form,i)=><p className="muted" key={i}>{form.name||form.id||'Formulário '+(i+1)} · {form.method} · {form.fields} campos{form.hasPassword?' · contém senha':''}{form.hasCaptcha?' · contém CAPTCHA':''}</p>)}
   <div className="browser-permission-panel">
    <div className="panel-head"><div><h2><ShieldCheck size={18}/> 2. Autorizar este site</h2><p>Origem: {currentOrigin(page.url)}</p></div><span className={'tag '+(grantActive?'active':'')}>{grantActive?'Autorizado':'Sem autorização'}</span></div>
    {grantActive?<><p>Restam {permission?.remainingFills} preenchimentos; expira em até {Math.ceil((permission?.expiresInSeconds||0)/60)} minutos.</p>
      <button className="button secondary" disabled={busy} onClick={revoke}><XCircle size={16}/> Revogar acesso agora</button></>:
     <><label className="browser-approval"><input type="checkbox" checked={allowDomain} onChange={e=>setAllowDomain(e.target.checked)}/> Autorizo preencher campos comuns apenas neste domínio, por até 15 minutos e 10 operações. Sei que o site pode salvar dados enquanto digito.</label>
       <button className="button primary" disabled={busy||!allowDomain} onClick={grant}><KeyRound size={16}/> Conceder permissão temporária</button></>}
   </div>
   <div className="browser-fill-batch"><h3>3. Preparar cadastro para preenchimento ({editableFields.length} campos permitidos)</h3>
    <div className="browser-ai-helper">
     <h3>Assistente de IA para sugerir o preenchimento</h3>
     <p className="muted">Você pode informar os dados em linguagem natural. A IA apenas prepara uma prévia editável: não digita nada automaticamente no site.</p>
     <textarea rows={3} value={aiInstructions} onChange={e=>{setAiInstructions(e.target.value);setAiConsent(false)}} placeholder="Exemplo: Preencha Nome como Maria Silva e Cidade como Ribeirão Preto. Não invente outros dados."/>
     <label className="browser-approval"><input type="checkbox" checked={aiConsent} onChange={e=>setAiConsent(e.target.checked)}/> Autorizo enviar este texto e os rótulos dos campos para o provedor externo de IA gratuito selecionado. Não informarei senhas nem dados sensíveis.</label>
     <button className="button secondary" disabled={busy||aiBusy||!aiConsent||!aiInstructions.trim()||!editableFields.length} onClick={()=>void suggestFieldsWithAI()}><FileText size={16}/>{aiBusy?'Consultando IA...':'IA: sugerir preenchimento'}</button>
    </div>
    <p className="muted">Digite os valores abaixo. Eles ficarão apenas nesta sessão da interface até você confirmar e não serão registrados no histórico de ações.</p>
    {editableFields.length?<div className="browser-batch-grid">{editableFields.map((f,i)=><label key={f.selector+i}><span>{f.label||f.placeholder||f.name||f.id||f.selector}{f.required?' *':''}</span><small>{f.type} · {f.selector}</small>
      {f.tag==='select'?<select disabled={busy} value={draft[f.selector]||''} onChange={e=>setDraft(v=>({...v,[f.selector]:e.target.value}))}><option value="">Selecionar...</option>{f.options.map(o=><option value={o.value} key={o.value}>{o.label||o.value}</option>)}</select>:
      <input disabled={busy} type="text" value={draft[f.selector]||''} placeholder="Valor a preencher" onChange={e=>setDraft(v=>({...v,[f.selector]:e.target.value}))}/>}
    </label>)}</div>:<p className="muted">Não há campos comuns com identificação única. Você pode preenchê-los manualmente no Chrome.</p>}
    <div className="browser-batch-toolbar"><span>{changed} campo(s) preparados</span><button className="button secondary" onClick={()=>setDraft({})} disabled={busy}>Limpar</button>
      <button className="button primary" disabled={busy||!grantActive||!changed} onClick={fillBatch}><CheckCircle2 size={16}/> Revisar e preencher cadastro</button>
    </div>
   </div>
  </div>}
  <div className="browser-fill"><div className="panel-head"><div><h2><MousePointer2 size={17}/> Preenchimento individual avançado</h2><p>Para campos não listados, use um seletor CSS seguro e confirmação.</p></div></div>
   <label>Seletor CSS</label><input value={singleSelector} placeholder="#nome" onChange={e=>setSingleSelector(e.target.value)}/>
   <label>Texto a preencher</label><textarea value={singleValue} rows={2} onChange={e=>setSingleValue(e.target.value)} placeholder="Valor do campo"/>
   <button className="button secondary" disabled={busy||!grantActive||!singleSelector.trim()||!singleValue.trim()} onClick={fillOne}><ShieldCheck size={16}/> Preencher campo individual</button>
  </div>
  {feedback&&<div className="browser-feedback" role="status"><AlertTriangle size={16}/><span>{feedback}</span></div>}
  <small className="muted">Envio final, autenticação e CAPTCHAs continuam manuais. Você decide quando enviar o cadastro pelo Chrome. O TRT-Info registra ação, domínio e resultado sem guardar os valores preenchidos.</small>
 </section>
}
