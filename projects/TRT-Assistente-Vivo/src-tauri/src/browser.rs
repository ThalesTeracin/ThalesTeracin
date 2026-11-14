use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Write,
    fs::OpenOptions,
    time::{SystemTime, UNIX_EPOCH},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};
use tauri::State;

const PROJECT: &str = r"C:\TRT-Assistente-Vivo-Standalone";
const CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
const NODE: &str = r"C:\Program Files\nodejs\node.exe";

fn isolated_session_running() -> bool {
    let port_file = PathBuf::from(PROJECT)
        .join("data").join("browser-profile").join("DevToolsActivePort");
    let Ok(file) = std::fs::read_to_string(port_file) else { return false };
    let Some(port) = file.lines().next().and_then(|v|v.parse::<u16>().ok()) else { return false };
    let address = SocketAddr::from(([127,0,0,1],port));
    TcpStream::connect_timeout(&address,Duration::from_millis(300)).is_ok()
}

fn record_audit(action: &str, domain: &str, success: bool) {
    let log = PathBuf::from(PROJECT).join("logs").join("browser-actions.jsonl");
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log) else { return };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|v|v.as_secs()).unwrap_or_default();
    let safe_action = match action {
        "navigate" => "navigate", "inspect" => "inspect", "fill" => "fill",
        "start" => "start", "shutdown" => "shutdown", "fill_batch" => "fill_batch",
        "grant" => "grant", "revoke" => "revoke", _ => "other"
    };
    let safe_domain: String = domain.chars().take(120)
        .filter(|c|c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
    let line = json!({"timestamp":timestamp,"action":safe_action,"domain":safe_domain,"success":success});
    let _ = writeln!(file,"{line}");
}

#[derive(Clone)]
struct PermissionGrant {
    origin: String,
    expires_at: u64,
    remaining_fills: u32,
}

pub struct BrowserState {
    child: Mutex<Option<Child>>,
    last_origin: Mutex<Option<String>>,
    grant: Mutex<Option<PermissionGrant>>,
}
impl Default for BrowserState {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            last_origin: Mutex::new(None),
            grant: Mutex::new(None),
        }
    }
}
fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|x|x.as_secs()).unwrap_or_default()
}
fn validated_origin(url: &str) -> Result<String,String> {
    let u = url::Url::parse(url).map_err(|_|"Endereço inválido".to_string())?;
    if !matches!(u.scheme(),"http"|"https") || u.host().is_none()
        || !u.username().is_empty() || u.password().is_some() {
        return Err("Origem HTTP(S) inválida".into());
    }
    Ok(u.origin().ascii_serialization())
}

#[tauri::command]
pub fn browser_grant(state: State<'_, BrowserState>, url: String, allow_fill: bool) -> Result<Value,String> {
    if !isolated_session_running() {return Err("Inicie o Chrome isolado antes de conceder acesso".into())}
    let requested = validated_origin(&url)?;
    let last = state.last_origin.lock().map_err(|_|"Permissões indisponíveis")?;
    if last.as_deref() != Some(requested.as_str()) {
        return Err("Inspecione a página atual antes de autorizar este domínio".into());
    }
    drop(last);
    if !allow_fill {return Err("Acesso de preenchimento não autorizado".into())}
    let grant = PermissionGrant {origin:requested.clone(),expires_at:now_secs()+15*60,remaining_fills:10};
    *state.grant.lock().map_err(|_|"Permissões indisponíveis")? = Some(grant);
    record_audit("grant",&requested,true);
    Ok(json!({"origin":requested,"expiresInSeconds":900,"remainingFills":10}))
}
#[tauri::command]
pub fn browser_revoke(state: State<'_, BrowserState>) -> Result<(),String> {
    *state.grant.lock().map_err(|_|"Permissões indisponíveis")? = None;
    record_audit("revoke","local",true);
    Ok(())
}
#[tauri::command]
pub fn browser_permission_status(state: State<'_, BrowserState>) -> Result<Value,String> {
    let grant = state.grant.lock().map_err(|_|"Permissões indisponíveis")?;
    if let Some(permission)=grant.as_ref() {
        let left=permission.expires_at.saturating_sub(now_secs());
        return Ok(json!({"active":left>0 && permission.remaining_fills>0,
            "origin":permission.origin,"expiresInSeconds":left,
            "remainingFills":permission.remaining_fills}));
    }
    Ok(json!({"active":false,"origin":null,"expiresInSeconds":0,"remainingFills":0}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInput {
    action: String,
    url: Option<String>,
    expected_url: Option<String>,
    selector: Option<String>,
    value: Option<String>,
    confirmed: Option<bool>,
    fields: Option<Vec<serde_json::Value>>,
}

#[tauri::command]
pub fn browser_start(state: State<'_, BrowserState>) -> Result<String, String> {
    if !PathBuf::from(CHROME).is_file() { return Err("Chrome ARM64 não localizado".into()); }
    if !PathBuf::from(NODE).is_file() { return Err("Node.js não localizado".into()); }
    let root = PathBuf::from(PROJECT).join("data").join("browser-profile");
    std::fs::create_dir_all(&root).map_err(|e|format!("Não foi possível criar perfil isolado: {e}"))?;
    let mut child = state.child.lock().map_err(|_|"Controle do navegador indisponível")?;
    if isolated_session_running() {
        return Ok("O Chrome isolado já está aberto. Não foi criado outro perfil.".into());
    }
    *child = None;
    // Evita reutilizar arquivo de porta de sessão anterior.
    let _ = std::fs::remove_file(root.join("DevToolsActivePort"));
    let mut chrome = Command::new(CHROME);
    chrome.arg(format!("--user-data-dir={}",root.display()))
        .arg("--remote-debugging-address=127.0.0.1")
        .arg("--remote-debugging-port=0")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--new-window")
        .arg("about:blank")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    *child = Some(chrome.spawn().map_err(|e|format!("Não foi possível abrir Chrome: {e}"))?);
    for _ in 0..100 {
        if isolated_session_running() {
            record_audit("start","local",true);
            return Ok("Chrome isolado iniciado. O perfil principal não foi acessado.".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("Chrome iniciou, mas o protocolo de automação não ficou pronto".into())
}

// A chat command is itself an explicit user instruction to navigate a public site.
pub async fn open_from_chat(state: State<'_, BrowserState>, url: String) -> Result<Value, String> {
    browser_start(state.clone())?;
    browser_action(state, BrowserInput {
        action: "navigate".to_owned(),
        url: Some(url), expected_url: None, selector: None, value: None,
        confirmed: None, fields: None
    }).await
}

#[tauri::command]
pub fn browser_status() -> Value {
    let root = PathBuf::from(PROJECT).join("data").join("browser-profile");
    json!({
        "running":isolated_session_running(),
        "profile": root.display().to_string(),
        "isolation": "Perfil próprio, sem usar o Chrome pessoal",
        "manualSubmit": true
    })
}

fn parse_action(input: &BrowserInput) -> Result<(), String> {
    match input.action.as_str() {
        "inspect" => Ok(()),
        "navigate" => {
            let url = input.url.as_deref().ok_or("URL obrigatória")?;
            if url.len() > 2048 { return Err("URL muito longa".into()); }
            let parsed = url::Url::parse(url).map_err(|_|"URL inválida")?;
            if !["http","https"].contains(&parsed.scheme()) || parsed.host().is_none()
                || !parsed.username().is_empty() || parsed.password().is_some() {
                return Err("Somente endereços HTTP(S) sem credenciais são permitidos".into());
            }
            Ok(())
        }
        "fill" | "fill_batch" => {
            if input.confirmed != Some(true) { return Err("Confirmação explícita exigida".into()); }
            if input.expected_url.is_none() {
                return Err("Página não verificada".into());
            }
            if input.action=="fill" {
                if input.selector.as_deref().unwrap_or("").len()>180
                    || input.selector.as_deref().unwrap_or("").is_empty()
                    || input.value.as_deref().unwrap_or("").len()>8000 {
                    return Err("Campo ou conteúdo inválido".into());
                }
            } else {
                let fields = input.fields.as_ref().ok_or("Lista de campos não fornecida")?;
                if fields.is_empty() || fields.len()>20 {return Err("Máximo de 20 campos por lote".into())}
                for field in fields {
                    let selector=field.get("selector").and_then(Value::as_str).unwrap_or("");
                    let value=field.get("value").and_then(Value::as_str).unwrap_or("");
                    if selector.is_empty() || selector.len()>180 || value.len()>8000
                        || field.as_object().map(|o|o.len()).unwrap_or(0)!=2 {
                        return Err("Seletor ou valor de campo inválido".into());
                    }
                }
            }
            Ok(())
        }
        _ => Err("Operação de navegador não permitida".into())
    }
}

#[tauri::command]
pub async fn browser_action(
    state: State<'_, BrowserState>, input: BrowserInput
) -> Result<Value, String> {
    parse_action(&input)?;
    if !isolated_session_running() {
        return Err("Inicie o Chrome isolado antes de usar a automação".into());
    }
    let operation = input.action.clone();
    let is_fill = operation=="fill" || operation=="fill_batch";
    let mut allowed_origin: Option<String> = None;
    if is_fill {
        let expected = input.expected_url.as_deref().ok_or("Página não verificada")?;
        let origin = validated_origin(expected)?;
        let mut grant = state.grant.lock().map_err(|_|"Permissões indisponíveis")?;
        match grant.as_mut() {
            Some(permission) if permission.origin==origin &&
                permission.expires_at>now_secs() && permission.remaining_fills>0 => {
                permission.remaining_fills-=1;
                allowed_origin=Some(origin);
            }
            _ => return Err("Preenchimento não autorizado para este site. Conceda permissão temporária.".into())
        }
    }
    let payload = json!({
        "action":input.action, "url":input.url, "expectedUrl":input.expected_url,
        "selector":input.selector, "value":input.value, "confirmed":input.confirmed,
        "allowedOrigin":allowed_origin, "fields":input.fields
    });
    let result = tauri::async_runtime::spawn_blocking(move || {
        let script = PathBuf::from(PROJECT).join("scripts").join("isolated-browser.cjs");
        if !script.is_file() { return Err("Script do navegador não encontrado".into()); }
        let mut process = Command::new(NODE)
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn().map_err(|e|format!("Falha ao conectar ao Chrome: {e}"))?;
        if let Some(mut stdin) = process.stdin.take() {
            stdin.write_all(payload.to_string().as_bytes()).map_err(|e|e.to_string())?;
        }
        let output = process.wait_with_output().map_err(|e|e.to_string())?;
        let result: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_|"Resposta do navegador inválida".to_string())?;
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            record_audit(payload["action"].as_str().unwrap_or("other"),"",false);
            return Err(result.get("error").and_then(Value::as_str).unwrap_or("Ação não concluída").to_owned());
        }
        let domain = result.get("url").and_then(Value::as_str)
            .and_then(|u|url::Url::parse(u).ok())
            .and_then(|u|u.host_str().map(str::to_owned))
            .unwrap_or_default();
        record_audit(payload["action"].as_str().unwrap_or("other"),&domain,true);
        Ok(result)
    }).await.map_err(|e|format!("Execução do navegador falhou: {e}"))?;
    let result = result?;
    if !is_fill {
        if let Some(url)=result.get("url").and_then(Value::as_str) {
            let origin=validated_origin(url)?;
            let mut last=state.last_origin.lock().map_err(|_|"Estado do navegador indisponível")?;
            if last.as_deref()!=Some(origin.as_str()) {
                *state.grant.lock().map_err(|_|"Permissões indisponíveis")?=None;
                *last=Some(origin);
            }
        }
    }
    Ok(result)
}

pub async fn inspect_for_task(state: State<'_, BrowserState>) -> Result<Value, String> {
    browser_action(state, BrowserInput {
        action: "inspect".to_owned(),url:None,expected_url:None,
        selector:None,value:None,confirmed:None,fields:None,
    }).await
}

#[tauri::command]
pub async fn browser_stop(state: State<'_, BrowserState>) -> Result<(), String> {
    if !isolated_session_running() { return Ok(()); }
    let script = PathBuf::from(PROJECT).join("scripts").join("isolated-browser.cjs");
    tauri::async_runtime::spawn_blocking(move || {
        let mut process = Command::new(NODE)
            .arg(script)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn().map_err(|e|format!("Falha ao fechar navegador isolado: {e}"))?;
        if let Some(mut stdin) = process.stdin.take() {
            stdin.write_all(br#"{"action":"shutdown","confirmed":true}"#)
                .map_err(|e|e.to_string())?;
        }
        let output = process.wait_with_output().map_err(|e|e.to_string())?;
        let response: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_|"Resposta de encerramento inválida".to_string())?;
        if response.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(response.get("error").and_then(Value::as_str)
                .unwrap_or("O navegador isolado não encerrou").to_owned());
        }
        Ok(())
    }).await.map_err(|e|format!("Falha no encerramento: {e}"))??;
    let mut child = state.child.lock().map_err(|_|"Controle do navegador indisponível")?;
    *child = None;
    *state.grant.lock().map_err(|_|"Permissões indisponíveis")?=None;
    *state.last_origin.lock().map_err(|_|"Estado indisponível")?=None;
    record_audit("shutdown","local",true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_requires_confirmation_and_valid_data() {
        let input = BrowserInput { action:"fill_batch".into(),url:None,
            expected_url:Some("https://example.org/form".into()),selector:None,value:None,
            confirmed:Some(false), 
            fields:Some(vec![json!({"selector":"#name","value":"Teste"})]) };
        assert!(parse_action(&input).is_err());
    }
    #[test]
    fn origin_validation_rejects_credentials() {
        assert!(validated_origin("https://www.example.org/form").is_ok());
        assert!(validated_origin("file:///C:/Windows").is_err());
        assert!(validated_origin("https://admin:pass@example.org").is_err());
    }
    #[test]
    fn navigation_does_not_accept_files_or_scripts() {
        for url in ["file:///C:/Windows", "javascript:alert(1)", "https://user:pass@example.org/"] {
            assert!(parse_action(&BrowserInput {
                action:"navigate".into(),url:Some(url.into()),expected_url:None,
                selector:None,value:None,confirmed:None,fields:None
            }).is_err());
        }
        assert!(parse_action(&BrowserInput{
            action:"navigate".into(),url:Some("https://example.org/".into()),
            expected_url:None,selector:None,value:None,confirmed:None,fields:None
        }).is_ok());
    }
    #[test]
    fn requires_confirm_for_fill() {
        assert!(parse_action(&BrowserInput{
            action:"fill".into(),url:None,expected_url:Some("https://example.org".into()),
            selector:Some("#full-name".into()),value:Some("Teste".into()),confirmed:Some(false),fields:None
        }).is_err());
    }
}
