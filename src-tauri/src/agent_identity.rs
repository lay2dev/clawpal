use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::config_io::read_openclaw_config;
use crate::models::OpenClawPaths;
use crate::ssh::SshConnectionPool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct IdentityDocument {
    name: Option<String>,
    emoji: Option<String>,
    persona: Option<String>,
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_identity_content(text: &str) -> IdentityDocument {
    let mut result = IdentityDocument::default();
    let normalized = text.replace("\r\n", "\n");
    let mut sections = normalized.splitn(2, "\n## Persona\n");
    let header = sections.next().unwrap_or_default();
    let persona = sections.next().map(|value| value.trim_end_matches('\n'));

    for line in header.lines() {
        if let Some(name) = line.strip_prefix("- Name:") {
            result.name = normalize_optional_text(Some(name));
        } else if let Some(emoji) = line.strip_prefix("- Emoji:") {
            result.emoji = normalize_optional_text(Some(emoji));
        }
    }

    result.persona = normalize_optional_text(persona);
    result
}

fn merge_identity_document(
    existing: Option<&str>,
    name: Option<&str>,
    emoji: Option<&str>,
    persona: Option<&str>,
) -> Result<IdentityDocument, String> {
    let existing = existing.map(parse_identity_content).unwrap_or_default();
    let name = normalize_optional_text(name).or(existing.name.clone());
    let emoji = normalize_optional_text(emoji).or(existing.emoji.clone());
    let persona = normalize_optional_text(persona).or(existing.persona.clone());

    let Some(name) = name else {
        return Err(
            "agent identity requires a name when no existing IDENTITY.md is present".into(),
        );
    };

    Ok(IdentityDocument {
        name: Some(name),
        emoji,
        persona,
    })
}

fn identity_content(
    existing: Option<&str>,
    name: Option<&str>,
    emoji: Option<&str>,
    persona: Option<&str>,
) -> Result<String, String> {
    let merged = merge_identity_document(existing, name, emoji, persona)?;
    let mut content = format!(
        "- Name: {}\n",
        merged.name.as_deref().unwrap_or_default().trim()
    );
    if let Some(emoji) = merged.emoji.as_deref() {
        content.push_str(&format!("- Emoji: {}\n", emoji));
    }
    if let Some(persona) = merged.persona.as_deref() {
        content.push_str("\n## Persona\n");
        content.push_str(persona);
        content.push('\n');
    }
    Ok(content)
}

fn resolve_workspace(
    cfg: &Value,
    agent_id: &str,
    default_workspace: Option<&str>,
) -> Result<String, String> {
    clawpal_core::doctor::resolve_agent_workspace_from_config(cfg, agent_id, default_workspace)
}

pub fn write_local_agent_identity(
    paths: &OpenClawPaths,
    agent_id: &str,
    name: Option<&str>,
    emoji: Option<&str>,
    persona: Option<&str>,
) -> Result<(), String> {
    let cfg = read_openclaw_config(paths)?;
    let workspace = resolve_workspace(&cfg, agent_id, None)
        .map(|path| shellexpand::tilde(&path).to_string())?;
    let workspace_path = Path::new(&workspace);
    fs::create_dir_all(workspace_path)
        .map_err(|error| format!("Failed to create workspace dir: {}", error))?;
    let identity_path = workspace_path.join("IDENTITY.md");
    let existing = fs::read_to_string(&identity_path).ok();
    fs::write(
        &identity_path,
        identity_content(existing.as_deref(), name, emoji, persona)?,
    )
    .map_err(|error| format!("Failed to write IDENTITY.md: {}", error))?;
    Ok(())
}

fn shell_escape(value: &str) -> String {
    let escaped = value.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

pub async fn write_remote_agent_identity(
    pool: &SshConnectionPool,
    host_id: &str,
    agent_id: &str,
    name: Option<&str>,
    emoji: Option<&str>,
    persona: Option<&str>,
) -> Result<(), String> {
    let (_config_path, _raw, cfg) =
        crate::commands::remote_read_openclaw_config_text_and_json(pool, host_id)
            .await
            .map_err(|error| format!("Failed to parse config: {error}"))?;

    let workspace = resolve_workspace(&cfg, agent_id, Some("~/.openclaw/agents"))?;
    let remote_workspace = if workspace.starts_with("~/") {
        workspace
    } else {
        format!("~/{workspace}")
    };
    pool.exec(
        host_id,
        &format!("mkdir -p {}", shell_escape(&remote_workspace)),
    )
    .await?;
    let identity_path = format!("{remote_workspace}/IDENTITY.md");
    let existing = match pool.sftp_read(host_id, &identity_path).await {
        Ok(text) => Some(text),
        Err(error) if error.contains("No such file") || error.contains("not found") => None,
        Err(error) => return Err(error),
    };
    pool.sftp_write(
        host_id,
        &identity_path,
        &identity_content(existing.as_deref(), name, emoji, persona)?,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_local_agent_identity;
    use crate::cli_runner::{
        lock_active_override_test_state, set_active_clawpal_data_override,
        set_active_openclaw_home_override,
    };
    use crate::models::resolve_paths;
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn write_local_agent_identity_creates_identity_file_from_config_workspace() {
        let _override_guard = lock_active_override_test_state();
        let temp_root = std::env::temp_dir().join(format!("clawpal-identity-{}", Uuid::new_v4()));
        let openclaw_home = temp_root.join("home");
        let clawpal_data = temp_root.join("data");
        let openclaw_dir = openclaw_home.join(".openclaw");
        let workspace = temp_root.join("workspace").join("lobster");
        fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
        fs::create_dir_all(&clawpal_data).expect("create clawpal data dir");
        fs::write(
            openclaw_dir.join("openclaw.json"),
            serde_json::to_string_pretty(&json!({
                "agents": {
                    "list": [
                        {
                            "id": "lobster",
                            "workspace": workspace.to_string_lossy(),
                        }
                    ]
                }
            }))
            .expect("serialize config"),
        )
        .expect("write config");

        set_active_openclaw_home_override(Some(openclaw_home.to_string_lossy().to_string()))
            .expect("set openclaw override");
        set_active_clawpal_data_override(Some(clawpal_data.to_string_lossy().to_string()))
            .expect("set clawpal override");

        let result = write_local_agent_identity(
            &resolve_paths(),
            "lobster",
            Some("Lobster"),
            Some("🦞"),
            Some("You help triage crabby incidents."),
        );

        set_active_openclaw_home_override(None).expect("clear openclaw override");
        set_active_clawpal_data_override(None).expect("clear clawpal override");

        assert!(result.is_ok());
        assert_eq!(
            fs::read_to_string(workspace.join("IDENTITY.md")).expect("read identity file"),
            "- Name: Lobster\n- Emoji: 🦞\n\n## Persona\nYou help triage crabby incidents.\n"
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn write_local_agent_identity_preserves_name_and_emoji_when_updating_persona_only() {
        let _override_guard = lock_active_override_test_state();
        let temp_root = std::env::temp_dir().join(format!("clawpal-identity-{}", Uuid::new_v4()));
        let openclaw_home = temp_root.join("home");
        let clawpal_data = temp_root.join("data");
        let openclaw_dir = openclaw_home.join(".openclaw");
        let workspace = temp_root.join("workspace").join("lobster");
        fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
        fs::create_dir_all(&clawpal_data).expect("create clawpal data dir");
        fs::create_dir_all(&workspace).expect("create workspace dir");
        fs::write(
            workspace.join("IDENTITY.md"),
            "- Name: Lobster\n- Emoji: 🦞\n\n## Persona\nOld persona.\n",
        )
        .expect("write identity seed");
        fs::write(
            openclaw_dir.join("openclaw.json"),
            serde_json::to_string_pretty(&json!({
                "agents": {
                    "list": [
                        {
                            "id": "lobster",
                            "workspace": workspace.to_string_lossy(),
                        }
                    ]
                }
            }))
            .expect("serialize config"),
        )
        .expect("write config");

        set_active_openclaw_home_override(Some(openclaw_home.to_string_lossy().to_string()))
            .expect("set openclaw override");
        set_active_clawpal_data_override(Some(clawpal_data.to_string_lossy().to_string()))
            .expect("set clawpal override");

        let result = write_local_agent_identity(
            &resolve_paths(),
            "lobster",
            None,
            None,
            Some("New persona."),
        );

        set_active_openclaw_home_override(None).expect("clear openclaw override");
        set_active_clawpal_data_override(None).expect("clear clawpal override");

        assert!(result.is_ok());
        assert_eq!(
            fs::read_to_string(workspace.join("IDENTITY.md")).expect("read identity file"),
            "- Name: Lobster\n- Emoji: 🦞\n\n## Persona\nNew persona.\n"
        );

        let _ = fs::remove_dir_all(temp_root);
    }
}
