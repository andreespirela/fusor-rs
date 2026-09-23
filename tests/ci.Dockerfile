# The CI browser job's environment, for `just ci-docker`: Linux, the latest
# stable Rust (what CI installs) and the Playwright browsers.
FROM mcr.microsoft.com/playwright:v1.63.0-noble

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
        --target wasm32-unknown-unknown --component rustfmt,clippy,rust-analyzer \
    && curl -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

ENV PLAYWRIGHT_BROWSERS=chromium,firefox,webkit
WORKDIR /work
