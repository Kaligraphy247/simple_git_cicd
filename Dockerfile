FROM debian:buster
# uses glibc 2.28

RUN apt-get update && apt-get install -y \
    build-essential curl pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Install rustup and rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /usr/src/myapp

COPY . .

RUN cargo build --release
