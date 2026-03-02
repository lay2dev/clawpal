pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn wrap_login_shell_eval(command: &str) -> String {
    let escaped = shell_quote(command);
    format!(
        "export CLAWPAL_LOGIN_CMD={escaped}; \
LOGIN_SHELL=\"${{SHELL:-/bin/sh}}\"; \
[ -x \"$LOGIN_SHELL\" ] || LOGIN_SHELL=\"/bin/sh\"; \
case \"$LOGIN_SHELL\" in \
  */zsh|*/bash) \"$LOGIN_SHELL\" -lc 'eval \"$CLAWPAL_LOGIN_CMD\"' ;; \
  *) \"$LOGIN_SHELL\" -lc '[ -f ~/.profile ] && . ~/.profile >/dev/null 2>&1 || true; eval \"$CLAWPAL_LOGIN_CMD\"' ;; \
esac"
    )
}

#[cfg(test)]
mod tests {
    use super::{shell_quote, wrap_login_shell_eval};

    #[test]
    fn shell_quote_escapes_single_quote() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn wrap_login_shell_eval_uses_login_shell_for_bash_zsh() {
        let wrapped = wrap_login_shell_eval("openclaw --version");
        assert!(wrapped.contains("*/zsh|*/bash) \"$LOGIN_SHELL\" -lc"));
        assert!(wrapped.contains("[ -f ~/.profile ]"));
        assert!(wrapped.contains("eval \"$CLAWPAL_LOGIN_CMD\""));
    }
}
