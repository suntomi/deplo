FROM rust:slim

RUN apt-get update && apt-get install -y git
RUN rustup target add x86_64-unknown-linux-musl

# precompile dependency crates and store above path
ENV CARGO_BUILD_TARGET_DIR=/tmp/target
RUN mkdir /workdir
WORKDIR /workdir

RUN USER=root cargo new --bin cli
RUN USER=root cargo new --lib core
COPY ./manifests/Cargo.toml Cargo.toml
COPY ./manifests/Cargo.lock Cargo.lock
COPY ./manifests/cli/Cargo.* cli
COPY ./manifests/core/Cargo.* core
# remove build result cache to force rebuild with actual project
RUN cargo build --release --target x86_64-unknown-linux-musl --color never && \
    rm -r /tmp/target/x86_64-unknown-linux-musl/release/.fingerprint/cli-*
