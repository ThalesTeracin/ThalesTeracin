import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { CheckCircle2, FileCode2, FileText, Folder, FolderOpen, History, Plus, Search, ShieldCheck, X } from 'lucide-react'

type ProjectFile={path:string;size:number;isDir:boolean}
type ProjectResult={action:string;path:string;detail:string;content?:string;backup?:string}
type AuditEntry={timestamp:number;action:string;path:string;outcome:string}
function describe(error:unknown){return error instanceof Error?error.message:String(error)}

export default function AgentToolsPanel({onUseContent}:{onUseContent:(content:string)=>void}){
 const [folder,setFolder]=useState('.')
 const [items,setItems]=useState<ProjectFile[]>([])
 const [query,setQuery]=useState('')
 const [selected,setSelected]=useState('')
 const [content,setContent]=useState('')
 const [draft,setDraft]=useState('')
 const [draftPath,setDraftPath]=useState('docs/nota-trt.md')
 const [audit,setAudit]=useState<AuditEntry[]>([])
 const [busy,setBusy]=useState(false)
 const [feedback,setFeedback]=useState('')
 const [tab,setTab]=useState<'files'|'write'|'audit'>('files')
 const [ack,setAck]=useState(false)

 const refresh=async(dir=folder)=>{
   const list=await invoke<ProjectFile[]>('project_list',{path:dir})
   setFolder(dir);setItems(list)
 }
 const loadAudit=async()=>setAudit(await invoke<AuditEntry[]>('tool_audit_history'))
 useEffect(()=>{const init=setTimeout(()=>{void invoke<ProjectFile[]>('project_list',{path:'.'}).then(setItems).catch(()=>{})},0);return()=>clearTimeout(init)},[])
 async function perform(action:()=>Promise<void>){
  setBusy(true);setFeedback('')
  try{await action()}catch(e){setFeedback('Erro: '+describe(e))}
  finally{setBusy(false)}
 }
 const enter=(target:string)=>void perform(async()=>{await refresh(target)})
 const back=()=>{const next=folder.split('/').slice(0,-1).join('/')||'.';enter(next)}
 const open=(file:ProjectFile)=>{
  if(file.isDir){enter(file.path);return}
  void perform(async()=>{
   const result=await invoke<ProjectResult>('project_read',{path:file.path})
   setContent(result.content||'');setSelected(file.path);setFeedback('Documento lido sem modificar o arquivo.')
  })
 }
 const search=()=>void perform(async()=>{
  if(query.trim().length<2){await refresh();return}
  const found=await invoke<ProjectFile[]>('project_search',{query:query.trim()})
  setItems(found);setFeedback(found.length+' arquivo(s) encontrados.')
 })
 const save=()=>{
  if(!ack){setFeedback('Confirme a gravação antes de continuar.');return}
  if(!window.confirm('Gravar o documento '+draftPath+'? Se já existir, o TRT-Info fará backup antes de substituir.'))return
  void perform(async()=>{
   const result=await invoke<ProjectResult>('project_write',{request:{
    action:'write',path:draftPath,content:draft,confirmed:true
   }})
   setAck(false);setFeedback(result.detail+(result.backup?' Backup: '+result.backup:''))
   setSelected(draftPath);setContent(draft);await loadAudit()
  })
 }
 return <div className="standard-page agent-tools-page">
  <div className="page-title"><div><div className="eyebrow">AGENTES COM FERRAMENTAS</div><h1>TRT Tool Studio</h1><p>Arquivos e conhecimento do projeto, sempre com permissões explícitas.</p></div><span className="tag active"><ShieldCheck size={14}/> Acesso restrito ao projeto</span></div>
  <div className="info-banner"><ShieldCheck size={19}/><p>Operações somente na pasta TRT-Info-GPT. Leitura apenas de arquivos .md/.txt; gravação somente em docs/ ou data/. Nenhum comando de terminal, exclusão de arquivos ou alteração de outras IAs.</p></div>
  <div className="agent-tool-tabs">
   <button className={tab==='files'?'current':''} onClick={()=>setTab('files')}><FolderOpen size={16}/> Explorar documentos</button>
   <button className={tab==='write'?'current':''} onClick={()=>setTab('write')}><FileCode2 size={16}/> Criar documento</button>
   <button className={tab==='audit'?'current':''} onClick={()=>{setTab('audit');void perform(loadAudit)}}><History size={16}/> Auditoria</button>
  </div>
  {tab==='files'&&<div className="agent-file-layout"><section className="panel">
    <div className="panel-head"><div><h2><FolderOpen size={18}/> Arquivos do projeto</h2><p>Diretório: {folder}</p></div><button className="button secondary" disabled={busy||folder==='.'} onClick={back}>Voltar</button></div>
    <div className="agent-file-search"><input value={query} onChange={e=>setQuery(e.target.value)} placeholder="Buscar nomes em docs, config e data..." onKeyDown={e=>{if(e.key==='Enter')search()}}/><button className="button secondary" disabled={busy} onClick={search}><Search size={16}/> Buscar</button></div>
    <div className="agent-file-items">{items.map(file=><button key={file.path} disabled={busy} onClick={()=>open(file)}><span>{file.isDir?<Folder size={17}/>:<FileText size={17}/>}</span><span><strong>{file.path.split('/').at(-1)}</strong><small>{file.path}</small></span>{file.isDir?<Plus size={14}/>:<small>{Math.ceil(file.size/1024)} KB</small>}</button>)}{items.length===0&&<p className="muted">Nenhum arquivo encontrado.</p>}</div>
   </section><section className="panel"><div className="panel-head"><div><h2><FileText size={18}/> Conteúdo selecionado</h2><p>{selected||'Selecione um documento Markdown ou TXT.'}</p></div></div>
    {content?<><pre className="agent-file-preview">{content.slice(0,22000)}</pre><button className="button primary" onClick={()=>onUseContent('Analise o conteúdo do documento '+selected+' e responda em português:\n\n'+content.slice(0,14000))}><FileText size={16}/> Usar no chat com IA</button></>:<p className="muted">A prévia aparecerá aqui após escolher um documento permitido.</p>}
   </section></div>}
  {tab==='write'&&<section className="panel agent-write-panel"><div className="panel-head"><div><h2><FileCode2 size={18}/> Redigir e salvar</h2><p>Acesso apenas a documentos de texto. Se houver um arquivo anterior, um backup será criado.</p></div></div>
   <label>Caminho relativo (docs/ ou data/)</label><input value={draftPath} onChange={e=>{setDraftPath(e.target.value);setAck(false)}} placeholder="docs/nota-trt.md"/>
   <label>Conteúdo do documento</label><textarea value={draft} onChange={e=>{setDraft(e.target.value);setAck(false)}} rows={12} placeholder="# Minhas anotações..."/>
   <label className="browser-approval"><input type="checkbox" checked={ack} onChange={e=>setAck(e.target.checked)}/> Confirmo que desejo salvar o documento neste caminho e autorizo criar backup do arquivo anterior.</label>
   <button className="button primary" disabled={busy||!ack||!draft.trim()||!draftPath.trim()} onClick={save}><CheckCircle2 size={16}/> Confirmar e salvar documento</button>
  </section>}
  {tab==='audit'&&<section className="panel"><div className="panel-head"><div><h2><History size={18}/> Histórico de ferramentas</h2><p>Somente data, ação, nome do arquivo e resultado. O conteúdo não é registrado.</p></div><button className="button secondary" onClick={()=>void perform(loadAudit)}>Atualizar</button></div>{audit.length===0?<p className="muted">Nenhuma ação registrada.</p>:audit.map((entry,i)=><div className="route-line" key={i}><span className="route-index">{entry.action==='read'?'L':'G'}</span><div><strong>{entry.path}</strong><small>{new Date(entry.timestamp*1000).toLocaleString('pt-BR')}</small></div><span className="tag active">{entry.outcome}</span></div>)}</section>}
  {feedback&&<div className="browser-feedback" role="status"><FileText size={16}/><span>{feedback}</span><button className="icon-btn" aria-label="Fechar mensagem" onClick={()=>setFeedback('')}><X size={16}/></button></div>}
 </div>
}
