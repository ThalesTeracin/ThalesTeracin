import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { CheckCircle2, Database, Globe, Network, Plus, RefreshCw, ShieldCheck, Trash2, Wrench } from 'lucide-react'

type McpServer={id:string;name:string;url:string}
type McpTool={name:string;description?:string;inputSchema?:unknown}
const describe=(e:unknown)=>e instanceof Error?e.message:String(e)

export default function McpPanel({onUseContent}:{onUseContent:(text:string)=>void}){
 const [servers,setServers]=useState<McpServer[]>([])
 const [id,setId]=useState('')
 const [name,setName]=useState('')
 const [url,setUrl]=useState('http://127.0.0.1:3000/mcp')
 const [selected,setSelected]=useState('')
 const [tools,setTools]=useState<McpTool[]>([])
 const [tool,setTool]=useState('')
 const [args,setArgs]=useState('{}')
 const [result,setResult]=useState('')
 const [builtin,setBuiltin]=useState(false)
 const [busy,setBusy]=useState(false)
 const [status,setStatus]=useState('')
 async function reload(){const list=await invoke<McpServer[]>('mcp_servers');setServers(list)}
 useEffect(()=>{const t=setTimeout(()=>{void reload().catch(()=>{});void invoke<{running:boolean}>('mcp_builtin_status').then(s=>setBuiltin(s.running)).catch(()=>{})},0);return()=>clearTimeout(t)},[])
 async function action(run:()=>Promise<void>){
  setBusy(true);setStatus('')
  try{await run()}catch(e){setStatus('Erro: '+describe(e))}
  finally{setBusy(false)}
 }
 const startBuiltin=()=>void action(async()=>{
  const note=await invoke<string>('mcp_start_builtin')
  setBuiltin(true);await reload();setStatus(note)
 })
 const stopBuiltin=()=>{
  if(!window.confirm('Encerrar somente o servidor MCP local do TRT-Info?'))return
  void action(async()=>{await invoke<void>('mcp_stop_builtin');setBuiltin(false);setTools([]);setSelected('');setStatus('Servidor MCP local encerrado.')})
 }
 const add=()=>void action(async()=>{
  if(!window.confirm('Cadastrar o servidor MCP local '+name+' em '+url+'? Ele poderá oferecer ferramentas com ações no computador quando você aprovar cada execução.'))return
  const server={id,name,url};await invoke<void>('mcp_upsert_server',{server,confirmed:true})
  await reload();setStatus('Servidor local cadastrado. Nenhuma ferramenta foi executada.')
 })
 const remove=(server:McpServer)=>void action(async()=>{
  if(!window.confirm('Excluir somente o registro MCP "'+server.name+'"? Não será removido software externo.'))return
  await invoke<void>('mcp_remove_server',{id:server.id,confirmed:true})
  if(selected===server.id){setSelected('');setTools([]);setTool('')}
  await reload();setStatus('Registro removido.')
 })
 const discover=(server:McpServer)=>void action(async()=>{
  setSelected(server.id);setTools([]);setTool('');setResult('')
  const answer=await invoke<{tools:McpTool[]}>('mcp_list_tools',{id:server.id})
  setTools(answer.tools);setStatus(answer.tools.length+' ferramentas anunciadas por '+server.name)
 })
 const execute=()=>{
  if(!selected||!tool)return
  let parsed:unknown
  try{parsed=JSON.parse(args)}catch{setStatus('Argumentos JSON inválidos');return}
  if(!parsed||Array.isArray(parsed)||typeof parsed!=='object'){setStatus('Use um objeto JSON nos argumentos');return}
  if(!window.confirm('Executar a ferramenta MCP "'+tool+'" no servidor '+selected+' com estes argumentos?\n\n'+args.slice(0,2500)+'\n\nEsta ferramenta externa pode alterar dados conforme suas próprias permissões.'))return
  void action(async()=>{
   const response=await invoke<{result:string;isError:boolean}>('mcp_call_tool',{
     id:selected,tool,args:parsed,confirmed:true
   })
   setResult(response.result)
   setStatus(response.isError?'A ferramenta reportou um erro':'MCP executado mediante autorização explícita.')
  })
 }
 return <section className="panel mcp-panel" style={{marginTop:18}}>
  <div className="panel-head"><div><h2><Network size={18}/> Central MCP local</h2><p>Cadastre servidores MCP que estejam rodando no seu PC. Nenhuma configuração do OpenCode ou Claude Code será alterada.</p></div><span className="tag active">Aprovação por ação</span></div>
  <div className="info-banner"><ShieldCheck size={18}/><p>Por segurança, esta versão aceita somente servidores MCP HTTP no próprio computador (localhost). A descoberta é de leitura, mas executar uma ferramenta MCP pode alterar dados. Verifique argumentos e permissões antes de aprovar.</p></div>
  <div className="mcp-builtin-actions">
   <div><strong>Servidor MCP próprio do TRT-Info</strong><small>Três ferramentas locais de leitura: listar documentos, ler documento e consultar memória/arquitetura. Não inicia automaticamente com o Windows.</small></div>
   <span className={'tag '+(builtin?'active':'')}>{builtin?'Online':'Parado'}</span>
   {builtin?<button className="button secondary" onClick={stopBuiltin} disabled={busy}>Encerrar servidor</button>:<button className="button primary" onClick={startBuiltin} disabled={busy}><Plus size={15}/> Iniciar MCP local</button>}
  </div>
  <div className="mcp-registration">
    <div><label>ID do servidor</label><input value={id} onChange={e=>setId(e.target.value)} placeholder="mcp-local"/></div>
    <div><label>Nome</label><input value={name} onChange={e=>setName(e.target.value)} placeholder="MCP local"/></div>
    <div className="wide"><label>Endpoint HTTP local</label><input value={url} onChange={e=>setUrl(e.target.value)} placeholder="http://127.0.0.1:3000/mcp"/></div>
    <button className="button secondary" disabled={busy||!id.trim()||!name.trim()} onClick={add}><Plus size={15}/> Cadastrar</button>
  </div>
  <div className="mcp-server-list">
   {servers.length===0?<p className="muted">Nenhum servidor MCP cadastrado. Você pode registrar um endpoint MCP local compatível, sem instalar outro aplicativo pelo TRT-Info.</p>:servers.map(s=><div className="mcp-server" key={s.id}><Globe size={18}/><div><strong>{s.name}</strong><small>{s.url}</small></div><button className="button secondary" disabled={busy} onClick={()=>discover(s)}><RefreshCw size={14}/> Descobrir ferramentas</button><button className="icon-btn" disabled={busy} title="Remover registro" onClick={()=>remove(s)}><Trash2 size={16}/></button></div>)}
  </div>
  {selected&&<div className="mcp-tool-content"><h3><Wrench size={17}/> Ferramentas do servidor {selected}</h3><div className="mcp-tool-list">{tools.map(t=><button className={tool===t.name?'chosen':''} key={t.name} onClick={()=>{setTool(t.name);setArgs('{}');setResult('')}}><strong>{t.name}</strong><small>{t.description||'Sem descrição'}</small></button>)}</div>
  {tool&&<div className="mcp-execution"><label>Ferramenta selecionada: <strong>{tool}</strong></label><p className="muted">Argumentos JSON conforme o esquema declarado pelo servidor. Não cole segredos.</p><pre>{JSON.stringify(tools.find(t=>t.name===tool)?.inputSchema||{},null,2).slice(0,3000)}</pre><label>Argumentos</label><textarea value={args} onChange={e=>setArgs(e.target.value)} rows={5}/><button className="button primary" disabled={busy} onClick={execute}><CheckCircle2 size={16}/> Revisar e executar ferramenta</button></div>}</div>}
  {result&&<div className="mcp-result"><h3><Database size={17}/> Resultado da ferramenta</h3><pre>{result.slice(0,20000)}</pre><button className="button secondary" onClick={()=>onUseContent('Analise o resultado desta ferramenta MCP chamada '+tool+':\n\n'+result.slice(0,12000))}>Enviar resultado ao chat</button></div>}
  {status&&<div className="browser-feedback" role="status"><ShieldCheck size={16}/><span>{status}</span></div>}
 </section>
}
