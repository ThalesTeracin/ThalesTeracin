use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap, fs::OpenOptions, io::Write, sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::{browser, chat_allowed_app, chat_browser_url, chat_diagnostic_intent, chat_form_intent,
    save_exchange, windows, AppState};

const EXPIRY_SECONDS: u64 = 600;
const MAX_STEPS: usize = 5;
const AUDIT: &str = r"C:\TRT-Assistente-Vivo-Standalone\logs\local-task-execution.jsonl";

#[derive(Clone)]
enum TaskAction {
    App(&'static str),
    OpenChrome,
    Navigate(String),
    Diagnose,
    Inspect,
    FormReview,
}

impl TaskAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::App(_) => "app", Self::OpenChrome => "browser_start",
            Self::Navigate(_) => "navigate", Self::Diagnose => "diagnostic",
            Self::Inspect => "inspect", Self::FormReview => "form_review",
        }
    }
    fn label(&self) -> String {
        match self {
            Self::App("notepad") => "Abrir Bloco de Notas".into(),
            Self::App("calculator") => "Abrir Calculadora".into(),
            Self::App("explorer") => "Abrir Explorador de Arquivos".into(),
            Self::App("paint") => "Abrir Paint".into(),
            Self::App("task-manager") => "Abrir Gerenciador de Tarefas".into(),
            Self::App(_) => "Abrir aplicativo autorizado".into(),
            Self::OpenChrome => "Iniciar Chrome isolado".into(),
            Self::Navigate(url) => format!("Navegar para {url} no Chrome isolado"),
            Self::Diagnose => "Executar diagnóstico do Windows (somente leitura)".into(),
            Self::Inspect => "Inspecionar página no Chrome isolado".into(),
            Self::FormReview => "Abrir revisão de formulário (sem preencher ou enviar)".into(),
        }
    }
}

#[derive(Clone)]
struct Draft {
    session_id: String,
    prompt: String,
    created_at: u64,
    steps: Vec<TaskAction>,
}

#[derive(Default)]
pub struct TaskState {
    plans: Mutex<HashMap<String, Draft>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStep {
    index: usize,
    kind: String,
    label: String,
    status: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPreview {
    id: String,
    expires_in_seconds: u64,
    steps: Vec<TaskStep>,
    notice: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReport {
    id: String,
    completed: usize,
    success: bool,
    steps: Vec<TaskStep>,
    summary: String,
    contains_form_review: bool,
}

fn epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|x| x.as_secs()).unwrap_or_default()
}

fn audit(id: &str, event: &str, index: usize, kind: &str, ok: bool) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(AUDIT) {
        let record = json!({"timestamp":epoch(),"taskId":id,"event":event,
            "index":index,"kind":kind,"success":ok});
        let _ = writeln!(f, "{record}");
    }
}

fn parse_segment(input: &str) -> Option<TaskAction> {
    let clause = input.trim().trim_end_matches('.').trim();
    let low = clause.to_lowercase();
    if let Some(app) = chat_allowed_app(clause) {return Some(TaskAction::App(app))}
    if chat_diagnostic_intent(clause) {return Some(TaskAction::Diagnose)}
    if chat_form_intent(clause) {return Some(TaskAction::FormReview)}
    if matches!(low.as_str(),
        "abra chrome" | "abra o chrome" | "inicie chrome" | "inicie o chrome"
        | "acesse o chrome" | "abrir chrome" | "abra navegador" | "inicie navegador")
    {return Some(TaskAction::OpenChrome)}
    if matches!(low.as_str(),
        "inspecione pagina" | "inspecione página" | "inspecione o site"
        | "inspecione a pagina" | "inspecione a página" | "inspecionar pagina")
    {return Some(TaskAction::Inspect)}
    chat_browser_url(clause).map(TaskAction::Navigate)
}

fn parse_plan(input: &str) -> Result<Vec<TaskAction>, String> {
    let s = input.trim();
    if s.is_empty() || s.len() > 700 { return Err("Descreva a tarefa em até 700 caracteres.".into()) }
    // Split only explicit sequential clauses; never pass raw instructions to a shell.
    if s.contains(';') || s.contains("&&") || s.contains('|') || s.contains('\0') {
        return Err("Comandos compostos de shell não são aceitos.".into())
    }
    let expanded = s.replace("\r\n", "\n")
        .replace(" e depois ", "\n").replace(" depois ", "\n")
        .replace(" e ", "\n").replace(" E ", "\n");
    let clauses: Vec<&str> = expanded.split('\n').map(str::trim)
        .filter(|part| !part.is_empty()).collect();
    if clauses.is_empty() || clauses.len() > MAX_STEPS {
        return Err("Use de 1 a 5 etapas explícitas.".into());
    }
    let mut steps = Vec::with_capacity(clauses.len());
    for (i, clause) in clauses.iter().enumerate() {
        match parse_segment(clause) {
            Some(action) => steps.push(action),
            None => return Err(format!(
                "Etapa {} não permitida ou não reconhecida. Use apenas: abrir apps permitidos, navegar, inspecionar, diagnóstico ou revisão de cadastro.",
                i + 1)),
        }
    }
    Ok(steps)
}

#[tauri::command]
pub fn preview_local_task(
    tasks: State<'_, TaskState>, prompt: String, session_id: String,
) -> Result<TaskPreview, String> {
    Uuid::parse_str(&session_id).map_err(|_| "Sessão inválida")?;
    let steps = parse_plan(&prompt)?;
    let id = Uuid::new_v4().to_string();
    let created_at = epoch();
    let draft = Draft { session_id, prompt, created_at, steps: steps.clone() };
    let mut plans = tasks.plans.lock().map_err(|_|"Planos indisponíveis")?;
    plans.retain(|_, p| created_at.saturating_sub(p.created_at) <= EXPIRY_SECONDS);
    if plans.len() >= 10 { return Err("Limite de planos aguardando aprovação atingido.".into()); }
    plans.insert(id.clone(), draft);
    drop(plans);
    audit(&id, "preview", 0, "task", true);
    Ok(TaskPreview {
        id,
        expires_in_seconds: EXPIRY_SECONDS,
        steps: steps.iter().enumerate().map(|(i, a)| TaskStep {
            index:i + 1, kind:a.kind().into(), label:a.label(),
            status:"pending".into(), detail:String::new(),
        }).collect(),
        notice:"Aprovação válida somente para este plano e por 10 minutos. Não executa shell, altera BIOS, preenche dados pessoais ou envia formulários.".into(),
    })
}

#[tauri::command]
pub fn cancel_local_task(tasks: State<'_, TaskState>, id: String) -> Result<(), String> {
    let removed = tasks.plans.lock().map_err(|_|"Planos indisponíveis")?.remove(&id);
    if removed.is_some() { audit(&id, "cancel", 0, "task", true); }
    Ok(())
}

#[tauri::command]
pub async fn execute_local_task(
    app: AppHandle, tasks: State<'_, TaskState>,
    browser_state: State<'_, browser::BrowserState>,
    app_state: State<'_, AppState>,
    id: String, session_id: String, approved: bool,
) -> Result<TaskReport, String> {
    if !approved {return Err("É necessária aprovação explícita para executar o plano.".into());}
    let plan = tasks.plans.lock().map_err(|_|"Planos indisponíveis")?
        .remove(&id).ok_or("Plano inexistente, cancelado ou já executado")?;
    if plan.session_id != session_id {
        audit(&id, "invalid_session", 0, "task", false);
        return Err("Plano associado a outra conversa. Prepare um novo plano.".into());
    }
    if epoch().saturating_sub(plan.created_at) > EXPIRY_SECONDS {
        audit(&id, "expired", 0, "task", false);
        return Err("Plano expirou. Gere uma nova prévia antes de executar.".into());
    }
    audit(&id, "approved", 0, "task", true);
    let mut results = Vec::new();
    for (i, action) in plan.steps.iter().enumerate() {
        let step = i + 1;
        let _ = app.emit("trt:task-progress",
            json!({"taskId":id,"index":step,"label":action.label(),"status":"running"}));
        let outcome: Result<String, String> = match action {
            TaskAction::App(app_id) => windows::launch_windows_app((*app_id).to_owned())
                .map(|()| format!("Aplicativo {app_id} iniciado")),
            TaskAction::OpenChrome => browser::browser_start(browser_state.clone()),
            TaskAction::Navigate(url) => {
                browser::open_from_chat(browser_state.clone(), url.clone()).await
                    .map(|data| format!(
                        "Página: {} | URL: {} | HTTP {}",
                        data["title"].as_str().unwrap_or("sem título").chars().take(90).collect::<String>(),
                        data["url"].as_str().unwrap_or("indisponível"),
                        data["httpStatus"].as_u64().map(|n|n.to_string()).unwrap_or_else(||"?".into())
                    ))
            }
            TaskAction::Diagnose => {
                let d = windows::diagnose_windows();
                Ok(format!(
                    "CPU: {} | RAM disponível: {} de {} MB | BIOS: {} | Somente leitura",
                    d["cpu"].as_str().unwrap_or("-"),
                    d["ramFreeMb"].as_u64().unwrap_or_default(),
                    d["ramTotalMb"].as_u64().unwrap_or_default(),
                    d["biosVersion"].as_str().unwrap_or("-")
                ))
            }
            TaskAction::Inspect => {
                if let Err(e) = browser::browser_start(browser_state.clone()) {
                    Err(e)
                } else {
                    browser::inspect_for_task(browser_state.clone()).await.map(|v| format!(
                        "Página: {} | {} formulário(s) encontrado(s)",
                        v["url"].as_str().unwrap_or("-"),
                        v["forms"].as_array().map(Vec::len).unwrap_or(0)
                    ))
                }
            }
            TaskAction::FormReview => Ok(
                "Abra Automação Windows → Navegação e cadastros, inspecione a página, conceda autorização ao domínio e revise cada campo. Nenhuma informação foi enviada.".into()
            )
        };
        let ok = outcome.is_ok();
        let result = TaskStep {
            index:step, kind:action.kind().into(),label:action.label(),
            status:if ok {"completed"} else {"failed"}.into(),
            detail:outcome.unwrap_or_else(|e|format!("Falha: {e}")),
        };
        audit(&id, "step", step, action.kind(), ok);
        let _ = app.emit("trt:task-progress",json!({
            "taskId":id,"index":step,"label":result.label,
            "status":result.status,"detail":result.detail
        }));
        results.push(result);
        if !ok {break;} // Fail closed: no later step after an error.
    }
    let completed = results.iter().filter(|r|r.status=="completed").count();
    let success = completed==plan.steps.len();
    let summary = format!(
        "## Tarefa supervisionada {}\n\n{}",
        if success {"concluída"} else {"interrompida"},
        results.iter().map(|r|format!("{}. **{}** — {}: {}",
            r.index,r.label, if r.status=="completed" {"OK"} else {"falhou"},
            r.detail)).collect::<Vec<_>>().join("\n\n")
    );
    audit(&id, "finished", completed, "task", success);
    // Actions already performed must remain visible even if local history cannot be written.
    let _ = save_exchange(&app_state, &session_id, &plan.prompt, &summary, "Tarefas supervisionadas");
    Ok(TaskReport {
        id,completed,success,steps:results,summary,
        contains_form_review:plan.steps.iter().any(|step|matches!(step,TaskAction::FormReview))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_multi_step_parsing() {
        let steps = parse_plan("abra calculadora e faça diagnóstico").unwrap();
        assert_eq!(steps.len(),2);
        assert_eq!(steps[0].kind(),"app");
        assert_eq!(steps[1].kind(),"diagnostic");
        let steps=parse_plan("abra o chrome e visite a pagina ufc.com e inspecione a página").unwrap();
        assert_eq!(steps.len(),3);
        assert_eq!(steps[1].kind(),"navigate");
        let steps=parse_plan("preencha cadastro").unwrap();
        assert_eq!(steps[0].kind(),"form_review");
    }
    #[test]
    fn forbid_unsupported_or_dangerous_instructions() {
        for s in [
            "rode powershell", "desative o antivirus",
            "abra calculadora e apague arquivos",
            "execute cmd.exe /c del C:",
            "abra calculadora; del *",
            "altere a bios", "formate o disco",
            "abra https://user:pass@example.com",
            "abra http://192.168.0.1",
            "abra calculadora e execute powershell",
        ] {assert!(parse_plan(s).is_err(), "{s}");}
    }
    #[test]
    fn task_limit() {
        assert!(parse_plan("abra calculadora\nabra paint\nabra bloco de notas\nabra calculadora\nabra paint\nabra bloco de notas").is_err());
        assert!(parse_plan("").is_err());
    }
}
