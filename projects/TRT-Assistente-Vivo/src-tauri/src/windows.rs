use serde::Serialize;
use serde_json::{json, Value};
use std::process::Command;
use std::path::PathBuf;
use sysinfo::{Disks, System};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub executable: Option<String>,
    pub note: String,
}

fn home() -> Option<PathBuf> { std::env::var_os("USERPROFILE").map(PathBuf::from) }

fn resolve_tool(id: &str) -> Option<PathBuf> {
    let home = home()?;
    let candidate = match id {
        "claude" => home.join(".local").join("bin").join("claude.exe"),
        "opencode" => home.join("bin").join("opencode.cmd"),
        "git" => PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"),
        "node" => PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
        "pwsh" => home.join("AppData").join("Local").join("Microsoft")
            .join("WindowsApps").join("pwsh.exe"),
        _ => return None,
    };
    candidate.is_file().then_some(candidate)
}

#[tauri::command]
pub fn installed_tools() -> Vec<ToolInfo> {
    [
        ("claude", "Claude Code", "CLI identificado; nenhuma sessão ou configuração foi acessada"),
        ("opencode", "OpenCode", "CLI identificado; nenhuma configuração foi modificada"),
        ("git", "Git", "Ferramenta local de desenvolvimento"),
        ("node", "Node.js", "Runtime JavaScript"),
        ("pwsh", "PowerShell 7", "Shell disponível para tarefas assistidas"),
    ].into_iter().map(|(id,name,note)| {
        let path = resolve_tool(id);
        ToolInfo {
            id: id.to_string(), name:name.to_string(),
            installed:path.is_some(),
            executable:path.map(|p|p.display().to_string()),
            note:note.to_string(),
        }
    }).collect()
}

#[tauri::command]
pub fn inspect_windows() -> Value {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "Desconhecido".into());
    let root = PathBuf::from(r"C:\TRT-Assistente-Vivo-Standalone");
    let free = root.join("data");
    json!({
        "operatingSystem":os,
        "architecture":arch,
        "computer": std::env::var("COMPUTERNAME").unwrap_or_default(),
        "user":user,
        "projectExists":root.is_dir(),
        "databaseDirectoryExists":free.join("database").is_dir(),
        "tools": installed_tools(),
        "permissions":"Somente leitura; nenhuma ação de controle do sistema é executada"
    })
}

// Structured, read-only hardware/Windows diagnostic; never exposes device IDs or secrets.
#[tauri::command]
pub fn diagnose_windows() -> Value {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let disks = Disks::new_with_refreshed_list();
    let storage: Vec<Value> = disks.iter()
        .filter(|d| d.mount_point().to_string_lossy().to_ascii_uppercase().starts_with("C:"))
        .take(2).map(|d| json!({
            "drive":d.mount_point().to_string_lossy().to_string(),
            "totalGb":d.total_space()/1_073_741_824,
            "freeGb":d.available_space()/1_073_741_824,
        })).collect();
    let cpu = sys.cpus().first().map(|c|c.brand().to_owned())
        .unwrap_or_else(|| "CPU nao identificado".into());
    // No parameters supplied by a model: static Get-CimInstance query, strictly read-only.
    let bios = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "(Get-CimInstance -ClassName Win32_BIOS -ErrorAction SilentlyContinue).SMBIOSBIOSVersion"])
        .output().ok().filter(|o|o.status.success())
        .map(|o|String::from_utf8_lossy(&o.stdout).trim().chars().take(120).collect::<String>())
        .filter(|s|!s.is_empty()).unwrap_or_else(||"Nao disponivel".into());
    json!({
        "operatingSystem":System::long_os_version().unwrap_or_else(||"Windows".into()),
        "architecture":std::env::consts::ARCH,
        "cpu":cpu,
        "logicalCpus":sys.cpus().len(),
        "ramTotalMb":sys.total_memory()/1_048_576,
        "ramUsedMb":sys.used_memory()/1_048_576,
        "ramFreeMb":sys.available_memory()/1_048_576,
        "drives":storage,
        "biosVersion":bios,
        "biosChangesAllowed":false,
        "readOnly":true,
        "recommendation":"Diagnostico local somente leitura. Atualizacoes de BIOS e configuracoes de firmware exigem procedimento separado e confirmacao."
    })
}

#[tauri::command]
pub fn tool_location(id: String) -> Result<String,String> {
    if !["claude","opencode","git","node","pwsh"].contains(&id.as_str()) {
        return Err("Ferramenta não permitida".into());
    }
    // Apenas abre o terminal e exibe a localização da ferramenta para
    // não modificar perfis ou iniciar IAs sem conhecimento do operador.
    let tool = resolve_tool(&id).ok_or("Ferramenta não localizada")?;
    let path = tool.display().to_string();
    Ok(format!("Ferramenta {} localizada em {}",id,path))
}

#[tauri::command]
pub fn open_project_folder() -> Result<(),String> {
    let target = PathBuf::from(r"C:\TRT-Assistente-Vivo-Standalone");
    if !target.is_dir() { return Err("Pasta de projeto ausente".into()); }
    Command::new("explorer.exe").arg(&target)
        .spawn().map_err(|e|format!("Não foi possível abrir o Explorador: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_app_is_not_resolved() {
        assert!(resolve_tool("powershell.exe -EncodedCommand").is_none());
        assert!(resolve_tool("C:\\Windows\\System32\\cmd.exe").is_none());
    }
    #[test]
    fn diagnostics_are_read_only() {
        // The returned report must not offer firmware modification.
        assert_eq!(diagnose_windows()["biosChangesAllowed"],false);
        assert_eq!(diagnose_windows()["readOnly"],true);
    }
    #[test]
    fn system_inspection_contains_architecture() {
        assert_eq!(inspect_windows()["architecture"], "aarch64");
    }
}
#[tauri::command]
pub fn launch_windows_app(id: String) -> Result<(), String> {
    let executable = match id.as_str() {
        "notepad" => "notepad.exe",
        "calculator" => "calc.exe",
        "explorer" => "explorer.exe",
        "paint" => "mspaint.exe",
        "task-manager" => "taskmgr.exe",
        _ => return Err("Aplicativo não autorizado".into()),
    };
    Command::new(executable)
        .spawn()
        .map(|_| ())
        .map_err(|e|format!("Falha ao abrir {id}: {e}"))
}

#[tauri::command]
pub fn launch_cli_terminal(id: String) -> Result<(), String> {
    if id != "claude" && id != "opencode" {
        return Err("Apenas as CLIs Claude Code e OpenCode são permitidas".into());
    }
    let executable = resolve_tool(&id).ok_or("CLI não localizada")?;
    let title = match id.as_str() {
        "claude" => "TRT - Claude Code",
        _ => "TRT - OpenCode",
    };
    let folder = r"C:\TRT-Assistente-Vivo-Standalone";
    // Abre terminal visível, sem executar comandos remotos ou alterar configurações.
    // O caminho de cada CLI vem de uma lista fechada de locais confiáveis.
    let args = [
        "new-tab", "--title", title, "-d", folder,
        "cmd.exe", "/k", executable.to_str().ok_or("Caminho CLI inválido")?,
    ];
    Command::new("wt.exe")
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e|format!("Falha ao abrir o terminal: {e}"))
}
