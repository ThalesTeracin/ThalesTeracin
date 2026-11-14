use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tauri::State;

use crate::{db, secret_entry, validate_url, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub existing: usize,
    pub keys_available: usize,
    pub skipped: usize,
    pub source: String,
}

fn is_free_model(model: &str) -> bool {
    model.starts_with("free/") || model.ends_with(":free") || model.ends_with("-free")
}

fn env_secret_reference(value: &str) -> Option<&str> {
    let name = value.strip_prefix("{env:")?.strip_suffix('}')?;
    if !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Some(name)
    } else {
        None
    }
}

#[tauri::command]
pub fn import_from_opencode(state: State<'_, AppState>) -> Result<ImportReport, String> {
    let user = std::env::var_os("USERPROFILE").ok_or("Perfil Windows indisponível")?;
    let source = PathBuf::from(user).join(".config").join("opencode").join("opencode.json");
    let data = std::fs::read_to_string(&source)
        .map_err(|_| "Não foi possível ler a configuração do OpenCode".to_owned())?;
    let object: Value = serde_json::from_str(&data)
        .map_err(|_| "Configuração OpenCode não é um JSON válido".to_owned())?;
    let providers = object
        .get("provider")
        .and_then(Value::as_object)
        .ok_or("Nenhum catálogo de provedores do OpenCode foi encontrado")?;

    let conn = db(&state)?;
    let mut result = ImportReport {
        imported: 0,
        existing: 0,
        keys_available: 0,
        skipped: 0,
        source: "OpenCode (leitura somente)".to_owned(),
    };

    for (source_id, provider) in providers {
        if source_id.len() > 70 || source_id.is_empty()
            || !source_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            result.skipped += 1;
            continue;
        }
        let endpoint = provider.pointer("/options/baseURL")
            .or_else(|| provider.get("baseURL"))
            .and_then(Value::as_str);
        let Some(url) = endpoint else {
            result.skipped += 1;
            continue;
        };
        if validate_url(url).is_err() {
            result.skipped += 1;
            continue;
        }
        let Some(models) = provider.get("models").and_then(Value::as_object) else {
            result.skipped += 1;
            continue;
        };
        if models.is_empty() {
            result.skipped += 1;
            continue;
        }
        let model = models.keys()
            .find(|id| is_free_model(id))
            .or_else(|| models.keys().next())
            .expect("verificado: coleção não vazia");

        let id = format!("oc-{source_id}");
        let display_name = format!("{} (OpenCode)", source_id);
        let cost = if is_free_model(model) || source_id == "freellmapi" { "free" } else { "unknown" };
        // Importação não ativa modelos; os testes de chat determinam quais são utilizáveis.
        let added = conn.execute(
            "INSERT OR IGNORE INTO providers(id,name,base_url,model,enabled,priority,cost_status)
             VALUES (?1,?2,?3,?4,0,90,?5)",
            rusqlite::params![id, display_name, url.trim_end_matches('/'), model, cost],
        ).map_err(|e|format!("Não foi possível registrar provedor: {e}"))?;
        if added == 1 {
            result.imported += 1;
        } else {
            result.existing += 1;
        }

        if let Some(reference) = provider.pointer("/options/apiKey").and_then(Value::as_str)
            .and_then(env_secret_reference)
        {
            let entry = secret_entry(&id)?;
            if entry.get_password().is_ok() {
                result.keys_available += 1;
            } else if let Ok(secret) = std::env::var(reference) {
                if !secret.trim().is_empty() {
                    entry.set_password(secret.trim()).map_err(|e|
                        format!("Falha ao salvar credencial do TRT-Info no Windows: {e}"))?;
                    result.keys_available += 1;
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    #[ignore = "Teste manual no Windows após importar as chaves; não mostra segredos"]
    fn imported_key_is_available_to_rust() {
        let key = secret_entry("oc-beatapi").expect("Gerenciador de Credenciais indisponível")
            .get_password().expect("Chave BeatAPI não encontrada no Windows");
        assert!(key.len() >= 8, "Credencial inválida");
    }

    #[test]
    fn free_model_markers() {
        assert!(is_free_model("free/glm-5.3-flash"));
        assert!(is_free_model("qwen/qwen3.5-flash:free"));
        assert!(is_free_model("glm-5.3-flash-free"));
        assert!(!is_free_model("deepseek-v4-flash"));
    }

    #[test]
    fn validate_environment_reference() {
        assert_eq!(env_secret_reference("{env:BEATAPI_API_KEY}"), Some("BEATAPI_API_KEY"));
        assert!(env_secret_reference("{env:FOO;DEL}").is_none());
        assert!(env_secret_reference("abc123").is_none());
    }
}
