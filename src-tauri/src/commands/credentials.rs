use super::*;

pub(crate) fn truncate_error_text(input: &str, max_chars: usize) -> String {
    if let Some((i, _)) = input.char_indices().nth(max_chars) {
        format!("{}...", &input[..i])
    } else {
        input.to_string()
    }
}

pub(crate) const MAX_ERROR_SNIPPET_CHARS: usize = 280;

pub(crate) fn provider_supports_optional_api_key(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama" | "lmstudio" | "lm-studio" | "localai" | "vllm" | "llamacpp" | "llama.cpp"
    )
}

pub(crate) fn default_base_url_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "openai-codex" | "github-copilot" | "copilot" => {
            Some("https://api.openai.com/v1")
        }
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "ollama" => Some("http://127.0.0.1:11434/v1"),
        "lmstudio" | "lm-studio" => Some("http://127.0.0.1:1234/v1"),
        "localai" => Some("http://127.0.0.1:8080/v1"),
        "vllm" => Some("http://127.0.0.1:8000/v1"),
        "groq" => Some("https://api.groq.com/openai/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "xai" | "grok" => Some("https://api.x.ai/v1"),
        "together" => Some("https://api.together.xyz/v1"),
        "mistral" => Some("https://api.mistral.ai/v1"),
        "anthropic" => Some("https://api.anthropic.com/v1"),
        _ => None,
    }
}

pub(crate) fn run_provider_probe(
    provider: String,
    model: String,
    base_url: Option<String>,
    api_key: String,
) -> Result<(), String> {
    let provider_trimmed = provider.trim().to_string();
    let mut model_trimmed = model.trim().to_string();
    let lower = provider_trimmed.to_ascii_lowercase();
    if provider_trimmed.is_empty() || model_trimmed.is_empty() {
        return Err("provider and model are required".into());
    }
    let provider_prefix = format!("{}/", provider_trimmed.to_ascii_lowercase());
    if model_trimmed
        .to_ascii_lowercase()
        .starts_with(&provider_prefix)
    {
        model_trimmed = model_trimmed[provider_prefix.len()..].to_string();
        if model_trimmed.trim().is_empty() {
            return Err("model is empty after provider prefix normalization".into());
        }
    }
    if api_key.trim().is_empty() && !provider_supports_optional_api_key(&provider_trimmed) {
        return Err("API key is not configured for this profile".into());
    }

    let resolved_base = base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.trim_end_matches('/').to_string())
        .or_else(|| default_base_url_for_provider(&provider_trimmed).map(str::to_string))
        .ok_or_else(|| format!("No base URL configured for provider '{}'", provider_trimmed))?;

    // Use stream:true so the provider returns HTTP headers immediately once
    // the request is accepted, rather than waiting for the full completion.
    // We only need the status code to verify auth + model access.
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let auth_kind = infer_auth_kind(&provider_trimmed, api_key.trim(), InternalAuthKind::ApiKey);
    let looks_like_claude_model = model_trimmed.to_ascii_lowercase().contains("claude");
    let use_anthropic_probe_for_openai_codex = lower == "openai-codex" && looks_like_claude_model;
    let response = if lower == "anthropic" || use_anthropic_probe_for_openai_codex {
        let normalized_model = model_trimmed
            .rsplit('/')
            .next()
            .unwrap_or(model_trimmed.as_str())
            .to_string();
        let url = format!("{}/messages", resolved_base);
        let payload = serde_json::json!({
            "model": normalized_model,
            "max_tokens": 1,
            "stream": true,
            "messages": [{"role": "user", "content": "ping"}]
        });
        let build_request = |use_bearer: bool| -> Result<reqwest::blocking::Response, String> {
            let mut req = client
                .post(&url)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json");
            req = if use_bearer {
                req.header("Authorization", format!("Bearer {}", api_key.trim()))
            } else {
                req.header("x-api-key", api_key.trim())
            };
            req.json(&payload)
                .send()
                .map_err(|e| format!("Provider request failed: {e}"))
        };
        let response = match auth_kind {
            InternalAuthKind::Authorization => build_request(true)?,
            InternalAuthKind::ApiKey => build_request(false)?,
        };
        if !response.status().is_success()
            && (response.status().as_u16() == 401 || response.status().as_u16() == 403)
        {
            let fallback_use_bearer = matches!(auth_kind, InternalAuthKind::ApiKey);
            if let Ok(fallback_response) = build_request(fallback_use_bearer) {
                if fallback_response.status().is_success() {
                    return Ok(());
                }
            }
        }
        response
    } else {
        let url = format!("{}/chat/completions", resolved_base);
        let mut req = client
            .post(&url)
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": model_trimmed,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1,
                "stream": true
            }));
        if !api_key.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
        }
        if lower == "openrouter" {
            req = req
                .header("HTTP-Referer", "https://clawpal.zhixian.io")
                .header("X-Title", "ClawPal");
        }
        req.send()
            .map_err(|e| format!("Provider request failed: {e}"))?
    };

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status().as_u16();
    let body = response
        .text()
        .unwrap_or_else(|e| format!("(could not read response body: {e})"));
    let snippet = truncate_error_text(body.trim(), MAX_ERROR_SNIPPET_CHARS);
    let snippet_lower = snippet.to_ascii_lowercase();
    if lower == "anthropic"
        && snippet_lower.contains("oauth authentication is currently not supported")
    {
        return Err(
            "Anthropic provider does not accept Claude setup-token OAuth tokens. Use an Anthropic API key (sk-ant-...) for provider=anthropic."
                .to_string(),
        );
    }
    if snippet.is_empty() {
        Err(format!("Provider rejected credentials (HTTP {status})"))
    } else {
        Err(format!(
            "Provider rejected credentials (HTTP {status}): {snippet}"
        ))
    }
}

pub(crate) fn resolve_profile_api_key_with_priority(
    profile: &ModelProfile,
    base_dir: &Path,
) -> Option<(String, u8)> {
    resolve_profile_credential_with_priority(profile, base_dir)
        .map(|(credential, priority, _)| (credential.secret, priority))
}


pub(crate) fn infer_auth_kind(provider: &str, secret: &str, fallback: InternalAuthKind) -> InternalAuthKind {
    if provider.trim().eq_ignore_ascii_case("anthropic") {
        let lower = secret.trim().to_ascii_lowercase();
        if lower.starts_with("sk-ant-oat") || lower.starts_with("oauth_") {
            return InternalAuthKind::Authorization;
        }
    }
    fallback
}

pub(crate) fn provider_env_var_candidates(provider: &str) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut push_unique = |name: &str| {
        if !name.is_empty() && !out.iter().any(|existing| existing == name) {
            out.push(name.to_string());
        }
    };

    let normalized = provider.trim().to_ascii_lowercase();
    let provider_env = normalized.to_uppercase().replace('-', "_");
    if !provider_env.is_empty() {
        push_unique(&format!("{provider_env}_API_KEY"));
        push_unique(&format!("{provider_env}_KEY"));
        push_unique(&format!("{provider_env}_TOKEN"));
    }

    if normalized == "anthropic" {
        push_unique("ANTHROPIC_OAUTH_TOKEN");
        push_unique("ANTHROPIC_AUTH_TOKEN");
    }
    if normalized == "openai-codex"
        || normalized == "openai_codex"
        || normalized == "github-copilot"
        || normalized == "copilot"
    {
        push_unique("OPENAI_CODEX_TOKEN");
        push_unique("OPENAI_CODEX_AUTH_TOKEN");
    }

    out
}

pub(crate) fn is_oauth_provider_alias(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "openai-codex" | "openai_codex" | "github-copilot" | "copilot"
    )
}

pub(crate) fn is_oauth_auth_ref(provider: &str, auth_ref: &str) -> bool {
    if !is_oauth_provider_alias(provider) {
        return false;
    }
    let lower = auth_ref.trim().to_ascii_lowercase();
    lower.starts_with("openai-codex:") || lower.starts_with("openai:")
}

pub(crate) fn infer_resolved_credential_kind(
    profile: &ModelProfile,
    source: Option<ResolvedCredentialSource>,
) -> ResolvedCredentialKind {
    let auth_ref = profile.auth_ref.trim();
    match source {
        Some(ResolvedCredentialSource::ManualApiKey) => ResolvedCredentialKind::Manual,
        Some(ResolvedCredentialSource::ProviderEnvVar) => ResolvedCredentialKind::EnvRef,
        Some(ResolvedCredentialSource::ExplicitAuthRef) => {
            if is_oauth_auth_ref(&profile.provider, auth_ref) {
                ResolvedCredentialKind::OAuth
            } else {
                ResolvedCredentialKind::EnvRef
            }
        }
        Some(ResolvedCredentialSource::ProviderFallbackAuthRef) => {
            let fallback_ref = format!("{}:default", profile.provider.trim().to_ascii_lowercase());
            if is_oauth_auth_ref(&profile.provider, &fallback_ref) {
                ResolvedCredentialKind::OAuth
            } else {
                ResolvedCredentialKind::EnvRef
            }
        }
        None => {
            if !auth_ref.is_empty() {
                if is_oauth_auth_ref(&profile.provider, auth_ref) {
                    ResolvedCredentialKind::OAuth
                } else {
                    ResolvedCredentialKind::EnvRef
                }
            } else if profile
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|v| !v.is_empty())
            {
                ResolvedCredentialKind::Manual
            } else {
                ResolvedCredentialKind::Unset
            }
        }
    }
}

pub(crate) fn resolve_profile_credential_with_priority(
    profile: &ModelProfile,
    base_dir: &Path,
) -> Option<(InternalProviderCredential, u8, ResolvedCredentialSource)> {
    // 1. Try explicit auth_ref (user-specified) as env var, then auth store.
    let auth_ref = profile.auth_ref.trim();
    let has_explicit_auth_ref = !auth_ref.is_empty();
    if has_explicit_auth_ref {
        if is_valid_env_var_name(auth_ref) {
            if let Ok(val) = std::env::var(auth_ref) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let kind =
                        infer_auth_kind(&profile.provider, trimmed, InternalAuthKind::ApiKey);
                    return Some((
                        InternalProviderCredential {
                            secret: trimmed.to_string(),
                            kind,
                        },
                        40,
                        ResolvedCredentialSource::ExplicitAuthRef,
                    ));
                }
            }
        }
        if let Some(credential) = resolve_credential_from_agent_auth_profiles(base_dir, auth_ref) {
            return Some((credential, 30, ResolvedCredentialSource::ExplicitAuthRef));
        }
    }

    // 2. Direct api_key field — takes priority over fallback auth_ref candidates
    //    so a user-entered key is never shadowed by stale auth-store entries.
    if let Some(ref key) = profile.api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            let kind = infer_auth_kind(&profile.provider, trimmed, InternalAuthKind::ApiKey);
            return Some((
                InternalProviderCredential {
                    secret: trimmed.to_string(),
                    kind,
                },
                20,
                ResolvedCredentialSource::ManualApiKey,
            ));
        }
    }

    // 3. Fallback: provider:default auth_ref (auto-generated) — env var then auth store.
    let provider_fallback = profile.provider.trim().to_ascii_lowercase();
    if !provider_fallback.is_empty() {
        let fallback_ref = format!("{provider_fallback}:default");
        let skip = has_explicit_auth_ref && auth_ref == fallback_ref;
        if !skip {
            if is_valid_env_var_name(&fallback_ref) {
                if let Ok(val) = std::env::var(&fallback_ref) {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        let kind =
                            infer_auth_kind(&profile.provider, trimmed, InternalAuthKind::ApiKey);
                        return Some((
                            InternalProviderCredential {
                                secret: trimmed.to_string(),
                                kind,
                            },
                            15,
                            ResolvedCredentialSource::ProviderFallbackAuthRef,
                        ));
                    }
                }
            }
            if let Some(credential) =
                resolve_credential_from_agent_auth_profiles(base_dir, &fallback_ref)
            {
                return Some((
                    credential,
                    15,
                    ResolvedCredentialSource::ProviderFallbackAuthRef,
                ));
            }
        }
    }

    // 4. Provider-based env var conventions.
    for env_name in provider_env_var_candidates(&profile.provider) {
        if let Ok(val) = std::env::var(&env_name) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                let fallback_kind = if env_name.ends_with("_TOKEN") {
                    InternalAuthKind::Authorization
                } else {
                    InternalAuthKind::ApiKey
                };
                let kind = infer_auth_kind(&profile.provider, trimmed, fallback_kind);
                return Some((
                    InternalProviderCredential {
                        secret: trimmed.to_string(),
                        kind,
                    },
                    10,
                    ResolvedCredentialSource::ProviderEnvVar,
                ));
            }
        }
    }

    None
}

pub(crate) fn resolve_profile_api_key(profile: &ModelProfile, base_dir: &Path) -> String {
    resolve_profile_api_key_with_priority(profile, base_dir)
        .map(|(key, _)| key)
        .unwrap_or_default()
}

pub(crate) fn collect_provider_credentials_for_internal(
) -> HashMap<String, InternalProviderCredential> {
    let paths = resolve_paths();
    collect_provider_credentials_from_paths(&paths)
}

pub(crate) fn collect_provider_credentials_from_paths(
    paths: &crate::models::OpenClawPaths,
) -> HashMap<String, InternalProviderCredential> {
    let profiles = load_model_profiles(&paths);
    let mut out = collect_provider_credentials_from_profiles(&profiles, &paths.base_dir);
    augment_provider_credentials_from_openclaw_config(paths, &mut out);
    out
}

pub(crate) fn collect_provider_credentials_from_profiles(
    profiles: &[ModelProfile],
    base_dir: &Path,
) -> HashMap<String, InternalProviderCredential> {
    let mut out = HashMap::<String, (InternalProviderCredential, u8)>::new();
    for profile in profiles.iter().filter(|p| p.enabled) {
        let Some((credential, priority, _)) =
            resolve_profile_credential_with_priority(profile, base_dir)
        else {
            continue;
        };
        let provider = profile.provider.trim().to_lowercase();
        match out.get_mut(&provider) {
            Some((existing_credential, existing_priority)) => {
                if priority > *existing_priority {
                    *existing_credential = credential;
                    *existing_priority = priority;
                }
            }
            None => {
                out.insert(provider, (credential, priority));
            }
        }
    }
    out.into_iter().map(|(k, (v, _))| (k, v)).collect()
}

pub(crate) fn augment_provider_credentials_from_openclaw_config(
    paths: &crate::models::OpenClawPaths,
    out: &mut HashMap<String, InternalProviderCredential>,
) {
    let cfg = match read_openclaw_config(paths) {
        Ok(cfg) => cfg,
        Err(_) => return,
    };
    let Some(providers) = cfg.pointer("/models/providers").and_then(Value::as_object) else {
        return;
    };

    for (provider, provider_cfg) in providers {
        let provider_key = provider.trim().to_ascii_lowercase();
        if provider_key.is_empty() || out.contains_key(&provider_key) {
            continue;
        }
        let Some(provider_obj) = provider_cfg.as_object() else {
            continue;
        };
        if let Some(credential) =
            resolve_provider_credential_from_config_entry(&cfg, provider, provider_obj)
        {
            out.insert(provider_key, credential);
        }
    }
}

pub(crate) fn resolve_provider_credential_from_config_entry(
    cfg: &Value,
    provider: &str,
    provider_cfg: &Map<String, Value>,
) -> Option<InternalProviderCredential> {
    for (field, fallback_kind, allow_plaintext) in [
        ("apiKey", InternalAuthKind::ApiKey, true),
        ("api_key", InternalAuthKind::ApiKey, true),
        ("key", InternalAuthKind::ApiKey, true),
        ("token", InternalAuthKind::Authorization, true),
        ("access", InternalAuthKind::Authorization, true),
        ("secretRef", InternalAuthKind::ApiKey, false),
        ("keyRef", InternalAuthKind::ApiKey, false),
        ("tokenRef", InternalAuthKind::Authorization, false),
        ("apiKeyRef", InternalAuthKind::ApiKey, false),
        ("api_key_ref", InternalAuthKind::ApiKey, false),
        ("accessRef", InternalAuthKind::Authorization, false),
    ] {
        let Some(raw_val) = provider_cfg.get(field) else {
            continue;
        };

        if allow_plaintext {
            if let Some(secret) = raw_val.as_str().map(str::trim).filter(|v| !v.is_empty()) {
                let kind = infer_auth_kind(provider, secret, fallback_kind);
                return Some(InternalProviderCredential {
                    secret: secret.to_string(),
                    kind,
                });
            }
        }
        if let Some(secret_ref) = try_parse_secret_ref(raw_val) {
            if let Some(secret) =
                resolve_secret_ref_with_provider_config(&secret_ref, cfg, &local_env_lookup)
            {
                let kind = infer_auth_kind(provider, &secret, fallback_kind);
                return Some(InternalProviderCredential { secret, kind });
            }
        }
    }
    None
}

pub(crate) fn resolve_credential_from_agent_auth_profiles(
    base_dir: &Path,
    auth_ref: &str,
) -> Option<InternalProviderCredential> {
    for root in local_openclaw_roots(base_dir) {
        let agents_dir = root.join("agents");
        if !agents_dir.exists() {
            continue;
        }
        let entries = match fs::read_dir(&agents_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let agent_dir = entry.path().join("agent");
            if let Some(credential) =
                resolve_credential_from_local_auth_store_dir(&agent_dir, auth_ref)
            {
                return Some(credential);
            }
        }
    }
    None
}

pub(crate) fn resolve_credential_from_local_auth_store_dir(
    agent_dir: &Path,
    auth_ref: &str,
) -> Option<InternalProviderCredential> {
    for file_name in ["auth-profiles.json", "auth.json"] {
        let auth_file = agent_dir.join(file_name);
        if !auth_file.exists() {
            continue;
        }
        let text = fs::read_to_string(&auth_file).ok()?;
        let data: Value = serde_json::from_str(&text).ok()?;
        if let Some(credential) = resolve_credential_from_auth_store_json(&data, auth_ref) {
            return Some(credential);
        }
    }
    None
}

pub(crate) fn local_openclaw_roots(base_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::<PathBuf>::new();
    let mut seen = std::collections::BTreeSet::<PathBuf>::new();
    let push_root = |roots: &mut Vec<PathBuf>,
                     seen: &mut std::collections::BTreeSet<PathBuf>,
                     root: PathBuf| {
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    };
    push_root(&mut roots, &mut seen, base_dir.to_path_buf());
    let home = dirs::home_dir();
    if let Some(home) = home {
        if let Ok(entries) = fs::read_dir(&home) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with(".openclaw") {
                    push_root(&mut roots, &mut seen, path);
                }
            }
        }
    }
    roots
}

pub(crate) fn auth_ref_lookup_keys(auth_ref: &str) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = auth_ref.trim();
    if trimmed.is_empty() {
        return out;
    }
    out.push(trimmed.to_string());
    if let Some((provider, _)) = trimmed.split_once(':') {
        if !provider.trim().is_empty() {
            out.push(provider.trim().to_string());
        }
    }
    out
}

pub(crate) fn resolve_key_from_auth_store_json(data: &Value, auth_ref: &str) -> Option<String> {
    resolve_credential_from_auth_store_json(data, auth_ref).map(|credential| credential.secret)
}

pub(crate) fn resolve_key_from_auth_store_json_with_env(
    data: &Value,
    auth_ref: &str,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    resolve_credential_from_auth_store_json_with_env(data, auth_ref, env_lookup)
        .map(|credential| credential.secret)
}

pub(crate) fn resolve_credential_from_auth_store_json(
    data: &Value,
    auth_ref: &str,
) -> Option<InternalProviderCredential> {
    resolve_credential_from_auth_store_json_with_env(data, auth_ref, &local_env_lookup)
}

pub(crate) fn resolve_credential_from_auth_store_json_with_env(
    data: &Value,
    auth_ref: &str,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<InternalProviderCredential> {
    let keys = auth_ref_lookup_keys(auth_ref);
    if keys.is_empty() {
        return None;
    }

    if let Some(profiles) = data.get("profiles").and_then(Value::as_object) {
        for key in &keys {
            if let Some(auth_entry) = profiles.get(key) {
                if let Some(credential) =
                    extract_credential_from_auth_entry_with_env(auth_entry, env_lookup)
                {
                    return Some(credential);
                }
            }
        }
    }

    if let Some(root_obj) = data.as_object() {
        for key in &keys {
            if let Some(auth_entry) = root_obj.get(key) {
                if let Some(credential) =
                    extract_credential_from_auth_entry_with_env(auth_entry, env_lookup)
                {
                    return Some(credential);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// SecretRef resolution — OpenClaw secrets management compatibility
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct SecretRef {
    source: String,
    provider: Option<String>,
    id: String,
}

pub(crate) fn try_parse_secret_ref(value: &Value) -> Option<SecretRef> {
    let obj = value.as_object()?;
    let source = obj.get("source")?.as_str()?.trim();
    let provider = obj
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_ascii_lowercase);
    let id = obj.get("id")?.as_str()?.trim();
    if source.is_empty() || id.is_empty() {
        return None;
    }
    Some(SecretRef {
        source: source.to_string(),
        provider,
        id: id.to_string(),
    })
}

pub(crate) fn normalize_secret_provider_name(cfg: &Value, secret_ref: &SecretRef) -> Option<String> {
    if let Some(provider) = secret_ref.provider.as_deref().map(str::trim) {
        if !provider.is_empty() {
            return Some(provider.to_ascii_lowercase());
        }
    }
    let defaults_key = format!("/secrets/defaults/{}", secret_ref.source.trim());
    cfg.pointer(&defaults_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_ascii_lowercase)
}

pub(crate) fn load_secret_provider_config<'a>(
    cfg: &'a Value,
    provider: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    cfg.pointer("/secrets/providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(provider))
        .and_then(Value::as_object)
}

pub(crate) fn secret_ref_allowed_in_provider_cfg(
    provider_cfg: &serde_json::Map<String, Value>,
    id: &str,
) -> bool {
    let Some(ids) = provider_cfg.get("ids").and_then(Value::as_array) else {
        return true;
    };
    ids.iter()
        .filter_map(Value::as_str)
        .any(|candidate| candidate.trim() == id)
}

pub(crate) fn expand_home_path(raw: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(raw).to_string())
}

pub(crate) fn resolve_secret_ref_file_with_provider_config(
    secret_ref: &SecretRef,
    provider_cfg: &serde_json::Map<String, Value>,
) -> Option<String> {
    let source = provider_cfg
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !source.is_empty() && source != "file" {
        return None;
    }
    if !secret_ref_allowed_in_provider_cfg(provider_cfg, &secret_ref.id) {
        return None;
    }
    let path = provider_cfg.get("path").and_then(Value::as_str)?.trim();
    if path.is_empty() {
        return None;
    }
    let file_path = expand_home_path(path);
    let content = fs::read_to_string(&file_path).ok()?;
    let mode = provider_cfg
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("json")
        .trim()
        .to_ascii_lowercase();
    if mode == "singlevalue" {
        if secret_ref.id.trim() != "value" {
            eprintln!(
                "SecretRef file source: singlevalue mode requires id 'value', got '{}'",
                secret_ref.id.trim()
            );
            return None;
        }
        let trimmed = content.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    let parsed: Value = serde_json::from_str(&content).ok()?;
    let id = secret_ref.id.trim();
    if !id.starts_with('/') {
        eprintln!("SecretRef file source: JSON mode expects id to start with '/', got '{id}'");
        return None;
    }
    let resolved = parsed.pointer(id)?;
    let out = match resolved {
        Value::String(v) => v.trim().to_string(),
        Value::Number(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        _ => String::new(),
    };
    (!out.is_empty()).then_some(out)
}

pub(crate) fn read_trusted_dirs(provider_cfg: &serde_json::Map<String, Value>) -> Vec<PathBuf> {
    provider_cfg
        .get("trustedDirs")
        .and_then(Value::as_array)
        .map(|dirs| {
            dirs.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|dir| !dir.is_empty())
                .map(expand_home_path)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn resolve_secret_ref_exec_with_provider_config(
    secret_ref: &SecretRef,
    provider_name: &str,
    provider_cfg: &serde_json::Map<String, Value>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let source = provider_cfg
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !source.is_empty() && source != "exec" {
        return None;
    }
    if !secret_ref_allowed_in_provider_cfg(provider_cfg, &secret_ref.id) {
        return None;
    }
    let command_path = provider_cfg.get("command").and_then(Value::as_str)?.trim();
    if command_path.is_empty() {
        return None;
    }
    let expanded_command = expand_home_path(command_path);
    if !expanded_command.is_absolute() {
        return None;
    }
    let allow_symlink_command = provider_cfg
        .get("allowSymlinkCommand")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Ok(meta) = fs::symlink_metadata(&expanded_command) {
        if meta.file_type().is_symlink() {
            if !allow_symlink_command {
                return None;
            }
            let trusted = read_trusted_dirs(provider_cfg);
            if !trusted.is_empty() {
                let Ok(canonical_command) = expanded_command.canonicalize() else {
                    return None;
                };
                let is_trusted = trusted.into_iter().any(|dir| {
                    dir.canonicalize()
                        .ok()
                        .is_some_and(|canonical_dir| canonical_command.starts_with(canonical_dir))
                });
                if !is_trusted {
                    return None;
                }
            }
        }
    }

    let args = provider_cfg
        .get("args")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let pass_env = provider_cfg
        .get("passEnv")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let json_only = provider_cfg
        .get("jsonOnly")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let timeout = provider_cfg
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .map(|ms| Duration::from_millis(ms.clamp(100, 120_000)))
        .or_else(|| {
            provider_cfg
                .get("timeoutSeconds")
                .or_else(|| provider_cfg.get("timeoutSec"))
                .or_else(|| provider_cfg.get("timeout"))
                .and_then(Value::as_u64)
                .map(|secs| Duration::from_secs(secs.clamp(1, 120)))
        })
        .unwrap_or_else(|| Duration::from_secs(10));

    let mut cmd = Command::new(expanded_command);
    cmd.args(args);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !pass_env.is_empty() {
        cmd.env_clear();
        for name in pass_env {
            if let Some(value) = env_lookup(&name) {
                cmd.env(name, value);
            }
        }
    }

    let mut child = cmd.spawn().ok()?;
    if let Some(stdin) = child.stdin.as_mut() {
        let payload = serde_json::json!({
            "protocolVersion": 1,
            "provider": provider_name,
            "ids": [secret_ref.id.clone()],
        });
        let _ = stdin.write_all(payload.to_string().as_bytes());
    }
    let _ = child.stdin.take();
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    if timed_out {
        return None;
    }
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<Value>(&stdout) {
        if let Some(value) = json
            .get("values")
            .and_then(Value::as_object)
            .and_then(|values| values.get(secret_ref.id.trim()))
        {
            let resolved = value
                .as_str()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    if value.is_number() || value.is_boolean() {
                        Some(value.to_string())
                    } else {
                        None
                    }
                });
            if resolved.is_some() {
                return resolved;
            }
        }
    }
    if json_only {
        return None;
    }
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == secret_ref.id.trim() {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    if secret_ref.id.trim() == "value" {
        let trimmed = stdout.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub(crate) fn resolve_secret_ref_with_provider_config(
    secret_ref: &SecretRef,
    cfg: &Value,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let source = secret_ref.source.trim().to_ascii_lowercase();
    if source.is_empty() {
        return None;
    }
    if source == "env" {
        return env_lookup(secret_ref.id.trim());
    }

    let provider_name = normalize_secret_provider_name(cfg, secret_ref)?;
    let provider_cfg = load_secret_provider_config(cfg, &provider_name)?;

    match source.as_str() {
        "file" => resolve_secret_ref_file_with_provider_config(secret_ref, provider_cfg),
        "exec" => resolve_secret_ref_exec_with_provider_config(
            secret_ref,
            &provider_name,
            provider_cfg,
            env_lookup,
        ),
        _ => None,
    }
}

pub(crate) fn resolve_secret_ref_with_env(
    secret_ref: &SecretRef,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    match secret_ref.source.as_str() {
        "env" => env_lookup(&secret_ref.id),
        "file" => resolve_secret_ref_file(&secret_ref.id),
        _ => None, // "exec" requires trusted binary + provider config, not supported here
    }
}

pub(crate) fn resolve_secret_ref_file(path_str: &str) -> Option<String> {
    let path = std::path::Path::new(path_str);
    if !path.is_absolute() {
        eprintln!("SecretRef file source: ignoring non-absolute path '{path_str}'");
        return None;
    }
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub(crate) fn local_env_lookup(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn collect_secret_ref_env_names_from_entry(entry: &Value, names: &mut Vec<String>) {
    for ref_field in [
        "secretRef",
        "keyRef",
        "tokenRef",
        "apiKeyRef",
        "api_key_ref",
        "accessRef",
    ] {
        if let Some(sr) = entry.get(ref_field).and_then(try_parse_secret_ref) {
            if sr.source.eq_ignore_ascii_case("env") {
                names.push(sr.id);
            }
        }
    }
    for field in ["token", "key", "apiKey", "api_key", "access"] {
        if let Some(field_val) = entry.get(field) {
            if let Some(sr) = try_parse_secret_ref(field_val) {
                if sr.source.eq_ignore_ascii_case("env") {
                    names.push(sr.id);
                }
            }
        }
    }
}

pub(crate) fn collect_secret_ref_env_names_from_auth_store(data: &Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(profiles) = data.get("profiles").and_then(Value::as_object) {
        for entry in profiles.values() {
            collect_secret_ref_env_names_from_entry(entry, &mut names);
        }
    }
    if let Some(root_obj) = data.as_object() {
        for (key, entry) in root_obj {
            if key != "profiles" && key != "version" {
                collect_secret_ref_env_names_from_entry(entry, &mut names);
            }
        }
    }
    names
}

/// Extract the actual key/token from an agent auth-profiles entry.
/// Handles different auth types: token, api_key, oauth, and SecretRef objects.
#[allow(dead_code)]
pub(crate) fn extract_credential_from_auth_entry(entry: &Value) -> Option<InternalProviderCredential> {
    extract_credential_from_auth_entry_with_env(entry, &local_env_lookup)
}

pub(crate) fn extract_credential_from_auth_entry_with_env(
    entry: &Value,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<InternalProviderCredential> {
    let auth_type = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let provider = entry
        .get("provider")
        .or_else(|| entry.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let kind_from_type = match auth_type.as_str() {
        "oauth" | "token" | "authorization" => Some(InternalAuthKind::Authorization),
        "api_key" | "api-key" | "apikey" => Some(InternalAuthKind::ApiKey),
        _ => None,
    };

    // SecretRef at entry level takes precedence (OpenClaw secrets management).
    for (ref_field, ref_kind) in [
        ("secretRef", kind_from_type),
        ("keyRef", Some(InternalAuthKind::ApiKey)),
        ("tokenRef", Some(InternalAuthKind::Authorization)),
        ("apiKeyRef", Some(InternalAuthKind::ApiKey)),
        ("api_key_ref", Some(InternalAuthKind::ApiKey)),
        ("accessRef", Some(InternalAuthKind::Authorization)),
    ] {
        if let Some(secret_ref) = entry.get(ref_field).and_then(try_parse_secret_ref) {
            if let Some(resolved) = resolve_secret_ref_with_env(&secret_ref, env_lookup) {
                let kind = infer_auth_kind(
                    provider,
                    &resolved,
                    ref_kind.unwrap_or(InternalAuthKind::ApiKey),
                );
                return Some(InternalProviderCredential {
                    secret: resolved,
                    kind,
                });
            }
        }
    }

    // "token" type → "token" field (e.g. anthropic)
    // "api_key" type → "key" field (e.g. kimi-coding)
    // "oauth" type → "access" field (e.g. minimax-portal, openai-codex)
    for field in ["token", "key", "apiKey", "api_key", "access"] {
        if let Some(field_val) = entry.get(field) {
            // Plaintext string value.
            if let Some(val) = field_val.as_str() {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let fallback_kind = match field {
                        "token" | "access" => InternalAuthKind::Authorization,
                        _ => InternalAuthKind::ApiKey,
                    };
                    let kind =
                        infer_auth_kind(provider, trimmed, kind_from_type.unwrap_or(fallback_kind));
                    return Some(InternalProviderCredential {
                        secret: trimmed.to_string(),
                        kind,
                    });
                }
            }
            // SecretRef object in credential field (OpenClaw secrets management).
            if let Some(secret_ref) = try_parse_secret_ref(field_val) {
                if let Some(resolved) = resolve_secret_ref_with_env(&secret_ref, env_lookup) {
                    let fallback_kind = match field {
                        "token" | "access" => InternalAuthKind::Authorization,
                        _ => InternalAuthKind::ApiKey,
                    };
                    let kind = infer_auth_kind(
                        provider,
                        &resolved,
                        kind_from_type.unwrap_or(fallback_kind),
                    );
                    return Some(InternalProviderCredential {
                        secret: resolved,
                        kind,
                    });
                }
            }
        }
    }
    None
}

pub(crate) fn mask_api_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return "not set".to_string();
    }
    if key.len() <= 8 {
        return "***".to_string();
    }
    let prefix = &key[..4.min(key.len())];
    let suffix = &key[key.len().saturating_sub(4)..];
    format!("{prefix}...{suffix}")
}

pub(crate) fn is_valid_env_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

mod secret_ref_tests {
    use super::*;

    #[test]
    fn try_parse_secret_ref_parses_valid_env_ref() {
        let val = serde_json::json!({ "source": "env", "id": "ANTHROPIC_API_KEY" });
        let sr = try_parse_secret_ref(&val).expect("should parse");
        assert_eq!(sr.source, "env");
        assert_eq!(sr.id, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn try_parse_secret_ref_parses_valid_file_ref() {
        let val = serde_json::json!({ "source": "file", "provider": "filemain", "id": "/tmp/secret.txt" });
        let sr = try_parse_secret_ref(&val).expect("should parse");
        assert_eq!(sr.source, "file");
        assert_eq!(sr.id, "/tmp/secret.txt");
    }

    #[test]
    fn try_parse_secret_ref_returns_none_for_plain_string() {
        let val = serde_json::json!("sk-ant-plaintext");
        assert!(try_parse_secret_ref(&val).is_none());
    }

    #[test]
    fn try_parse_secret_ref_returns_none_for_missing_source() {
        let val = serde_json::json!({ "id": "SOME_KEY" });
        assert!(try_parse_secret_ref(&val).is_none());
    }

    #[test]
    fn try_parse_secret_ref_returns_none_for_missing_id() {
        let val = serde_json::json!({ "source": "env" });
        assert!(try_parse_secret_ref(&val).is_none());
    }

    #[test]
    fn extract_credential_resolves_env_secret_ref_in_key_field() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "kimi-coding",
            "key": { "source": "env", "id": "KIMI_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "KIMI_API_KEY" {
                Some("sk-resolved-kimi".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-resolved-kimi");
        assert_eq!(credential.kind, InternalAuthKind::ApiKey);
    }

    #[test]
    fn extract_credential_resolves_env_secret_ref_in_key_ref_field() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "openai",
            "keyRef": { "source": "env", "id": "OPENAI_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "OPENAI_API_KEY" {
                Some("sk-keyref-openai".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-keyref-openai");
        assert_eq!(credential.kind, InternalAuthKind::ApiKey);
    }

    #[test]
    fn extract_credential_resolves_env_secret_ref_in_token_field() {
        let entry = serde_json::json!({
            "type": "token",
            "provider": "anthropic",
            "token": { "source": "env", "id": "ANTHROPIC_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "ANTHROPIC_API_KEY" {
                Some("sk-ant-resolved".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-ant-resolved");
        assert_eq!(credential.kind, InternalAuthKind::Authorization);
    }

    #[test]
    fn extract_credential_resolves_env_secret_ref_in_token_ref_field() {
        let entry = serde_json::json!({
            "type": "token",
            "provider": "anthropic",
            "tokenRef": { "source": "env", "id": "ANTHROPIC_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "ANTHROPIC_API_KEY" {
                Some("sk-ant-tokenref".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-ant-tokenref");
        assert_eq!(credential.kind, InternalAuthKind::Authorization);
    }

    #[test]
    fn extract_credential_resolves_top_level_secret_ref() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "openai",
            "secretRef": { "source": "env", "id": "OPENAI_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "OPENAI_API_KEY" {
                Some("sk-openai-resolved".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-openai-resolved");
        assert_eq!(credential.kind, InternalAuthKind::ApiKey);
    }

    #[test]
    fn top_level_secret_ref_takes_precedence_over_plaintext_field() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "openai",
            "key": "sk-plaintext-stale",
            "secretRef": { "source": "env", "id": "OPENAI_API_KEY" }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "OPENAI_API_KEY" {
                Some("sk-ref-fresh".to_string())
            } else {
                None
            }
        };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-ref-fresh");
    }

    #[test]
    fn falls_back_to_plaintext_when_secret_ref_env_unresolved() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "openai",
            "key": "sk-plaintext-fallback",
            "secretRef": { "source": "env", "id": "MISSING_VAR" }
        });
        let env_lookup = |_: &str| -> Option<String> { None };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup)
            .expect("should resolve");
        assert_eq!(credential.secret, "sk-plaintext-fallback");
    }

    #[test]
    fn resolve_key_from_auth_store_with_env_resolves_secret_ref() {
        let store = serde_json::json!({
            "version": 1,
            "profiles": {
                "anthropic:default": {
                    "type": "token",
                    "provider": "anthropic",
                    "token": { "source": "env", "id": "ANTHROPIC_API_KEY" }
                }
            }
        });
        let env_lookup = |name: &str| -> Option<String> {
            if name == "ANTHROPIC_API_KEY" {
                Some("sk-ant-from-env".to_string())
            } else {
                None
            }
        };
        let key =
            resolve_key_from_auth_store_json_with_env(&store, "anthropic:default", &env_lookup);
        assert_eq!(key, Some("sk-ant-from-env".to_string()));
    }

    #[test]
    fn collect_secret_ref_env_names_finds_names_from_profiles_and_root() {
        let store = serde_json::json!({
            "version": 1,
            "profiles": {
                "anthropic:default": {
                    "type": "token",
                    "provider": "anthropic",
                    "token": { "source": "env", "id": "ANTHROPIC_API_KEY" }
                },
                "openai:default": {
                    "type": "api_key",
                    "provider": "openai",
                    "secretRef": { "source": "env", "id": "OPENAI_API_KEY" }
                }
            }
        });
        let mut names = collect_secret_ref_env_names_from_auth_store(&store);
        names.sort();
        assert_eq!(names, vec!["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]);
    }

    #[test]
    fn collect_secret_ref_env_names_includes_keyref_and_tokenref_fields() {
        let store = serde_json::json!({
            "version": 1,
            "profiles": {
                "openai:default": {
                    "type": "api_key",
                    "provider": "openai",
                    "keyRef": { "source": "env", "id": "OPENAI_API_KEY" }
                },
                "anthropic:default": {
                    "type": "token",
                    "provider": "anthropic",
                    "tokenRef": { "source": "env", "id": "ANTHROPIC_API_KEY" }
                }
            }
        });
        let mut names = collect_secret_ref_env_names_from_auth_store(&store);
        names.sort();
        assert_eq!(names, vec!["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]);
    }

    #[test]
    fn resolve_secret_ref_file_reads_file_content() {
        let tmp =
            std::env::temp_dir().join(format!("clawpal-secretref-file-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let secret_file = tmp.join("api-key.txt");
        fs::write(&secret_file, "  sk-from-file\n").expect("write secret file");

        let resolved = resolve_secret_ref_file(secret_file.to_str().unwrap());
        assert_eq!(resolved, Some("sk-from-file".to_string()));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn resolve_secret_ref_file_returns_none_for_missing_file() {
        assert!(resolve_secret_ref_file("/nonexistent/path/secret.txt").is_none());
    }

    #[test]
    fn resolve_secret_ref_file_returns_none_for_relative_path() {
        assert!(resolve_secret_ref_file("relative/secret.txt").is_none());
    }

    #[test]
    fn resolve_secret_ref_with_provider_config_reads_file_json_pointer() {
        let tmp = std::env::temp_dir().join(format!(
            "clawpal-secretref-provider-file-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let secret_file = tmp.join("provider-secrets.json");
        fs::write(
            &secret_file,
            r#"{"providers":{"openai":{"api_key":"sk-file-provider"}}}"#,
        )
        .expect("write provider secret json");

        let cfg = serde_json::json!({
            "secrets": {
                "defaults": { "file": "file-main" },
                "providers": {
                    "file-main": {
                        "source": "file",
                        "path": secret_file.to_string_lossy().to_string(),
                        "mode": "json"
                    }
                }
            }
        });
        let secret_ref = SecretRef {
            source: "file".to_string(),
            provider: None,
            id: "/providers/openai/api_key".to_string(),
        };
        let env_lookup = |_: &str| -> Option<String> { None };
        let resolved = resolve_secret_ref_with_provider_config(&secret_ref, &cfg, &env_lookup);
        assert_eq!(resolved.as_deref(), Some("sk-file-provider"));

        let _ = fs::remove_dir_all(tmp);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_secret_ref_with_provider_config_runs_exec_provider() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join(format!(
            "clawpal-secretref-provider-exec-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let exec_file = tmp.join("secret-provider.sh");
        fs::write(
            &exec_file,
            "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"values\":{\"my-api-key\":\"sk-from-exec-provider\"}}'\n",
        )
        .expect("write exec script");
        let mut perms = fs::metadata(&exec_file)
            .expect("exec metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exec_file, perms).expect("chmod");

        let cfg = serde_json::json!({
            "secrets": {
                "defaults": { "exec": "vault-cli" },
                "providers": {
                    "vault-cli": {
                        "source": "exec",
                        "command": exec_file.to_string_lossy().to_string(),
                        "jsonOnly": true
                    }
                }
            }
        });
        let secret_ref = SecretRef {
            source: "exec".to_string(),
            provider: None,
            id: "my-api-key".to_string(),
        };
        let env_lookup = |_: &str| -> Option<String> { None };
        let resolved = resolve_secret_ref_with_provider_config(&secret_ref, &cfg, &env_lookup);
        assert_eq!(resolved.as_deref(), Some("sk-from-exec-provider"));

        let _ = fs::remove_dir_all(tmp);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_secret_ref_with_provider_config_exec_times_out() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join(format!(
            "clawpal-secretref-provider-exec-timeout-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let exec_file = tmp.join("secret-provider-timeout.sh");
        fs::write(
            &exec_file,
            "#!/bin/sh\ncat >/dev/null\nsleep 2\nprintf '%s' '{\"values\":{\"my-api-key\":\"sk-too-late\"}}'\n",
        )
        .expect("write exec script");
        let mut perms = fs::metadata(&exec_file)
            .expect("exec metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exec_file, perms).expect("chmod");

        let cfg = serde_json::json!({
            "secrets": {
                "defaults": { "exec": "vault-cli" },
                "providers": {
                    "vault-cli": {
                        "source": "exec",
                        "command": exec_file.to_string_lossy().to_string(),
                        "jsonOnly": true,
                        "timeoutSec": 1
                    }
                }
            }
        });
        let secret_ref = SecretRef {
            source: "exec".to_string(),
            provider: None,
            id: "my-api-key".to_string(),
        };
        let env_lookup = |_: &str| -> Option<String> { None };
        let resolved = resolve_secret_ref_with_provider_config(&secret_ref, &cfg, &env_lookup);
        assert!(resolved.is_none());

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn exec_source_secret_ref_is_not_resolved() {
        let entry = serde_json::json!({
            "type": "api_key",
            "provider": "vault",
            "key": { "source": "exec", "provider": "vault", "id": "my-api-key" }
        });
        let env_lookup = |_: &str| -> Option<String> { None };
        let credential = extract_credential_from_auth_entry_with_env(&entry, &env_lookup);
        assert!(credential.is_none());
    }
}
