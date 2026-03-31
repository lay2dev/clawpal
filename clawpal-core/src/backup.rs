use regex::Regex;

// ---- Workspace Git Backup ----

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitStatus {
    /// Whether the workspace directory is a git repository
    pub is_git_repo: bool,
    /// Whether a remote named "origin" is configured
    pub has_remote: bool,
    /// The remote URL (if any)
    pub remote_url: Option<String>,
    /// Current branch name
    pub branch: Option<String>,
    /// Number of uncommitted changes (staged + unstaged + untracked)
    pub uncommitted_count: u32,
    /// Number of commits ahead of the remote tracking branch
    pub ahead: u32,
    /// Number of commits behind the remote tracking branch
    pub behind: u32,
    /// Last commit timestamp (ISO 8601) if any
    pub last_commit_time: Option<String>,
    /// Last commit message (first line)
    pub last_commit_message: Option<String>,
}

/// The default `.gitignore` content recommended for OpenClaw workspaces.
pub const WORKSPACE_GITIGNORE: &str = "\
.DS_Store
.env
**/*.key
**/*.pem
**/secrets*
";

/// Parse the output of the combined git-status probe command.
///
/// Expected input format (one command producing multiple tagged lines):
/// ```text
/// GIT_REPO:true
/// BRANCH:main
/// REMOTE_URL:https://github.com/user/openclaw-workspace.git
/// UNCOMMITTED:3
/// AHEAD:1
/// BEHIND:0
/// LAST_COMMIT_TIME:2026-03-15T10:30:00+00:00
/// LAST_COMMIT_MSG:Update memory
/// ```
pub fn parse_workspace_git_status(output: &str) -> WorkspaceGitStatus {
    let mut status = WorkspaceGitStatus {
        is_git_repo: false,
        has_remote: false,
        remote_url: None,
        branch: None,
        uncommitted_count: 0,
        ahead: 0,
        behind: 0,
        last_commit_time: None,
        last_commit_message: None,
    };

    for line in output.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("GIT_REPO:") {
            status.is_git_repo = val.trim() == "true";
        } else if let Some(val) = line.strip_prefix("BRANCH:") {
            let val = val.trim();
            if !val.is_empty() {
                status.branch = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("REMOTE_URL:") {
            let val = val.trim();
            if !val.is_empty() {
                status.has_remote = true;
                status.remote_url = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("UNCOMMITTED:") {
            status.uncommitted_count = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("AHEAD:") {
            status.ahead = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("BEHIND:") {
            status.behind = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("LAST_COMMIT_TIME:") {
            let val = val.trim();
            if !val.is_empty() {
                status.last_commit_time = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("LAST_COMMIT_MSG:") {
            let val = val.trim();
            if !val.is_empty() {
                status.last_commit_message = Some(val.to_string());
            }
        }
    }

    status
}

/// Build the shell command that probes workspace git status.
/// `workspace_path` should be a shell-safe absolute or `$HOME`-relative path.
pub fn build_git_status_probe_cmd(workspace_path: &str) -> String {
    format!(
        concat!(
            "cd {ws} 2>/dev/null || {{ echo 'GIT_REPO:false'; exit 0; }}; ",
            "if [ ! -d .git ] && ! git rev-parse --git-dir >/dev/null 2>&1; then ",
            "echo 'GIT_REPO:false'; exit 0; fi; ",
            "echo 'GIT_REPO:true'; ",
            "echo \"BRANCH:$(git rev-parse --abbrev-ref HEAD 2>/dev/null)\"; ",
            "echo \"REMOTE_URL:$(git remote get-url origin 2>/dev/null)\"; ",
            "echo \"UNCOMMITTED:$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')\"; ",
            "TRACKING=$(git rev-parse --abbrev-ref '@{{u}}' 2>/dev/null); ",
            "if [ -n \"$TRACKING\" ]; then ",
            "echo \"AHEAD:$(git rev-list --count '@{{u}}..HEAD' 2>/dev/null || echo 0)\"; ",
            "echo \"BEHIND:$(git rev-list --count 'HEAD..@{{u}}' 2>/dev/null || echo 0)\"; ",
            "else echo 'AHEAD:0'; echo 'BEHIND:0'; fi; ",
            "echo \"LAST_COMMIT_TIME:$(git log -1 --format='%aI' 2>/dev/null)\"; ",
            "echo \"LAST_COMMIT_MSG:$(git log -1 --format='%s' 2>/dev/null)\""
        ),
        ws = workspace_path
    )
}

/// Build the shell command that runs git add + commit + push.
pub fn build_git_backup_cmd(workspace_path: &str, message: &str) -> String {
    let safe_msg = message.replace('\'', "'\\''");
    format!(
        concat!(
            "cd {ws} || exit 1; ",
            "git add -A; ",
            "if git diff --cached --quiet 2>/dev/null; then ",
            "echo 'NOTHING_TO_COMMIT'; exit 0; fi; ",
            "git commit -m '{msg}'; ",
            "if git remote get-url origin >/dev/null 2>&1; then ",
            "git push 2>&1; echo 'PUSHED'; ",
            "else echo 'COMMITTED_NO_REMOTE'; fi"
        ),
        ws = workspace_path,
        msg = safe_msg
    )
}

/// Build the shell command that initializes a git repo in the workspace
/// and writes a `.gitignore` if one doesn't exist.
pub fn build_git_init_cmd(workspace_path: &str) -> String {
    let gitignore_content = WORKSPACE_GITIGNORE.replace('\n', "\\n");
    format!(
        concat!(
            "cd {ws} || exit 1; ",
            "if [ -d .git ] || git rev-parse --git-dir >/dev/null 2>&1; then ",
            "echo 'ALREADY_INITIALIZED'; exit 0; fi; ",
            "git init; ",
            "if [ ! -f .gitignore ]; then printf '{gitignore}' > .gitignore; fi; ",
            "git add -A; ",
            "git commit -m 'Initial workspace backup'; ",
            "echo 'INITIALIZED'"
        ),
        ws = workspace_path,
        gitignore = gitignore_content
    )
}

// ---- Copy-based Backup ----

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub size_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeResult {
    pub detected_versions: Vec<String>,
}

pub fn parse_backup_list(du_output: &str) -> Vec<BackupEntry> {
    du_output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() != 2 {
                return None;
            }
            let size_kb = parts[0].trim().parse::<u64>().ok().unwrap_or(0);
            let path = parts[1].trim().trim_end_matches('/').to_string();
            Some(BackupEntry {
                path,
                size_bytes: size_kb * 1024,
            })
        })
        .collect()
}

pub fn parse_backup_result(output: &str) -> BackupResult {
    let size_bytes = output
        .trim()
        .lines()
        .last()
        .and_then(|l| l.trim().parse::<u64>().ok())
        .unwrap_or(0);
    BackupResult { size_bytes }
}

pub fn parse_upgrade_result(output: &str) -> UpgradeResult {
    let mut versions = Vec::new();
    let re = Regex::new(r"openclaw\s+([0-9]+\.[0-9]+\.[0-9]+)").expect("regex");
    for cap in re.captures_iter(output) {
        if let Some(v) = cap.get(1) {
            let ver = v.as_str().to_string();
            if !versions.contains(&ver) {
                versions.push(ver);
            }
        }
    }
    UpgradeResult {
        detected_versions: versions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Workspace git status tests ----

    #[test]
    fn parse_git_status_full() {
        let output = "\
GIT_REPO:true
BRANCH:main
REMOTE_URL:https://github.com/user/workspace.git
UNCOMMITTED:5
AHEAD:2
BEHIND:1
LAST_COMMIT_TIME:2026-03-15T10:30:00+00:00
LAST_COMMIT_MSG:Update memory
";
        let s = parse_workspace_git_status(output);
        assert!(s.is_git_repo);
        assert!(s.has_remote);
        assert_eq!(
            s.remote_url.as_deref(),
            Some("https://github.com/user/workspace.git")
        );
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.uncommitted_count, 5);
        assert_eq!(s.ahead, 2);
        assert_eq!(s.behind, 1);
        assert_eq!(
            s.last_commit_time.as_deref(),
            Some("2026-03-15T10:30:00+00:00")
        );
        assert_eq!(s.last_commit_message.as_deref(), Some("Update memory"));
    }

    #[test]
    fn parse_git_status_not_a_repo() {
        let output = "GIT_REPO:false\n";
        let s = parse_workspace_git_status(output);
        assert!(!s.is_git_repo);
        assert!(!s.has_remote);
        assert_eq!(s.uncommitted_count, 0);
    }

    #[test]
    fn parse_git_status_no_remote() {
        let output = "\
GIT_REPO:true
BRANCH:main
REMOTE_URL:
UNCOMMITTED:0
AHEAD:0
BEHIND:0
LAST_COMMIT_TIME:2026-03-10T08:00:00+00:00
LAST_COMMIT_MSG:init
";
        let s = parse_workspace_git_status(output);
        assert!(s.is_git_repo);
        assert!(!s.has_remote);
        assert_eq!(s.remote_url, None);
    }

    #[test]
    fn parse_git_status_empty_input() {
        let s = parse_workspace_git_status("");
        assert!(!s.is_git_repo);
    }

    #[test]
    fn build_git_status_probe_cmd_uses_workspace_path() {
        let cmd = build_git_status_probe_cmd("$HOME/.openclaw/workspace");
        assert!(cmd.contains("cd $HOME/.openclaw/workspace"));
        assert!(cmd.contains("GIT_REPO:"));
    }

    #[test]
    fn build_git_backup_cmd_escapes_quotes_in_message() {
        let cmd = build_git_backup_cmd("/ws", "it's a test");
        assert!(cmd.contains("it'\\''s a test"));
    }

    #[test]
    fn build_git_init_cmd_contains_gitignore() {
        let cmd = build_git_init_cmd("/ws");
        assert!(cmd.contains(".gitignore"));
        assert!(cmd.contains("git init"));
    }

    // ---- Copy-based backup tests ----

    #[test]
    fn parse_backup_list_reads_du_lines() {
        let out = parse_backup_list("10\t/home/a\n0\t/home/b\n");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].size_bytes, 10 * 1024);
    }

    #[test]
    fn parse_backup_result_reads_last_line_number() {
        let out = parse_backup_result("log\n123\n");
        assert_eq!(out.size_bytes, 123);
    }

    #[test]
    fn parse_upgrade_result_extracts_versions() {
        let out = parse_upgrade_result("openclaw 0.2.0\nfoo\nopenclaw 0.3.1");
        assert_eq!(out.detected_versions, vec!["0.2.0", "0.3.1"]);
    }

    #[test]
    fn parse_backup_list_empty_input() {
        let out = parse_backup_list("");
        assert!(out.is_empty());
    }

    #[test]
    fn parse_backup_list_strips_trailing_slash() {
        let out = parse_backup_list("50\t/home/user/backup/\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "/home/user/backup");
        assert_eq!(out[0].size_bytes, 50 * 1024);
    }

    #[test]
    fn parse_backup_list_skips_malformed_lines() {
        let out = parse_backup_list("no tab here\n10\t/valid\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "/valid");
    }

    #[test]
    fn parse_backup_result_empty_input() {
        let out = parse_backup_result("");
        assert_eq!(out.size_bytes, 0);
    }

    #[test]
    fn parse_backup_result_non_numeric_last_line() {
        let out = parse_backup_result("done\ncomplete\n");
        assert_eq!(out.size_bytes, 0);
    }

    #[test]
    fn parse_upgrade_result_no_versions() {
        let out = parse_upgrade_result("nothing relevant here");
        assert!(out.detected_versions.is_empty());
    }

    #[test]
    fn parse_upgrade_result_deduplicates() {
        let out = parse_upgrade_result("openclaw 1.0.0\nupgraded\nopenclaw 1.0.0\nopenclaw 1.1.0");
        assert_eq!(out.detected_versions, vec!["1.0.0", "1.1.0"]);
    }

    #[test]
    fn parse_backup_list_zero_size() {
        let out = parse_backup_list("0\t/empty/dir\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].size_bytes, 0);
    }
}
