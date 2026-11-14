use futures_util::StreamExt;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use sysinfo::{ProcessesToUpdate, System};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;
mod windows;
mod providers_sync;
mod nightrouter_dpapi;
mod browser;
mod agent_tools;
mod mcp;
mod task_runner;

const ROOT: &str = r"C:\TRT-Assistente-Vivo-Standalone";

struct AppState {
    db_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Provider {
    id: String,
    name: String,
    base_url: String,
    model: String,
    enabled: bool,
    priority: i32,
    cost_status: String,
    #[serde(default)]
    api_key_saved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredMessage {
    id: String,
    session_id: String,
    role: String,
    content: String,
    provider: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct InputMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    session_id: String,
    prompt: String,
    history: Vec<InputMessage>,
    agent_prompt: Option<String>,
    provider_id: Option<String>,
    auto_fallback: bool,
    free_only: bool,
    #[serde(default)]
    browser_commands: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResult {
    content: String,
    provider: String,
    model: String,
    estimated_tokens: usize,
    switched: bool,
}

fn db(state: &AppState) -> Result<Connection, String> {
    Connection::open(&state.db_path).map_err(|e| format!("Erro no banco local: {e}"))
}

fn init_db(path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS providers (
           id TEXT PRIMARY KEY, name TEXT NOT NULL, base_url TEXT NOT NULL,
           model TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0,
           priority INTEGER NOT NULL DEFAULT 50, cost_status TEXT NOT NULL DEFAULT 'unknown'
         );
         CREATE TABLE IF NOT EXISTS sessions (
           id TEXT PRIMARY KEY, title TEXT NOT NULL,
           created_at TEXT NOT NULL DEFAULT (datetime('now')),
           updated_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE TABLE IF NOT EXISTS messages (
           id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
           role TEXT NOT NULL, content TEXT NOT NULL,
           provider TEXT NOT NULL DEFAULT '',
           created_at TEXT NOT NULL DEFAULT (datetime('now')),
           FOREIGN KEY(session_id) REFERENCES sessions(id)
         );
         CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);",
    ).map_err(|e| e.to_string())?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if count == 0 {
        let defaults = [
            ("freellm", "FreeLLMAPI · Local", "http://127.0.0.1:31415/v1", "deepseek-v4-flash", 1, 10, "free"),
            ("flagship", "FlagshipRouter · Local", "http://127.0.0.1:20128/v1", "gpt-oss-20b", 0, 20, "unknown"),
            ("huggingface", "Hugging Face Router", "https://router.huggingface.co/v1", "openai/gpt-oss-20b", 0, 30, "unknown"),
        ];
        for (id, name, base_url, model, enabled, priority, cost_status) in defaults {
            conn.execute(
                "INSERT INTO providers (id,name,base_url,model,enabled,priority,cost_status) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![id, name, base_url, model, enabled, priority, cost_status],
            ).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn secret_entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("TRT-Assistente-Vivo", id).map_err(|e| format!("Credenciais Windows: {e}"))
}

fn get_secret(id: &str) -> Option<String> {
    if id == "nightrouter" {
        return nightrouter_dpapi::read_nightrouter_key();
    }
    if matches!(id, "dahl-glm" | "dahl-minimax" | "dahl-deepseek") {
        return nightrouter_dpapi::read_dpapi_key("dahl.dpapi");
    }
    secret_entry(id).ok()?.get_password().ok()
}

fn validate_url(input: &str) -> Result<(), String> {
    let u = url::Url::parse(input).map_err(|_| "URL inválida".to_string())?;
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    if u.scheme() != "https" && !(u.scheme() == "http" && (host == "127.0.0.1" || host == "localhost" || host == "[::1]")) {
        return Err("A API deve usar HTTPS; HTTP é permitido apenas para localhost".into());
    }
    if u.username() != "" || u.password().is_some() || u.fragment().is_some() {
        return Err("Remova credenciais e fragmentos da URL".into());
    }
    Ok(())
}

#[tauri::command]
fn list_providers(state: State<'_, AppState>) -> Result<Vec<Provider>, String> {
    let conn = db(&state)?;
    let mut query = conn.prepare("SELECT id,name,base_url,model,enabled,priority,cost_status FROM providers ORDER BY priority,name")
        .map_err(|e| e.to_string())?;
    let rows = query.query_map([], |r| {
        Ok(Provider {
            id: r.get(0)?, name: r.get(1)?, base_url: r.get(2)?, model: r.get(3)?,
            enabled: r.get::<_, i32>(4)? != 0, priority: r.get(5)?,
            cost_status: r.get(6)?, api_key_saved: false,
        })
    }).map_err(|e| e.to_string())?;
    let mut providers = Vec::new();
    for row in rows {
        let mut p = row.map_err(|e| e.to_string())?;
        p.api_key_saved = get_secret(&p.id).is_some();
        providers.push(p);
    }
    Ok(providers)
}

#[tauri::command]
fn upsert_provider(state: State<'_, AppState>, provider: Provider, api_key: Option<String>) -> Result<(), String> {
    if provider.id.trim().is_empty() || provider.name.trim().is_empty() || provider.model.trim().is_empty() {
        return Err("Nome, ID e modelo são obrigatórios".into());
    }
    validate_url(&provider.base_url)?;
    if !["free", "paid", "unknown"].contains(&provider.cost_status.as_str()) {
        return Err("Classificação de custo inválida".into());
    }
    let conn = db(&state)?;
    conn.execute(
        "INSERT INTO providers (id,name,base_url,model,enabled,priority,cost_status) VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name,base_url=excluded.base_url,model=excluded.model,
         enabled=excluded.enabled,priority=excluded.priority,cost_status=excluded.cost_status",
        params![provider.id,provider.name,provider.base_url,provider.model,
            provider.enabled as i32,provider.priority,provider.cost_status],
    ).map_err(|e| e.to_string())?;
    if let Some(value) = api_key {
        if !value.trim().is_empty() {
            secret_entry(&provider.id)?.set_password(value.trim())
                .map_err(|e| format!("Não foi possível proteger a chave: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn delete_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = db(&state)?;
    conn.execute("DELETE FROM providers WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
    if let Ok(entry) = secret_entry(&id) { let _ = entry.delete_credential(); }
    Ok(())
}

#[tauri::command]
async fn test_provider(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let provider = list_providers(state)?.into_iter().find(|p| p.id == id)
        .ok_or("Provedor não encontrado")?;
    validate_url(&provider.base_url)?;
    let url = format!("{}/models", provider.base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(Duration::from_secs(12)).build().map_err(|e| e.to_string())?;
    let mut req = client.get(url);
    if let Some(key) = get_secret(&provider.id) { req = req.bearer_auth(key); }
    let res = req.send().await.map_err(|e| format!("Sem conexão: {e}"))?;
    let code = res.status();
    if code.is_success() { Ok("API respondeu corretamente à consulta de modelos".into()) }
    else { Err(format!("API respondeu HTTP {code}; consulte endpoint, autenticação e suporte a /models")) }
}

#[tauri::command]
async fn fetch_provider_models(state: State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    let p = list_providers(state)?.into_iter().find(|p| p.id == id)
        .ok_or("Provedor não encontrado")?;
    validate_url(&p.base_url)?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15))
        .build().map_err(|e|e.to_string())?;
    let mut req = client.get(format!("{}/models",p.base_url.trim_end_matches('/')));
    if let Some(key) = get_secret(&p.id) { req = req.bearer_auth(key); }
    let response = req.send().await.map_err(|e|format!("Consulta de modelos falhou: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Consulta de modelos: HTTP {}",response.status()));
    }
    let data: Value = response.json().await.map_err(|_|"Resposta de modelos não é JSON válido")?;
    let mut models = Vec::new();
    let items = data.get("data").or_else(||data.get("models"));
    if let Some(items) = items.and_then(Value::as_array) {
        for item in items {
            if let Some(name) = item.get("id").or_else(||item.get("name")).and_then(Value::as_str) {
                if !name.is_empty() && name.len() <= 180 {
                    models.push(name.to_owned());
                }
            }
        }
    } else if let Some(map) = items.and_then(Value::as_object) {
        models.extend(map.keys().filter(|m|m.len() <= 180).cloned());
    }
    models.sort();
    models.dedup();
    models.truncate(200);
    if models.is_empty() { Err("A API respondeu sem uma lista reconhecível de modelos".into()) }
    else { Ok(models) }
}

#[tauri::command]
async fn test_chat_provider(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let p = list_providers(state)?.into_iter().find(|p| p.id == id)
        .ok_or("Provedor não encontrado")?;
    if p.cost_status != "free" {
        return Err("Teste de chat restrito a provedores marcados gratuitos. Verifique os custos antes.".into());
    }
    validate_url(&p.base_url)?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(30))
        .build().map_err(|e|e.to_string())?;
    let mut req = client.post(format!("{}/chat/completions",p.base_url.trim_end_matches('/')))
        .json(&json!({
            "model":p.model,
            "messages":[{"role":"user","content":"Responda apenas OK"}],
            "max_tokens":16,
            "stream":true
        }));
    if let Some(key) = get_secret(&p.id) { req = req.bearer_auth(key); }
    let response = req.send().await.map_err(|e|format!("Teste de chat falhou: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("O modelo {} não respondeu: HTTP {}",p.model,response.status()));
    }
    let mut stream = response.bytes_stream();
    let mut bytes_seen = 0usize;
    let mut first_data = Vec::<u8>::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e|format!("Stream interrompido: {e}"))?;
        bytes_seen += bytes.len();
        if bytes_seen > 100_000 { return Err("Stream muito grande para um teste de 16 tokens".into()); }
        first_data.extend_from_slice(&bytes);
        if first_data.windows(b"data: ".len()).any(|w|w == b"data: ") {
            let snippet = String::from_utf8_lossy(&first_data);
            if snippet.contains("\"delta\"") || snippet.contains("\"choices\"") {
                return Ok(format!("Chat SSE validado: {} / {}",p.name,p.model));
            }
        }
    }
    Err("A API retornou HTTP 200, mas não enviou eventos SSE compatíveis".into())
}

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    let conn = db(&state)?;
    let mut query = conn.prepare("SELECT id,title,created_at,updated_at FROM sessions ORDER BY updated_at DESC LIMIT 200")
        .map_err(|e| e.to_string())?;
    let rows = query.query_map([], |r| Ok(Session { id:r.get(0)?, title:r.get(1)?,
        created_at:r.get(2)?, updated_at:r.get(3)? })).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

#[tauri::command]
fn get_messages(state: State<'_, AppState>, session_id: String) -> Result<Vec<StoredMessage>, String> {
    let conn = db(&state)?;
    let mut query = conn.prepare("SELECT id,session_id,role,content,provider,created_at FROM messages WHERE session_id=?1 ORDER BY created_at,rowid")
        .map_err(|e| e.to_string())?;
    let rows = query.query_map(params![session_id], |r| Ok(StoredMessage {
        id:r.get(0)?, session_id:r.get(1)?, role:r.get(2)?,
        content:r.get(3)?, provider:r.get(4)?, created_at:r.get(5)?
    })).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let conn = db(&state)?;
    conn.execute("DELETE FROM messages WHERE session_id=?1", params![session_id]).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sessions WHERE id=?1", params![session_id]).map_err(|e| e.to_string())?;
    Ok(())
}

fn save_exchange(state: &AppState, session_id: &str, prompt: &str, answer: &str, provider: &str) -> Result<(), String> {
    let mut conn = db(state)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let title: String = prompt.chars().take(65).collect();
    tx.execute("INSERT OR IGNORE INTO sessions(id,title) VALUES (?1,?2)", params![session_id,title])
        .map_err(|e| e.to_string())?;
    for (role, text, source) in [("user",prompt,""),("assistant",answer,provider)] {
        tx.execute("INSERT INTO messages (id,session_id,role,content,provider) VALUES (?1,?2,?3,?4,?5)",
            params![Uuid::new_v4().to_string(),session_id,role,text,source]).map_err(|e| e.to_string())?;
    }
    tx.execute("UPDATE sessions SET updated_at=datetime('now') WHERE id=?1", params![session_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

// Coarena routes are opt-in: never auto-select or silently fall back to Coarena.
fn is_allowed_by_manual_selection(provider_id: &str, selected_id: Option<&str>) -> bool {
    !provider_id.starts_with("coarena-") || selected_id == Some(provider_id)
}


// Only direct, explicit navigation requests. Never infer navigation from an AI answer.
// The input must name one ordinary public DNS domain or HTTPS URL.
fn chat_browser_url(prompt: &str) -> Option<String> {
    let s = prompt.trim();
    if s.len() > 600 || s.contains('\n') || s.contains('\r') { return None; }
    let lower = s.to_lowercase();
    let mut rest = lower.trim();
    if let Some(suffix) = rest.strip_prefix("por favor, ") { rest = suffix; }
    else if let Some(suffix) = rest.strip_prefix("por favor ") { rest = suffix; }
    const STARTS: [&str; 14] = [
        "acesse ", "acessa ", "abra ", "abrir ", "visite ", "visitar ",
        "navegue ", "navegar ", "entre ", "vá ", "va ", "ir ",
        "inicie ", "abre "
    ];
    if !STARTS.iter().any(|verb| rest.starts_with(verb)) { return None; }
    if rest.contains("como ") || rest.contains("explique ") || rest.contains("pesquise ") {
        return None;
    }
    let mut result: Option<String> = None;
    for raw in s.split_whitespace() {
        let piece = raw.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '(' | ')' | ',' | ';' | '!' | '.' | ':' ));
        if piece.is_empty() { continue; }
        let absolute = piece.starts_with("https://") || piece.starts_with("http://");
        let hostlike = piece.starts_with("www.") || (piece.contains('.') && !piece.contains('@'));
        if !absolute && !hostlike { continue; }
        let candidate = if absolute { piece.to_owned() } else { format!("https://{piece}") };
        let Ok(url) = url::Url::parse(&candidate) else { continue };
        if !matches!(url.scheme(), "http" | "https") { continue; }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return None;
        }
        let Some(host) = url.host_str() else { return None };
        if !host.contains('.') || host.parse::<std::net::IpAddr>().is_ok()
            || host.eq_ignore_ascii_case("localhost")
            || host.ends_with(".local") || host.ends_with(".internal")
            || host.ends_with(".test") || host.ends_with(".invalid") { return None; }
        let Some(suffix) = host.rsplit('.').next() else { return None };
        if suffix.len() < 2 || suffix.len() > 24 ||
           !suffix.chars().all(|c|c.is_ascii_alphabetic()) { return None; }
        if !host.split('.').all(|label| !label.is_empty() && label.len() <= 63
            && !label.starts_with('-') && !label.ends_with('-')
            && label.chars().all(|c|c.is_ascii_alphanumeric() || c=='-')) {
            return None;
        }
        if result.is_some() { return None; }
        result = Some(url.to_string());
    }
    result
}


// Finite, audited allowlist of harmless application launches.
// No arbitrary command, shell argument, file path or executable supplied by an LLM.
fn chat_allowed_app(text: &str) -> Option<&'static str> {
    let s=text.trim().to_lowercase();
    if s.len()>180 || s.contains('\n') || s.contains(';') || s.contains("&&") {return None}
    let direct=["abra ","abrir ","inicie ","iniciar ","execute ","executar ","roda ","rodar "];
    let s=s.strip_prefix("por favor, ").unwrap_or(&s);
    if !direct.iter().any(|v|s.starts_with(v)) {return None}
    let apps=[
        ("bloco de notas","notepad"),("notepad","notepad"),
        ("calculadora","calculator"),("calc","calculator"),
        ("explorador de arquivos","explorer"),("explorer","explorer"),
        ("paint","paint"),("mspaint","paint"),
        ("gerenciador de tarefas","task-manager"),("task manager","task-manager"),
    ];
    // Disallow extra side effects or multiple actions in same instruction.
    if s.contains(" e ") || s.contains(" depois ") || s.contains("apague") || s.contains("delete") {
        return None;
    }
    for (name,id) in apps {
        if direct.iter().any(|verb|s==format!("{verb}{name}") || s==format!("{verb}o {name}")
            || s==format!("{verb}a {name}")) {return Some(id)}
    }
    None
}
fn chat_diagnostic_intent(text:&str) -> bool {
    let s=text.trim().to_lowercase();
    if s.len()>180 || s.contains('\n') {return false}
    ["diagnostico do windows","diagnóstico do windows","diagnostique o windows",
     "faça diagnostico","faça diagnóstico","faca diagnostico",
     "diagnostico do pc","diagnóstico do pc","verifique o desempenho do pc",
     "qual minha versao da bios","qual a versão da bios","verifique a bios",
     "verifique memoria ram","verifique memória ram"].iter()
        .any(|word|s==*word || s==format!("por favor, {word}"))
}
fn chat_form_intent(text:&str) -> bool {
    let s=text.trim().to_lowercase();
    s.len()<400 && ["preencha formulario","preencha formulário","preencher formulario",
        "preencher formulário","preencha cadastro","atualize cadastro","atualizar cadastro",
        "preencha os campos","preencha esse formulario","preencha este formulário"].iter()
        .any(|verb|s.starts_with(verb))
}

#[tauri::command]
async fn send_chat(app: tauri::AppHandle, state: State<'_, AppState>, browser_state: State<'_, browser::BrowserState>, request: ChatRequest) -> Result<ChatResult, String> {
    if request.prompt.trim().is_empty() { return Err("Digite uma mensagem.".into()); }
    if request.prompt.len() > 250_000 { return Err("Mensagem muito longa.".into()); }

    if request.browser_commands {
        if let Some(target_url) = chat_browser_url(&request.prompt) {
            let page = browser::open_from_chat(browser_state, target_url).await?;
            let actual = page.get("url").and_then(Value::as_str)
                .ok_or("Chrome não retornou a URL da página")?;
            let title = page.get("title").and_then(Value::as_str).unwrap_or("Site aberto");
            let status = page.get("httpStatus").and_then(Value::as_u64);
            let content = match status {
                Some(code) if code >= 400 => format!(
                    "Abri o Chrome isolado e acessei {actual}, mas o site respondeu HTTP {code}. Verifique a página no navegador."
                ),
                _ => format!(
                    "✅ **Chrome aberto e navegação executada.**\n\n**Página:** {}\n\n**Endereço:** {}\n\nO site está no Chrome isolado da Central TRT-Info. Nenhum formulário foi preenchido ou enviado.",
                    title.replace(['[', ']'], ""), actual
                ),
            };
            save_exchange(&state, &request.session_id, &request.prompt, &content, "Chrome isolado")?;
            return Ok(ChatResult {
                content, provider:"Chrome isolado".to_owned(), model:"Navegação local".to_owned(),
                estimated_tokens:0, switched:false
            });
        }
    }

    if let Some(id)=chat_allowed_app(&request.prompt) {
        windows::launch_windows_app(id.to_owned())?;
        let content=format!("✅ Aplicativo **{id}** iniciado pelo Windows. Nenhum comando de shell arbitrário foi executado.");
        save_exchange(&state,&request.session_id,&request.prompt,&content,"Windows local")?;
        return Ok(ChatResult{content,provider:"Windows local".into(),model:"Aplicativo autorizado".into(),estimated_tokens:0,switched:false});
    }
    if chat_diagnostic_intent(&request.prompt) {
        let report=windows::diagnose_windows();
        let ram=report["ramTotalMb"].as_u64().unwrap_or_default();
        let free=report["ramFreeMb"].as_u64().unwrap_or_default();
        let cpu=report["cpu"].as_str().unwrap_or("indisponível");
        let bios=report["biosVersion"].as_str().unwrap_or("indisponível");
        let arch=report["architecture"].as_str().unwrap_or("-");
        let disks=report["drives"].as_array().cloned().unwrap_or_default()
            .iter().map(|d| format!("{}: {} GB livres de {} GB",
                d["drive"].as_str().unwrap_or("Disco"),
                d["freeGb"].as_u64().unwrap_or_default(),
                d["totalGb"].as_u64().unwrap_or_default()))
            .collect::<Vec<_>>().join("\n");
        let content=format!("## Diagnóstico local do Windows\n\n**CPU:** {cpu}\n\n**Arquitetura:** {arch}\n\n**RAM:** {free} MB disponíveis de {ram} MB\n\n**BIOS (versão):** {bios}\n\n**Armazenamento:**\n{disks}\n\nSomente leitura: não alterei serviços, drivers ou BIOS.");
        save_exchange(&state,&request.session_id,&request.prompt,&content,"Diagnóstico local")?;
        return Ok(ChatResult{content,provider:"Diagnóstico local".into(),model:"Windows read-only".into(),estimated_tokens:0,switched:false});
    }
    if chat_form_intent(&request.prompt) {
        let content="✅ Posso preparar e preencher campos comuns de cadastro. Abra a seção **Automação Windows**, use **Inspecionar** no site já aberto, conceda a autorização temporária para o domínio e revise os dados em **Preparar cadastro**. Depois clique em **Revisar e preencher cadastro**. O Chrome preencherá os campos permitidos, mas **não enviará** o formulário automaticamente. Senhas, CPF, documentos, autenticação e pagamentos permanecem manuais.".to_owned();
        save_exchange(&state,&request.session_id,&request.prompt,&content,"Formulários assistidos")?;
        return Ok(ChatResult{content,provider:"Formulários assistidos".into(),model:"Navegador supervisionado".into(),estimated_tokens:0,switched:false});
    }

    let providers = list_providers(state.clone())?;
    let mut candidates: Vec<Provider> = providers.into_iter()
        .filter(|p| p.enabled && (!request.free_only || p.cost_status == "free")
            && is_allowed_by_manual_selection(&p.id, request.provider_id.as_deref()))
        .collect();
    if let Some(ref id) = request.provider_id {
        candidates.sort_by_key(|p| if &p.id == id { -1000 } else { p.priority });
        if !request.auto_fallback { candidates.retain(|p| &p.id == id); }
    }
    if candidates.is_empty() {
        return Err("Nenhum provedor habilitado atende às condições. Abra Central de Provedores.".into());
    }
    if !request.auto_fallback { candidates.truncate(1); }
    // Identidade da marca definida pelo titular: nunca deduzir que TRT e um tribunal.
    const TRT_IDENTITY: &str = "IDENTIDADE DA CENTRAL: TRT significa exclusivamente Thales Renato Teracin, nome do proprietario e criador do Centro de Comando TRT-Info GPT. TRT-Info e o assistente pessoal de tecnologia e produtividade de Thales Renato Teracin. Nao interprete TRT como Tribunal Regional do Trabalho, nem atribua a Central funcoes de tribunais, processos trabalhistas ou vinculos institucionais. So fale sobre um tribunal quando o usuario solicitar explicitamente esse assunto; nesse caso, distinga o assunto externo do nome da Central. Esta regra vale para todos os agentes e modelos.";
    let mut messages: Vec<Value> = vec![json!({"role":"system","content":TRT_IDENTITY})];
    if let Some(ref agent_prompt) = request.agent_prompt {
        if !agent_prompt.trim().is_empty() { messages.push(json!({"role":"system","content":agent_prompt})); }
    }
    for msg in request.history.iter().rev().take(24).rev() {
        if ["user", "assistant", "system"].contains(&msg.role.as_str()) && msg.content.len() <= 80_000 {
            messages.push(json!({"role":msg.role,"content":msg.content}));
        }
    }
    messages.push(json!({"role":"user","content":request.prompt}));

    let client = reqwest::Client::builder().timeout(Duration::from_secs(120))
        .build().map_err(|e| e.to_string())?;
    let mut failures: Vec<String> = Vec::new();
    for (index, p) in candidates.iter().enumerate() {
        if let Err(e) = validate_url(&p.base_url) { failures.push(format!("{}: {}",p.name,e)); continue; }
        if index > 0 {
            let _ = app.emit("trt:switch", json!({
                "sessionId":request.session_id,"provider":p.name,"model":p.model,
                "reason":failures.last().cloned().unwrap_or_default()
            }));
        }
        let mut req = client.post(format!("{}/chat/completions",p.base_url.trim_end_matches('/')))
            .json(&json!({"model":p.model,"messages":messages,"stream":true}));
        if let Some(key) = get_secret(&p.id) { req = req.bearer_auth(key); }
        let response = match req.send().await {
            Ok(response) => response,
            Err(e) => { failures.push(format!("{}: {}",p.name,e)); continue; }
        };
        if !response.status().is_success() {
            let code = response.status();
            failures.push(format!("{}: HTTP {}",p.name,code));
            continue;
        }
        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut answer = String::new();
        let mut stream_error: Option<String> = None;
        while let Some(next) = stream.next().await {
            match next {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|c| *c == b'\n') {
                        let line: Vec<u8> = buffer.drain(..=pos).collect();
                        let raw = String::from_utf8_lossy(&line);
                        let raw = raw.trim();
                        if let Some(payload) = raw.strip_prefix("data:") {
                            let payload = payload.trim();
                            if payload == "[DONE]" { continue; }
                            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                                if let Some(token) = value.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                                    if !token.is_empty() {
                                        answer.push_str(token);
                                        let _ = app.emit("trt:chunk",json!({"sessionId":request.session_id,"chunk":token}));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => { stream_error = Some(e.to_string()); break; }
            }
        }
        if let Some(e) = stream_error {
            if !answer.is_empty() {
                return Err(format!("A resposta foi interrompida após iniciar a transmissão em {}: {}. Não houve retry para evitar duplicar texto.",p.name,e));
            }
            failures.push(format!("{}: {}",p.name,e));
            continue;
        }
        if answer.is_empty() {
            failures.push(format!("{}: resposta vazia ou streaming incompatível",p.name));
            continue;
        }
        save_exchange(&state, &request.session_id, &request.prompt, &answer, &p.name)?;
        let estimated_tokens = (request.prompt.chars().count() + answer.chars().count()) / 4;
        return Ok(ChatResult {
            content: answer, provider:p.name.clone(), model:p.model.clone(),
            estimated_tokens, switched:index > 0
        });
    }
    Err(format!("Nenhum provedor respondeu. {}",failures.join(" | ")))
}

#[tauri::command]
fn system_snapshot() -> Value {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut processes: Vec<_> = sys.processes().values()
        .map(|p| json!({"name":p.name().to_string_lossy(),"ramMb":p.memory()/1024/1024,"pid":p.pid().to_string()}))
        .collect();
    processes.sort_by(|a,b| b["ramMb"].as_u64().cmp(&a["ramMb"].as_u64()));
    json!({
        "memoryTotalMb":sys.total_memory()/1024/1024,
        "memoryUsedMb":sys.used_memory()/1024/1024,
        "processCount":sys.processes().len(),
        "topProcesses":processes.into_iter().take(8).collect::<Vec<_>>(),
        "arch":std::env::consts::ARCH
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_windows_actions_are_allowlisted() {
        assert_eq!(chat_allowed_app("abra calculadora"),Some("calculator"));
        assert_eq!(chat_allowed_app("abra o bloco de notas"),Some("notepad"));
        assert_eq!(chat_allowed_app("execute Paint"),Some("paint"));
        assert!(chat_allowed_app("execute powershell").is_none());
        assert!(chat_allowed_app("execute calc && del C:").is_none());
        assert!(chat_allowed_app("abra bloco de notas e execute script").is_none());
        assert!(chat_diagnostic_intent("faça diagnóstico"));
        assert!(chat_diagnostic_intent("qual a versão da bios"));
        assert!(!chat_diagnostic_intent("altere a bios"));
        assert!(chat_form_intent("preencha formulário neste site"));
        assert!(!chat_form_intent("apague os arquivos"));
    }
    #[test]
    fn chat_browser_commands_are_explicit_and_public_only() {
        assert_eq!(chat_browser_url("acesse o chrome e visite a pagina ufc.com").as_deref(),Some("https://ufc.com/"));
        assert_eq!(chat_browser_url("Abra o site https://www.ufc.com/events").as_deref(),Some("https://www.ufc.com/events"));
        assert_eq!(chat_browser_url("Por favor, visite www.wikipedia.org").as_deref(),Some("https://www.wikipedia.org/"));
        assert!(chat_browser_url("explique como visitar ufc.com").is_none());
        assert!(chat_browser_url("acesse http://localhost:9222/").is_none());
        assert!(chat_browser_url("abra http://192.168.0.1/").is_none());
        assert!(chat_browser_url("acesse file:///C:/Users").is_none());
        assert!(chat_browser_url("acesse ufc.com e wikipedia.org").is_none());
        assert!(chat_browser_url("acesse https://user:pass@ufc.com/").is_none());
    }
    #[test]
    fn coarena_requires_explicit_selection() {
        assert!(!is_allowed_by_manual_selection("coarena-gemini-4-argon", None));
        assert!(!is_allowed_by_manual_selection("coarena-gemini-4-argon", Some("dahl-glm")));
        assert!(!is_allowed_by_manual_selection("coarena-gemini-4-carbon", Some("coarena-gemini-4-argon")));
        assert!(is_allowed_by_manual_selection("coarena-gemini-4-argon", Some("coarena-gemini-4-argon")));
        assert!(is_allowed_by_manual_selection("dahl-glm", None));
    }
    #[test]
    fn only_https_or_loopback_http() {
        assert!(validate_url("https://router.huggingface.co/v1").is_ok());
        assert!(validate_url("http://127.0.0.1:31415/v1").is_ok());
        assert!(validate_url("http://example.com/v1").is_err());
        assert!(validate_url("https://bob:password@example.com").is_err());
    }
}


#[tauri::command]
fn set_floating_mode(window: tauri::WebviewWindow, floating: bool) -> Result<(), String> {
    use tauri::{LogicalSize, Position, Size};
    if floating {
        window.set_min_size(None::<Size>).map_err(|e| e.to_string())?;
        window.set_decorations(false).map_err(|e| e.to_string())?;
        window.set_resizable(false).map_err(|e| e.to_string())?;
        window.set_size(LogicalSize::new(410.0, 630.0)).map_err(|e| e.to_string())?;
        window.set_always_on_top(true).map_err(|e| e.to_string())?;
        window.center().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        window.set_always_on_top(false).map_err(|e| e.to_string())?;
        window.set_decorations(true).map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;
        window.set_min_size(Some(Size::Logical(LogicalSize::new(1020.0, 690.0)))).map_err(|e| e.to_string())?;
        window.set_size(LogicalSize::new(1460.0, 920.0)).map_err(|e| e.to_string())?;
        window.center().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn drag_companion_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = PathBuf::from(ROOT).join("data").join("database").join("trt-info.sqlite");
    if let Err(e) = init_db(&db_path) {
        eprintln!("Erro de inicialização do banco: {e}");
    }
    tauri::Builder::default()
        .manage(AppState { db_path })
        .manage(browser::BrowserState::default())
        .manage(task_runner::TaskState::default())
        .manage(mcp::BuiltInMcpState::default())
        .invoke_handler(tauri::generate_handler![
            set_floating_mode, drag_companion_window,
            task_runner::preview_local_task, task_runner::cancel_local_task, task_runner::execute_local_task,
            list_providers, upsert_provider, delete_provider, test_provider, fetch_provider_models, test_chat_provider,
            list_sessions, get_messages, delete_session, send_chat, system_snapshot,
            windows::installed_tools, windows::inspect_windows, windows::diagnose_windows, windows::tool_location, windows::open_project_folder, windows::launch_windows_app, windows::launch_cli_terminal,
            providers_sync::import_from_opencode,
            browser::browser_start, browser::browser_status, browser::browser_action, browser::browser_stop,browser::browser_grant,browser::browser_revoke,browser::browser_permission_status,
            agent_tools::project_list, agent_tools::project_read, agent_tools::project_write,
            agent_tools::tool_audit_history, agent_tools::project_search,
            mcp::mcp_servers, mcp::mcp_upsert_server, mcp::mcp_remove_server,
            mcp::mcp_list_tools, mcp::mcp_call_tool,mcp::mcp_builtin_status, mcp::mcp_start_builtin, mcp::mcp_stop_builtin
        ])
        .setup(|app| {
            let _ = app.path().app_data_dir();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Erro ao iniciar TRT-Info GPT");
}
