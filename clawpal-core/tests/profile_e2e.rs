//! E2E tests: create profiles for all supported models, persist them, and
//! verify the API key works with a real provider probe.
//!
//! Each test case carries its own env-var guard:
//!   - ANTHROPIC_API_KEY  → Anthropic models
//!   - OPENAI_API_KEY     → OpenAI models
//!   - GEMINI_API_KEY     → Google Gemini models
//!   - GROQ_API_KEY       → Groq models
//!   - MISTRAL_API_KEY    → Mistral models
//!
//! When a key is absent the corresponding cases are **skipped**, but a
//! structured summary is always printed so CI logs make coverage gaps explicit.

use std::fs;
use std::sync::Mutex;

use clawpal_core::openclaw::OpenclawCli;
use clawpal_core::profile::{self, ModelProfile};
use uuid::Uuid;

// ── shared lock so parallel tests don't race on CLAWPAL_DATA_DIR ──────────
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── model matrix ─────────────────────────────────────────────────────────

struct ModelCase {
    provider: &'static str,
    model: &'static str,
    env_var: &'static str,
    probe_url: &'static str,
    auth_header: &'static str,
    auth_prefix: &'static str,
}

const MODELS: &[ModelCase] = &[
    // ── Anthropic ──────────────────────────────────────────────────
    ModelCase {
        provider: "anthropic",
        model: "claude-opus-4-20250514",
        env_var: "ANTHROPIC_API_KEY",
        probe_url: "https://api.anthropic.com/v1/messages",
        auth_header: "x-api-key",
        auth_prefix: "",
    },
    ModelCase {
        provider: "anthropic",
        model: "claude-sonnet-4-20250514",
        env_var: "ANTHROPIC_API_KEY",
        probe_url: "https://api.anthropic.com/v1/messages",
        auth_header: "x-api-key",
        auth_prefix: "",
    },
    ModelCase {
        provider: "anthropic",
        model: "claude-haiku-4-20250514",
        env_var: "ANTHROPIC_API_KEY",
        probe_url: "https://api.anthropic.com/v1/messages",
        auth_header: "x-api-key",
        auth_prefix: "",
    },
    // ── OpenAI ────────────────────────────────────────────────────
    ModelCase {
        provider: "openai",
        model: "gpt-4o",
        env_var: "OPENAI_API_KEY",
        probe_url: "https://api.openai.com/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    ModelCase {
        provider: "openai",
        model: "gpt-4o-mini",
        env_var: "OPENAI_API_KEY",
        probe_url: "https://api.openai.com/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    ModelCase {
        provider: "openai",
        model: "o3",
        env_var: "OPENAI_API_KEY",
        probe_url: "https://api.openai.com/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    // ── Google Gemini ─────────────────────────────────────────────
    ModelCase {
        provider: "google",
        model: "gemini-2.5-pro",
        env_var: "GEMINI_API_KEY",
        probe_url: "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent",
        auth_header: "x-goog-api-key",
        auth_prefix: "",
    },
    ModelCase {
        provider: "google",
        model: "gemini-2.5-flash",
        env_var: "GEMINI_API_KEY",
        probe_url: "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent",
        auth_header: "x-goog-api-key",
        auth_prefix: "",
    },
    // ── Groq ─────────────────────────────────────────────────────
    ModelCase {
        provider: "groq",
        model: "llama-3.3-70b-versatile",
        env_var: "GROQ_API_KEY",
        probe_url: "https://api.groq.com/openai/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    ModelCase {
        provider: "groq",
        model: "moonshotai/kimi-k2-instruct",
        env_var: "GROQ_API_KEY",
        probe_url: "https://api.groq.com/openai/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    // ── Mistral ───────────────────────────────────────────────────
    ModelCase {
        provider: "mistral",
        model: "mistral-large-latest",
        env_var: "MISTRAL_API_KEY",
        probe_url: "https://api.mistral.ai/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
    ModelCase {
        provider: "mistral",
        model: "codestral-latest",
        env_var: "MISTRAL_API_KEY",
        probe_url: "https://api.mistral.ai/v1/chat/completions",
        auth_header: "Authorization",
        auth_prefix: "Bearer ",
    },
];

// ── probe helpers ─────────────────────────────────────────────────────────

fn build_probe_body(case: &ModelCase) -> serde_json::Value {
    match case.provider {
        "anthropic" => serde_json::json!({
            "model": case.model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}]
        }),
        "google" => serde_json::json!({
            "contents": [{"parts": [{"text": "ping"}]}],
            "generationConfig": {"maxOutputTokens": 1}
        }),
        _ => serde_json::json!({
            "model": case.model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}]
        }),
    }
}

fn probe_model(case: &ModelCase, api_key: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    // Google uses a query-param key; others use a header
    let url = if case.provider == "google" {
        format!("{}?key={}", case.probe_url, api_key.trim())
    } else {
        case.probe_url.to_string()
    };

    let auth_value = format!("{}{}", case.auth_prefix, api_key.trim());

    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&build_probe_body(case));

    if case.provider != "google" {
        req = req.header(case.auth_header, &auth_value);
    }
    if case.provider == "anthropic" {
        req = req.header("anthropic-version", "2023-06-01");
    }

    let resp = req.send().map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) || status == 429 {
        // 429 means the API key is valid but rate-limited — treat as success.
        return Ok(());
    }
    let body = resp.text().unwrap_or_default();
    Err(format!("probe failed (HTTP {status}): {body}"))
}

// ── temp dir ──────────────────────────────────────────────────────────────

fn temp_data_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("clawpal-core-profile-e2e-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

// ── per-case runner ───────────────────────────────────────────────────────

#[derive(Debug)]
enum CaseResult {
    Passed,
    Skipped { reason: String },
    Failed { error: String },
}

fn run_case(case: &ModelCase) -> CaseResult {
    let api_key = match std::env::var(case.env_var) {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            return CaseResult::Skipped {
                reason: format!("{} not set", case.env_var),
            }
        }
    };

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let data_dir = temp_data_dir();
    // SAFETY: serialised via ENV_LOCK
    unsafe { std::env::set_var("CLAWPAL_DATA_DIR", &data_dir) };

    let mp = ModelProfile {
        id: String::new(),
        name: String::new(),
        provider: case.provider.to_string(),
        model: case.model.to_string(),
        auth_ref: case.env_var.to_string(),
        api_key: Some(api_key.clone()),
        base_url: None,
        description: Some(format!("E2E — {}/{}", case.provider, case.model)),
        sync_source_device_name: None,
        sync_source_host_id: None,
        sync_synced_at: None,
        enabled: true,
    };

    let cli = OpenclawCli::with_bin("__unused__".to_string());

    // 1. upsert
    let saved = match profile::upsert_profile(&cli, mp) {
        Ok(p) => p,
        Err(e) => {
            return CaseResult::Failed {
                error: format!("upsert_profile: {e}"),
            }
        }
    };
    if saved.id.is_empty() {
        return CaseResult::Failed {
            error: "profile id empty after upsert".into(),
        };
    }
    if saved.provider != case.provider || saved.model != case.model {
        return CaseResult::Failed {
            error: format!(
                "unexpected provider/model: got {}/{}",
                saved.provider, saved.model
            ),
        };
    }

    // 2. persistence round-trip
    match profile::list_profiles(&cli) {
        Ok(ps) if ps.iter().any(|p| p.id == saved.id) => {}
        Ok(_) => {
            return CaseResult::Failed {
                error: "saved profile missing from list".into(),
            }
        }
        Err(e) => {
            return CaseResult::Failed {
                error: format!("list_profiles: {e}"),
            }
        }
    }

    // 3. real API probe
    if let Err(e) = probe_model(case, &api_key) {
        return CaseResult::Failed {
            error: format!("API probe: {e}"),
        };
    }

    CaseResult::Passed
}

// ── test entry-point ──────────────────────────────────────────────────────

#[test]
fn e2e_all_model_profiles() {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;
    let mut lines: Vec<String> = Vec::new();

    for case in MODELS {
        let label = format!("{}/{}", case.provider, case.model);
        match run_case(case) {
            CaseResult::Passed => {
                passed += 1;
                lines.push(format!("  ✅  {label}"));
            }
            CaseResult::Skipped { reason } => {
                skipped += 1;
                lines.push(format!("  ⏭   {label}  [skipped: {reason}]"));
            }
            CaseResult::Failed { error } => {
                failed += 1;
                lines.push(format!("  ❌  {label}  [FAILED: {error}]"));
            }
        }
    }

    // Always emit the full matrix — makes CI log gaps self-explaining
    println!("\n── Profile E2E Summary ─────────────────────────────────────────");
    println!(
        "  {} passed  |  {} skipped  |  {} failed  (total: {})",
        passed,
        skipped,
        failed,
        MODELS.len()
    );
    println!("────────────────────────────────────────────────────────────────");
    for line in &lines {
        println!("{line}");
    }
    println!("────────────────────────────────────────────────────────────────\n");

    if passed == 0 && failed == 0 {
        eprintln!(
            "WARNING: all {} cases skipped — set at least one API key env var to run probes",
            skipped
        );
    }

    assert_eq!(
        failed, 0,
        "{failed} profile e2e case(s) failed — see summary above"
    );
}
