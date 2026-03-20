FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.local/bin:/usr/local/bin:${PATH}"

ARG ROOT_PASSWORD=clawpal-recipe-e2e

RUN apt-get update && apt-get install -y \
    openssh-server \
    curl \
    ca-certificates \
    git \
    xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/run/sshd

RUN echo "root:${ROOT_PASSWORD}" | chpasswd \
    && sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && sed -i 's/PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config 
    && echo "MaxSessions 20" >> /etc/ssh/sshd_config 
    && echo "MaxStartups 20:30:60" >> /etc/ssh/sshd_config 
    && echo "Subsystem sftp /usr/lib/openssh/sftp-server" >> /etc/ssh/sshd_config

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g openclaw@latest

RUN mkdir -p \
    /root/.openclaw/agents/main/agent \
    /root/.openclaw/agents/test-e2e-agent/agent \
    /root/.openclaw/instances/openclaw-recipe-e2e/workspace

COPY harness/recipe-e2e/openclaw-container/seed/openclaw.json /root/.openclaw/openclaw.json
COPY harness/recipe-e2e/openclaw-container/seed/auth-profiles.json /root/.openclaw/agents/main/agent/auth-profiles.json
COPY harness/recipe-e2e/openclaw-container/seed/IDENTITY.md /root/.openclaw/agents/main/agent/IDENTITY.md
COPY harness/recipe-e2e/openclaw-container/seed/SOUL.md /root/.openclaw/agents/main/agent/SOUL.md

RUN echo "export ANTHROPIC_API_KEY=test-anthropic-recipe-key" >> /root/.bashrc \
    && echo "export OPENAI_API_KEY=test-openai-recipe-key" >> /root/.bashrc \
    && echo "export PATH=/root/.local/bin:/usr/local/bin:\$PATH" >> /root/.bashrc \
    && echo "export ANTHROPIC_API_KEY=test-anthropic-recipe-key" >> /root/.profile \
    && echo "export OPENAI_API_KEY=test-openai-recipe-key" >> /root/.profile \
    && echo "export PATH=/root/.local/bin:/usr/local/bin:\$PATH" >> /root/.profile

COPY harness/recipe-e2e/openclaw-container/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22 18789

ENTRYPOINT ["/entrypoint.sh"]
