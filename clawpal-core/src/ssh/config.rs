use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigHostSuggestion {
    pub host_alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

pub fn parse_ssh_config_hosts(data: &str) -> Vec<SshConfigHostSuggestion> {
    let mut out = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut host_name: Option<String> = None;
    let mut user: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut identity_file: Option<String> = None;

    for raw in data.lines() {
        let Some((key, value)) = parse_ssh_config_entry(raw) else {
            continue;
        };

        if key == "host" {
            if !aliases.is_empty() {
                push_ssh_config_hosts(&mut out, &aliases, &host_name, &user, &port, &identity_file);
            }
            aliases = split_host_aliases(&value)
                .into_iter()
                .filter(|v| !v.is_empty())
                .collect();
            host_name = None;
            user = None;
            port = None;
            identity_file = None;
            continue;
        }

        if aliases.is_empty() {
            continue;
        }

        match key.as_str() {
            "hostname" => {
                if host_name.is_none() {
                    host_name = Some(value.to_string());
                }
            }
            "user" => {
                if user.is_none() {
                    user = Some(value.to_string());
                }
            }
            "port" => {
                if port.is_none() {
                    port = value.parse::<u16>().ok();
                }
            }
            "identityfile" => {
                if identity_file.is_none() {
                    identity_file = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    if !aliases.is_empty() {
        push_ssh_config_hosts(&mut out, &aliases, &host_name, &user, &port, &identity_file);
    }

    let mut dedup = std::collections::BTreeMap::new();
    for entry in out {
        dedup.entry(entry.host_alias.clone()).or_insert(entry);
    }
    dedup.into_values().collect()
}

fn push_ssh_config_hosts(
    out: &mut Vec<SshConfigHostSuggestion>,
    aliases: &[String],
    host_name: &Option<String>,
    user: &Option<String>,
    port: &Option<u16>,
    identity_file: &Option<String>,
) {
    for alias in aliases {
        if alias.is_empty()
            || alias == "*"
            || alias.starts_with('!')
            || alias.contains('*')
            || alias.contains('?')
        {
            continue;
        }
        out.push(SshConfigHostSuggestion {
            host_alias: alias.clone(),
            host_name: host_name.clone(),
            user: user.clone(),
            port: *port,
            identity_file: identity_file.clone(),
        });
    }
}

fn parse_quoted_ssh_value(mut value: &str) -> String {
    value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn strip_ssh_comment(value: &str) -> String {
    let mut output = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            output.push(ch);
            escaped = true;
            continue;
        }

        if quote.is_some() {
            if quote == Some(ch) {
                quote = None;
            }
            output.push(ch);
            continue;
        }

        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            output.push(ch);
            continue;
        }

        if ch == '#' {
            break;
        }

        output.push(ch);
    }

    output.trim().to_string()
}

fn parse_ssh_config_entry(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let mut sep = None;
    let mut quote: Option<char> = None;

    for (idx, ch) in line.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }

        if matches!(ch, '"' | '\'') {
            quote = Some(ch);
            continue;
        }

        if ch == '=' {
            sep = Some((idx, true));
            break;
        }

        if ch.is_whitespace() {
            sep = Some((idx, false));
            break;
        }
    }

    let (sep_idx, is_eq) = sep?;

    let key = line[..sep_idx].trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }

    let raw_value = if is_eq {
        line[sep_idx + 1..].trim()
    } else {
        line[sep_idx..].trim()
    };
    if raw_value.is_empty() {
        return None;
    }

    let value = parse_quoted_ssh_value(&strip_ssh_comment(raw_value));
    if value.is_empty() {
        None
    } else {
        Some((key, value))
    }
}

fn split_host_aliases(value: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in value.trim().chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if let Some(q) = quote {
            if ch == q {
                quote = None;
                continue;
            }
            current.push(ch);
            continue;
        }

        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                aliases.push(current.clone());
                current.clear();
            }
            continue;
        }

        current.push(ch);
    }

    if !current.is_empty() {
        aliases.push(current);
    }

    aliases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ssh_config_hosts_extracts_aliases() {
        let input = r#"
Host my-box
  HostName example.com
  User ubuntu
  Port 2222
  IdentityFile ~/.ssh/id_ed25519
"#;
        let hosts = parse_ssh_config_hosts(input);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host_alias, "my-box");
        assert_eq!(hosts[0].host_name.as_deref(), Some("example.com"));
        assert_eq!(hosts[0].user.as_deref(), Some("ubuntu"));
        assert_eq!(hosts[0].port, Some(2222));
    }
}
