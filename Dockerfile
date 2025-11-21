FROM debian:bookworm-slim AS build
# uses glibc 2.36

SHELL ["/bin/bash", "-eo", "pipefail", "-c"]

RUN apt update && apt install -y \
    build-essential \
    curl \
    pkg-config \
    libssl-dev \
    unzip \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Install rustup and rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

# Install Bun for building the UI assets
ENV BUN_INSTALL=/root/.bun
RUN curl -fsSL https://bun.sh/install | bash

ENV PATH="/root/.cargo/bin:${BUN_INSTALL}/bin:${PATH}"

WORKDIR /usr/src/app

COPY . .

# Build UI and Rust binary
RUN cd ui && bun install && bun run build
RUN cargo build --release
RUN install -Dm755 target/release/simple_git_cicd /artifacts/simple_git_cicd

FROM scratch AS artifact
COPY --from=build /artifacts/simple_git_cicd /simple_git_cicd
