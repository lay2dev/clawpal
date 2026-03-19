FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y openssh-server curl git && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir /var/run/sshd

# Allow root login with password
RUN echo "root:clawpal-perf-e2e" | chpasswd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config && \
    echo "Port 2299" >> /etc/ssh/sshd_config

# Install Node.js + openclaw
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g openclaw@latest

# Seed OpenClaw configuration
RUN mkdir -p /root/.openclaw/agents/main/agent

COPY tests/e2e/perf/seed/openclaw.json /root/.openclaw/openclaw.json
COPY tests/e2e/perf/seed/auth-profiles.json /root/.openclaw/agents/main/agent/auth-profiles.json

COPY tests/e2e/perf/docker-entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 2299 18789
CMD ["/entrypoint.sh"]
