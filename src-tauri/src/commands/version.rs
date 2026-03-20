use super::*;

pub(crate) fn extract_version_from_text(input: &str) -> Option<String> {
    let re = regex::Regex::new(r"\d+\.\d+(?:\.\d+){1,3}(?:[-+._a-zA-Z0-9]*)?").ok()?;
    re.find(input).map(|mat| mat.as_str().to_string())
}

pub(crate) fn compare_semver(installed: &str, latest: Option<&str>) -> bool {
    let installed = normalize_semver_components(installed);
    let latest = latest.and_then(normalize_semver_components);
    let (mut installed, mut latest) = match (installed, latest) {
        (Some(installed), Some(latest)) => (installed, latest),
        _ => return false,
    };

    let len = installed.len().max(latest.len());
    while installed.len() < len {
        installed.push(0);
    }
    while latest.len() < len {
        latest.push(0);
    }
    installed < latest
}

pub(crate) fn normalize_semver_components(raw: &str) -> Option<Vec<u32>> {
    let mut parts = Vec::new();
    for bit in raw.split('.') {
        let filtered = bit.trim_start_matches(|c: char| c == 'v' || c == 'V');
        let head = filtered
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("");
        if head.is_empty() {
            continue;
        }
        parts.push(head.parse::<u32>().ok()?);
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts)
}

pub(crate) fn normalize_openclaw_release_tag(raw: &str) -> Option<String> {
    extract_version_from_text(raw).or_else(|| {
        let trimmed = raw.trim().trim_start_matches(['v', 'V']);
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn query_openclaw_latest_github_release() -> Result<Option<String>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("ClawPal Update Checker (+https://github.com/zhixianio/clawpal)")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let resp = client
        .get("https://api.github.com/repos/openclaw/openclaw/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("GitHub releases request failed: {e}"))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: Value = resp
        .json()
        .map_err(|e| format!("GitHub releases parse failed: {e}"))?;
    let version = body
        .get("tag_name")
        .and_then(Value::as_str)
        .and_then(normalize_openclaw_release_tag)
        .or_else(|| {
            body.get("name")
                .and_then(Value::as_str)
                .and_then(normalize_openclaw_release_tag)
        });
    Ok(version)
}

pub(crate) fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |delta| delta.as_secs())
}

pub(crate) fn format_timestamp_from_unix(timestamp: u64) -> String {
    let Some(utc) = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp as i64, 0) else {
        return "unknown".into();
    };
    utc.to_rfc3339()
}

pub(crate) fn openclaw_update_cache_path(paths: &crate::models::OpenClawPaths) -> PathBuf {
    paths.clawpal_dir.join("openclaw-update-cache.json")
}

pub(crate) fn read_openclaw_update_cache(path: &Path) -> Option<OpenclawUpdateCache> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<OpenclawUpdateCache>(&text).ok()
}

pub(crate) fn save_openclaw_update_cache(
    path: &Path,
    cache: &OpenclawUpdateCache,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(cache).map_err(|error| error.to_string())?;
    write_text(path, &text)
}

pub(crate) fn check_openclaw_update_cached(
    paths: &crate::models::OpenClawPaths,
    force: bool,
) -> Result<OpenclawUpdateCheck, String> {
    let installed_version = resolve_openclaw_version();
    let cache_path = openclaw_update_cache_path(paths);
    let mut cache = resolve_openclaw_latest_release_cached(paths, force).unwrap_or_else(|_| {
        OpenclawUpdateCache {
            checked_at: unix_timestamp_secs(),
            latest_version: None,
            channel: None,
            details: Some("failed to detect latest GitHub release".into()),
            source: "github-release".into(),
            installed_version: None,
            ttl_seconds: 60 * 60 * 6,
        }
    });
    if cache.installed_version.as_deref() != Some(installed_version.as_str()) {
        cache.installed_version = Some(installed_version.clone());
        save_openclaw_update_cache(&cache_path, &cache)?;
    }
    let upgrade = compare_semver(&installed_version, cache.latest_version.as_deref());
    Ok(OpenclawUpdateCheck {
        installed_version,
        latest_version: cache.latest_version,
        upgrade_available: upgrade,
        channel: cache.channel,
        details: cache.details,
        source: cache.source,
        checked_at: format_timestamp_from_unix(cache.checked_at),
    })
}

pub(crate) fn resolve_openclaw_latest_release_cached(
    paths: &crate::models::OpenClawPaths,
    force: bool,
) -> Result<OpenclawUpdateCache, String> {
    let cache_path = openclaw_update_cache_path(paths);
    let now = unix_timestamp_secs();
    let existing = read_openclaw_update_cache(&cache_path);
    if !force {
        if let Some(cached) = existing.as_ref() {
            if now.saturating_sub(cached.checked_at) < cached.ttl_seconds {
                return Ok(cached.clone());
            }
        }
    }

    match query_openclaw_latest_github_release() {
        Ok(latest_version) => {
            let cache = OpenclawUpdateCache {
                checked_at: now,
                latest_version: latest_version.clone(),
                channel: None,
                details: latest_version
                    .as_ref()
                    .map(|value| format!("GitHub release {value}"))
                    .or_else(|| Some("GitHub release unavailable".into())),
                source: "github-release".into(),
                installed_version: existing.and_then(|cache| cache.installed_version),
                ttl_seconds: 60 * 60 * 6,
            };
            save_openclaw_update_cache(&cache_path, &cache)?;
            Ok(cache)
        }
        Err(error) => {
            if let Some(cached) = existing {
                Ok(cached)
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod openclaw_update_tests {
    use super::normalize_openclaw_release_tag;

    #[test]
    fn normalize_openclaw_release_tag_extracts_semver_from_github_tag() {
        assert_eq!(
            normalize_openclaw_release_tag("v2026.3.2"),
            Some("2026.3.2".into())
        );
        assert_eq!(
            normalize_openclaw_release_tag("OpenClaw v2026.3.2"),
            Some("2026.3.2".into())
        );
        assert_eq!(
            normalize_openclaw_release_tag("2026.3.2-rc.1"),
            Some("2026.3.2-rc.1".into())
        );
    }
}
