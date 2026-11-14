use std::{
    fs::{self,OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Child,Command,Stdio},
    net::{SocketAddr,TcpStream},
    sync::Mutex,
    thread,
    time::Duration,
    time::{SystemTime,UNIX_EPOCH},
};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};

const ROOT: &str = r"C:\TRT-Assistente-Vivo-Standalone";
const NODE: &str = r"C:\Program Files\nodejs\node.exe";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    servers: Vec<McpServer>,
}

const BUILTIN_PORT: u16 = 49321;

pub struct BuiltInMcpState {
    child: Mutex<Option<Child>>,
}
impl Default for BuiltInMcpState {
    fn default()->Self {Self{child:Mutex::new(None)}}
}
impl Drop for BuiltInMcpState {
    fn drop(&mut self){
        if let Ok(mut child)=self.child.lock(){
            if let Some(ref mut process)=*child {
                let _=process.kill();
                let _=process.wait();
            }
        }
    }
}
fn builtin_running()->bool{
    let addr=SocketAddr::from(([127,0,0,1],BUILTIN_PORT));
    TcpStream::connect_timeout(&addr,Duration::from_millis(150)).is_ok()
}
#[tauri::command]
pub fn mcp_builtin_status()->Value {
    json!({"running":builtin_running(),"url":"http://127.0.0.1:49321/mcp"})
}
#[tauri::command]
pub fn mcp_start_builtin(state:tauri::State<'_,BuiltInMcpState>)->Result<String,String>{
    let mut child=state.child.lock().map_err(|_|"Controle MCP indisponível")?;
    if builtin_running() {
        return Err("A porta MCP 49321 já está ocupada. Não foi iniciado outro servidor.".into());
    }
    if let Some(ref mut previous)=*child {
        if previous.try_wait().map_err(|e|e.to_string())?.is_none(){
            return Err("Servidor MCP já está iniciando".into());
        }
    }
    let script=PathBuf::from(ROOT).join("scripts").join("trt-mcp-local.cjs");
    if !script.is_file(){return Err("Servidor MCP local não está instalado".into())}
    let proc=Command::new(NODE).arg(script)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|e|format!("Falha ao iniciar MCP local: {e}"))?;
    *child=Some(proc);
    for _ in 0..60 {
        if builtin_running(){
            let mut registry=read_registry()?;
            let server=McpServer{id:"trt-builtin".into(),name:"TRT Ferramentas locais".into(),
                url:"http://127.0.0.1:49321/mcp".into()};
            if let Some(existing)=registry.servers.iter_mut().find(|s|s.id=="trt-builtin"){
                *existing=server;
            }else{registry.servers.push(server)}
            save_registry(&registry)?;
            return Ok("MCP local iniciado com três ferramentas somente leitura".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    if let Some(mut proc)=child.take(){
        let _=proc.kill();
        let _=proc.wait();
    }
    Err("Servidor MCP não respondeu na porta local".into())
}
#[tauri::command]
pub fn mcp_stop_builtin(state:tauri::State<'_,BuiltInMcpState>)->Result<(),String>{
    let mut child=state.child.lock().map_err(|_|"Controle MCP indisponível")?;
    if let Some(mut process)=child.take(){
        let _=process.kill();
        let _=process.wait();
    }
    Ok(())
}

fn registry_path()->PathBuf {
    PathBuf::from(ROOT).join("config").join("mcp").join("servers.json")
}
fn read_registry()->Result<Registry,String>{
    let path=registry_path();
    if !path.exists(){return Ok(Registry::default())}
    let bytes=fs::read(path).map_err(|e|e.to_string())?;
    if bytes.len()>40000 {return Err("Catálogo MCP excede o tamanho permitido".into())}
    serde_json::from_slice(&bytes).map_err(|_|"Catálogo MCP inválido".into())
}
fn validate_mcp_url(url: &str)->Result<(),String>{
    let parsed=url::Url::parse(url).map_err(|_|"URL inválida".to_string())?;
    let host=parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if parsed.scheme()!="http" || !["localhost","127.0.0.1","[::1]"].contains(&host.as_str())
        || !parsed.username().is_empty() || parsed.password().is_some()
        || parsed.fragment().is_some() || parsed.port().is_none() {
        return Err("Servidor MCP deve usar HTTP local, porta explícita e URL sem credenciais".into());
    }
    Ok(())
}
fn save_registry(reg: &Registry)->Result<(),String>{
    let target=registry_path();
    fs::create_dir_all(target.parent().ok_or("Pasta MCP inválida")?)
        .map_err(|e|e.to_string())?;
    let json=serde_json::to_vec_pretty(reg).map_err(|e|e.to_string())?;
    let temp=target.with_extension("json.tmp");
    fs::write(&temp,&json).map_err(|e|e.to_string())?;
    if target.exists(){
        let backup=target.with_extension("json.bak");
        fs::copy(&target,backup).map_err(|e|e.to_string())?;
        // Keep original until a backup exists. Windows rename cannot replace every open file.
        fs::copy(&temp,&target).map_err(|e|e.to_string())?;
        let _=fs::remove_file(temp);
    }else{
        fs::rename(temp,target).map_err(|e|e.to_string())?;
    }
    Ok(())
}
fn audit(action:&str,server:&str,tool:&str,success:bool){
    let file=PathBuf::from(ROOT).join("logs").join("mcp-actions.jsonl");
    let Ok(mut f)=OpenOptions::new().create(true).append(true).open(file) else{return};
    let t=SystemTime::now().duration_since(UNIX_EPOCH).map(|v|v.as_secs()).unwrap_or_default();
    let tool: String=tool.chars().filter(|c|c.is_ascii_alphanumeric()||*c=='_'||*c=='-').take(100).collect();
    let _=writeln!(f,"{}",json!({"time":t,"action":action,"server":server,"tool":tool,"success":success}));
}

#[tauri::command]
pub fn mcp_servers()->Result<Vec<McpServer>,String>{
    Ok(read_registry()?.servers)
}
#[tauri::command]
pub fn mcp_upsert_server(server:McpServer,confirmed:bool)->Result<(),String>{
    if !confirmed {return Err("Confirmação explícita obrigatória".into())}
    if server.id=="trt-builtin" {return Err("ID reservado para o MCP interno".into())}
    if server.id.is_empty()||server.id.len()>60
        || !server.id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'||c=='_')
        || server.name.trim().is_empty()||server.name.len()>120{
        return Err("ID ou nome MCP inválido".into())
    }
    validate_mcp_url(&server.url)?;
    let mut reg=read_registry()?;
    if let Some(existing)=reg.servers.iter_mut().find(|s|s.id==server.id){
        *existing=server;
    }else{
        if reg.servers.len()>=20{return Err("Limite de 20 servidores locais".into())}
        reg.servers.push(server);
    }
    save_registry(&reg)
}
#[tauri::command]
pub fn mcp_remove_server(id:String,confirmed:bool)->Result<(),String>{
    if !confirmed {return Err("Confirmação explícita obrigatória".into())}
    let mut reg=read_registry()?;
    reg.servers.retain(|s|s.id!=id);
    save_registry(&reg)
}

fn run_bridge(server:&McpServer,action:&str,tool:&str,args:Value)->Result<Value,String>{
    validate_mcp_url(&server.url)?;
    let script=PathBuf::from(ROOT).join("scripts").join("mcp-bridge.cjs");
    if !script.is_file(){return Err("Ponte MCP ausente".into())}
    let body=json!({"url":server.url,"action":action,"tool":tool,"args":args});
    let mut process=Command::new(NODE).arg(script)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|e|format!("Não foi possível iniciar ponte MCP: {e}"))?;
    if let Some(mut stdin)=process.stdin.take(){
        stdin.write_all(body.to_string().as_bytes()).map_err(|e|e.to_string())?;
    }
    let result=process.wait_with_output().map_err(|e|e.to_string())?;
    if result.stdout.len()>100000{return Err("Resposta MCP muito grande".into())}
    let response:Value=serde_json::from_slice(&result.stdout)
        .map_err(|_|"Resposta MCP inválida".to_string())?;
    if response.get("ok").and_then(Value::as_bool)!=Some(true){
        return Err(response.get("error").and_then(Value::as_str)
            .or_else(||response.get("result").and_then(Value::as_str))
            .unwrap_or("MCP retornou erro").to_string())
    }
    Ok(response)
}
fn find_server(id:&str)->Result<McpServer,String>{
    let server=read_registry()?.servers.into_iter().find(|s|s.id==id)
        .ok_or("Servidor MCP não cadastrado")?;
    validate_mcp_url(&server.url)?;
    Ok(server)
}

#[tauri::command]
pub async fn mcp_list_tools(id:String)->Result<Value,String>{
    let server=find_server(&id)?;
    tauri::async_runtime::spawn_blocking(move||
        run_bridge(&server,"list","",json!({}))
    ).await.map_err(|e|format!("Tarefa MCP falhou: {e}"))?
}
#[tauri::command]
pub async fn mcp_call_tool(id:String,tool:String,args:Value,confirmed:bool)->Result<Value,String>{
    if !confirmed{return Err("A execução MCP requer aprovação explícita".into())}
    if tool.len()>120 || tool.is_empty() || !args.is_object() ||
        args.to_string().len()>15000 {return Err("Parâmetros de ferramenta MCP inválidos".into())}
    let server=find_server(&id)?;
    let safe_tool_name=tool.clone();
    let result=tauri::async_runtime::spawn_blocking(move||
        run_bridge(&server,"call",&tool,args)
    ).await.map_err(|e|format!("Tarefa MCP falhou: {e}"))?;
    audit("call",&id,&safe_tool_name,result.is_ok());
    result
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn only_local_servers_accepted(){
        assert!(validate_mcp_url("http://127.0.0.1:4567/mcp").is_ok());
        assert!(validate_mcp_url("http://localhost:8099/mcp").is_ok());
        assert!(validate_mcp_url("https://example.org/mcp").is_err());
        assert!(validate_mcp_url("http://10.0.0.1:9999/mcp").is_err());
        assert!(validate_mcp_url("http://user:pass@localhost:9876/mcp").is_err());
        assert!(validate_mcp_url("file:///mcp").is_err());
    }
}
