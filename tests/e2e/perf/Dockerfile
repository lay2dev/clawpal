FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y openssh-server curl && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir /var/run/sshd

# Allow root login with password
RUN echo "root:clawpal-perf-e2e" | chpasswd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config

# Install Node.js + openclaw
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g openclaw@latest

# Seed OpenClaw configuration
RUN mkdir -p /root/.openclaw/agents/main/agent

RUN cat > /root/.openclaw/openclaw.json << 'OCEOF'
{
  "gateway": {
    "port": 18789,
    "token": "perf-test-token"
  },
  "defaults": {
    "model": "anthropic/claude-sonnet-4-20250514"
  },
  "models": {
    "anthropic/claude-sonnet-4-20250514": {
      "provider": "anthropic",
      "model": "claude-sonnet-4-20250514"
    },
    "openai/gpt-4o": {
      "provider": "openai",
      "model": "gpt-4o"
    }
  },
  "agents": {
    "list": [
      { "id": "main", "model": "anthropic/claude-sonnet-4-20250514" },
      { "id": "worker-1", "model": "openai/gpt-4o" }
    ]
  }
}
OCEOF

RUN cat > /root/.openclaw/agents/main/agent/auth-profiles.json << 'AUTHEOF'
{
  "anthropic": { "apiKey": "test-key-anthropic" },
  "openai": { "apiKey": "test-key-openai" }
}
AUTHEOF

EXPOSE 22
CMD ["/usr/sbin/sshd", "-D"]
