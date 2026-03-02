export const SSH_PASSPHRASE_RETRY_HINT =
  /passphrase|sign_and_send_pubkey|agent refused operation|can't open \/dev\/tty|authentication agent|key is encrypted|encrypted|passphrase required|public key authentication failed/i;

export const SSH_PASSPHRASE_REJECT_HINT =
  /bad decrypt|incorrect passphrase|wrong passphrase|passphrase.*failed|decrypt failed/i;

export const SSH_NO_KEY_HINT =
  /no such file|no such key|could not open|not found|cannot find/i;

export const SSH_PUBLIC_KEY_PERMISSION_HINT = /permission denied|public key authentication failed/i;

export function buildSshPassphraseConnectErrorMessage(rawError: string, hostLabel: string): string | null {
  if (SSH_PASSPHRASE_REJECT_HINT.test(rawError)) {
    return `SSH 口令校验失败（host: ${hostLabel}）。请确认私钥口令正确，或先解锁对应密钥后重试。`;
  }
  if (SSH_NO_KEY_HINT.test(rawError) && /key/i.test(rawError)) {
    return `未找到可用私钥文件（host: ${hostLabel}）。请检查 SSH 配置里的 IdentityFile 是否可读。`;
  }
  if (SSH_PUBLIC_KEY_PERMISSION_HINT.test(rawError)) {
    return (
      `SSH 认证失败（host: ${hostLabel}）。当前口令已提交，但远端仍拒绝。` +
      "请确认 public key 已加入 authorized_keys，用户为 root 并且主机指纹匹配。"
    );
  }
  return null;
}

export function buildSshPassphraseCancelMessage(hostLabel: string): string {
  return `已取消输入 SSH 私钥口令（host: ${hostLabel}）。如果该密钥加密，请重试并输入口令。`;
}

