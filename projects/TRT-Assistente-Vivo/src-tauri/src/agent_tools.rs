use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const ROOT: &str = r"C:\TRT-Assistente-Vivo-Standalone";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolFile {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub action: String,
    pub path: String,
    pub detail: String,
    pub content: Option<String>,
    pub backup: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequest {
    pub action: String,
    pub path: Option<String>,
    pub content: Option<String>,
    pub confirmed: Option<bool>,
}

fn project_root() -> PathBuf { PathBuf::from(ROOT) }

fn normalize_project_path(relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.len() > 300 || relative.contains('\0') {
        return Err("Caminho do projeto inválido".into());
    }
    let value = Path::new(relative);
    if value.is_absolute() { return Err("Caminhos absolutos não são permitidos".into()); }
    let mut normalized = PathBuf::new();
    for component in value.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy().contains(':') { return Err("Caminho inválido".into()); }
                normalized.push(part)
            },
            Component::CurDir => (),
            _ => return Err("Navegação fora do projeto bloqueada".into())
        }
    }
    if normalized.as_os_str().is_empty() { return Err("Informe um arquivo do projeto".into()); }
    Ok(normalized)
}

fn safe_project_file(relative: &str, require_exists: bool) -> Result<PathBuf,String> {
    let normalized = normalize_project_path(relative)?;
    let target = project_root().join(normalized);
    let base = fs::canonicalize(project_root()).map_err(|e|e.to_string())?;
    let current = if require_exists {
        fs::canonicalize(&target).map_err(|_|"Arquivo não encontrado".to_string())?
    } else {
        let parent = target.parent().ok_or("Sem diretório pai")?;
        fs::canonicalize(parent).map_err(|_|"Diretório pai deve existir".to_string())?
    };
    if !current.starts_with(&base) { return Err("Acesso fora do projeto bloqueado".into()); }
    Ok(target)
}

fn writable_area(relative: &str) -> bool {
    matches!(relative.split(['/', '\\']).next().unwrap_or(""), "docs" | "data")
        && !relative.to_ascii_lowercase().starts_with("data\\database")
        && !relative.to_ascii_lowercase().starts_with("data/database")
        && !relative.to_ascii_lowercase().starts_with("data\\browser-profile")
        && !relative.to_ascii_lowercase().starts_with("data/browser-profile")
}

fn disallow_sensitive(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    lower.ends_with(".env") || lower.contains(".env.")
        || lower.ends_with(".key") || lower.ends_with(".pem")
        || lower.ends_with(".pfx") || lower.ends_with(".p12")
        || lower.ends_with(".sqlite") || lower.ends_with(".db")
        || lower.ends_with(".exe") || lower.ends_with(".dll")
        || lower.ends_with(".ps1") || lower.ends_with(".bat")
        || lower.ends_with(".cmd") || lower.ends_with(".js")
        || lower.ends_with(".cjs") || lower.ends_with(".ts")
        || lower.ends_with(".tsx") || lower.ends_with(".json")
        || lower.ends_with(".html") || lower.ends_with(".py")
}

fn audit(action: &str, path: &str, outcome: &str) {
    let log = project_root().join("logs").join("agent-tools.jsonl");
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log) else { return };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs()).unwrap_or_default();
    let _ = writeln!(f, "{}",json!({"timestamp":timestamp,"action":action,"path":path,"outcome":outcome}));
}

#[tauri::command]
pub fn project_list(path: Option<String>) -> Result<Vec<ToolFile>,String> {
    let relative = path.unwrap_or_else(|| ".".into());
    let root = project_root();
    let dir = if relative == "." { root.clone() } else {
        safe_project_file(&relative,true)?
    };
    if !dir.is_dir() { return Err("Selecione uma pasta".into()); }
    let mut result = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e|e.to_string())?.take(150) {
        let entry = entry.map_err(|e|e.to_string())?;
        let entry_path = entry.path();
        if fs::symlink_metadata(&entry_path).map_err(|e|e.to_string())?.file_type().is_symlink() { continue; }
        let meta = entry.metadata().map_err(|e|e.to_string())?;
        let relative = entry_path.strip_prefix(&root).map_err(|e|e.to_string())?
            .to_string_lossy().replace('\\',"/");
        result.push(ToolFile {path:relative,size:meta.len(),is_dir:meta.is_dir()});
    }
    result.sort_by(|a,b| b.is_dir.cmp(&a.is_dir).then(a.path.cmp(&b.path)));
    Ok(result)
}

#[tauri::command]
pub fn project_read(path: String) -> Result<ToolResult, String> {
    let target = safe_project_file(&path,true)?;
    if !target.is_file() || disallow_sensitive(&path) {
        return Err("Formato de arquivo bloqueado: somente texto Markdown/TXT".into());
    }
    let ext = target.extension().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext != "md" && ext != "txt" {return Err("Leitura restrita a arquivos .md e .txt".into())}
    let len = fs::metadata(&target).map_err(|e|e.to_string())?.len();
    if len > 240_000 { return Err("Arquivo muito grande para leitura assistida".into()); }
    let content = fs::read_to_string(target).map_err(|_|"Arquivo não contém UTF-8 válido".to_string())?;
    audit("read",&path,"success");
    Ok(ToolResult{action:"read".into(),path,detail:"Arquivo lido em modo somente leitura".into(),content:Some(content),backup:None})
}

#[tauri::command]
pub fn project_write(request: ToolRequest) -> Result<ToolResult,String> {
    if request.action != "write" || request.confirmed != Some(true) {
        return Err("Confirmação explícita necessária para gravar arquivos".into());
    }
    let path = request.path.ok_or("Caminho ausente")?;
    if !writable_area(&path) || disallow_sensitive(&path) {
        return Err("A gravação é permitida apenas em docs/ ou data/ e arquivos Markdown/TXT".into());
    }
    let relative = normalize_project_path(&path)?;
    let ext = Path::new(&relative).extension().and_then(|x|x.to_str()).unwrap_or("");
    if !matches!(ext.to_ascii_lowercase().as_str(),"md"|"txt") {
        return Err("Somente arquivos .md e .txt são permitidos".into());
    }
    let content = request.content.ok_or("Conteúdo ausente")?;
    if content.as_bytes().len() > 100_000 { return Err("Conteúdo excede 100 KB".into()); }
    let target = safe_project_file(&path,false)?;
    if target.is_dir() { return Err("Não é possível sobrescrever uma pasta".into()); }
    let backup = if target.exists() {
        let _ = safe_project_file(&path,true)?;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|v|v.as_millis()).unwrap_or_default();
        let file = format!("tool-file-{}.bak",timestamp);
        let backup = project_root().join("backups").join(file);
        fs::copy(&target,&backup).map_err(|e|format!("Não foi possível salvar backup: {e}"))?;
        Some(backup.to_string_lossy().to_string())
    } else { None };
    let new_file = target.with_extension(format!("{}.trt-tmp",ext));
    fs::write(&new_file,&content).map_err(|e|e.to_string())?;
    // Keep original if rename fails; a previous backup is still available.
    if let Err(e) = fs::rename(&new_file,&target) {
        if target.exists() {
            // Windows does not always replace an open target with rename.
            // Never delete the original first. Backup exists before replacement.
            fs::copy(&new_file,&target)
                .map_err(|x|format!("Falha ao substituir arquivo: {x}; backup preservado. Erro inicial: {e}"))?;
            let _ = fs::remove_file(&new_file);
        } else {
            return Err(format!("Falha ao finalizar arquivo: {e}"));
        }
    }
    audit("write",&path,"success");
    Ok(ToolResult {action:"write".into(),path,detail:"Arquivo gravado com confirmação".into(),content:None,backup})
}

#[tauri::command]
pub fn tool_audit_history() -> Result<Vec<serde_json::Value>,String> {
    let file = project_root().join("logs").join("agent-tools.jsonl");
    if !file.is_file() {return Ok(Vec::new())}
    let log = fs::read_to_string(file).map_err(|e|e.to_string())?;
    Ok(log.lines().rev().take(40)
        .filter_map(|line|serde_json::from_str(line).ok()).collect())
}

#[tauri::command]
pub fn project_search(query: String) -> Result<Vec<ToolFile>,String> {
    let term = query.trim().to_ascii_lowercase();
    if term.len() < 2 || term.len() > 80 { return Err("Busca deve conter de 2 a 80 caracteres".into()); }
    let root=project_root();
    let folders=["docs","config","data"];
    let mut results=Vec::new();
    for folder in folders {
        let location=root.join(folder);
        if !location.is_dir(){continue;}
        for entry in fs::read_dir(location).map_err(|e|e.to_string())?.take(100) {
            let entry=entry.map_err(|e|e.to_string())?;
            if entry.file_type().map_err(|e|e.to_string())?.is_symlink() {continue;}
            let meta=entry.metadata().map_err(|e|e.to_string())?;
            if !meta.is_file() {continue;}
            let filename=entry.file_name().to_string_lossy().to_string();
            if !filename.to_ascii_lowercase().contains(&term) || disallow_sensitive(&filename) {continue;}
            results.push(ToolFile{path:entry.path().strip_prefix(&root).map_err(|e|e.to_string())?
                .to_string_lossy().replace('\\',"/"),size:meta.len(),is_dir:false});
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prevent_path_escape() {
        for bad in ["../secret","..\\Windows\\System32",r"C:\Windows\win.ini","docs/../../.env"] {
            assert!(normalize_project_path(bad).is_err(),"not blocked: {bad}");
        }
        assert!(normalize_project_path("docs/readme.md").is_ok());
    }
    #[test]
    fn round_trip_document_write_read() {
        let name = format!("docs/trt-test-{}.md",uuid::Uuid::new_v4());
        let input=ToolRequest{action:"write".into(),path:Some(name.clone()),
            content:Some("# Teste local\nDocumento seguro".into()),confirmed:Some(true)};
        let result=project_write(input).expect("Gravação aprovada");
        assert_eq!(result.action,"write");
        let file=project_read(name.clone()).expect("Leitura do arquivo recém-criado");
        assert!(file.content.unwrap().contains("Documento seguro"));
        let file_path=safe_project_file(&name,true).expect("Caminho seguro");
        fs::remove_file(file_path).expect("Limpeza do teste");
    }

    #[test]
    fn write_requires_confirmation() {
        let denied=ToolRequest{action:"write".into(),path:Some("docs/blocked.md".into()),
            content:Some("sem consentimento".into()),confirmed:Some(false)};
        assert!(project_write(denied).is_err());
    }

    #[test]
    fn limit_writes_to_documents() {
        assert!(writable_area("docs/nota.md"));
        assert!(writable_area("data/notas.txt"));
        assert!(!writable_area("app/src/App.tsx"));
        assert!(!writable_area("data/database/trt-info.sqlite"));
        assert!(disallow_sensitive("docs/token.env"));
    }
}
