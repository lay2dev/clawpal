# ================================================================
# Stage 1: Build ClawPal + tauri-driver
# ================================================================
FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    build-essential curl git pkg-config \
    libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
    libglib2.0-dev librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*

# Rust stable
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

# tauri-driver
RUN cargo install tauri-driver --locked

# Node.js 22
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm install

COPY . .

# Use Tauri CLI to build — this properly embeds frontend into the binary
RUN npx @tauri-apps/cli build --no-bundle 2>&1 | tail -30

# ================================================================
# Stage 2: Runtime with Xvfb + WebDriver
# ================================================================
FROM ubuntu:24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    xvfb \
    libwebkit2gtk-4.1-0 libgtk-3-0 \
    libsoup-3.0-0 libjavascriptcoregtk-4.1-0 \
    webkit2gtk-driver \
    fonts-noto-cjk fonts-noto-color-emoji \
    dbus dbus-x11 \
    curl \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Binaries from builder
COPY --from=builder /root/.cargo/bin/tauri-driver /usr/local/bin/tauri-driver
COPY --from=builder /app/target/release/clawpal /usr/local/bin/clawpal

# Harness scripts + deps
COPY harness/screenshot/package.json /harness/package.json
WORKDIR /harness
RUN npm install

COPY harness/screenshot/capture.mjs /harness/capture.mjs
COPY harness/screenshot/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Mock OpenClaw data
COPY harness/screenshot/mock-data/ /root/.openclaw/

RUN mkdir -p /screenshots
ENV DISPLAY=:99

ENTRYPOINT ["/entrypoint.sh"]
CMD ["all"]
