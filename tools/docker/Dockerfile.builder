FROM rust:slim

RUN apt-get update && apt-get install -y git
RUN rustup target add x86_64-unknown-linux-musl

# TODO: implement following kind of techinic is useful for faster build.

# precompile dependency crates and store above path
# RUN USER=root cargo new --bin workdir
# WORKDIR /workdir
# COPY ./Cargo.toml Cargo.toml
# COPY ./Cargo.lock Cargo.lock
# # remove build result cache to force rebuild with actual project 
# RUN cargo build --color never && rm src/*.rs && rm -r /tmp/target/debug/.fingerprint/deplo-*
