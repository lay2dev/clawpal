FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev \
    libglib2.0-dev \
    librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
RUN cargo install tauri-driver --locked

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm install

COPY . .

RUN npx @tauri-apps/cli build --no-bundle 2>&1 | tail -30

FROM ubuntu:24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive
ENV DISPLAY=:99
ENV SCREENSHOT_DIR=/screenshots
ENV REPORT_DIR=/report
ENV APP_BINARY=/usr/local/bin/clawpal
ENV OPENCLAW_IMAGE=clawpal-recipe-openclaw:latest
ENV OPENCLAW_CONTAINER_NAME=clawpal-recipe-e2e
ENV OPENCLAW_SSH_HOST=127.0.0.1
ENV OPENCLAW_SSH_PORT=2222
ENV OPENCLAW_SSH_USER=root
ENV OPENCLAW_SSH_PASSWORD=clawpal-recipe-e2e

RUN apt-get update && apt-get install -y \
    xvfb \
    libwebkit2gtk-4.1-0 \
    libgtk-3-0 \
    libsoup-3.0-0 \
    libjavascriptcoregtk-4.1-0 \
    webkit2gtk-driver \
    fonts-noto-cjk \
    fonts-noto-color-emoji \
    dbus \
    dbus-x11 \
    ca-certificates \
    curl \
    docker.io \
    jq \
    openssh-client \
    sshpass \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /root/.cargo/bin/tauri-driver /usr/local/bin/tauri-driver
COPY --from=builder /app/target/release/clawpal /usr/local/bin/clawpal

COPY harness/recipe-e2e/package.json /harness/package.json
WORKDIR /harness
RUN npm install

COPY harness/recipe-e2e/recipe-e2e.mjs /harness/recipe-e2e.mjs
RUN mkdir -p /workspace/harness/recipe-e2e
COPY harness/recipe-e2e/openclaw-container/ /workspace/harness/recipe-e2e/openclaw-container/
COPY harness/recipe-e2e/entrypoint.sh /entrypoint.sh

RUN mkdir -p /root/.openclaw/agents/main/agent /root/.clawpal /screenshots /report
COPY harness/recipe-e2e/mock-data/openclaw.json /root/.openclaw/openclaw.json
COPY harness/recipe-e2e/mock-data/agents/ /root/.openclaw/agents/
COPY harness/recipe-e2e/mock-data/instances.json /root/.clawpal/instances.json

# Copy recipe library to where the binary expects it
COPY examples/recipe-library /usr/lib/ClawPal/recipe-library
COPY examples/recipe-library /usr/lib/ClawPal/examples/recipe-library
COPY src-tauri/resources/watchdog.js /usr/lib/ClawPal/watchdog.js

RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
