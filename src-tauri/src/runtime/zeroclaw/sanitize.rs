pub fn sanitize_output(raw: &str) -> String {
    // Strip ANSI escape/control fragments and zeroclaw tracing lines.
    let ansi_esc = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("valid ansi regex");
    let ansi_literal = regex::Regex::new(r"\[[0-9;]*m").expect("valid ansi literal regex");
    let escaped = ansi_esc.replace_all(raw, "");
    let cleaned = ansi_literal.replace_all(&escaped, "");
    let mut lines = Vec::<String>::new();
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("zeroclaw::") && (lower.contains(" info ") || lower.contains(" warn ")) {
            continue;
        }
        lines.push(trimmed.to_string());
    }
    lines.join("\n")
}
