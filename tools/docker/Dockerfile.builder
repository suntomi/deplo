FROM --platform=${BUILDPLATFORM} rust:alpine

ARG TARGETARCH
ENV TARGETARCH=${TARGETARCH}
RUN apk add --update --no-cache musl-dev openssl-dev openssl-libs-static
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    rustup target add ${ARCH}-unknown-linux-musl

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
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV OPENSSL_STATIC=true
# remove build result cache to force rebuild with actual project
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    cargo build --release --target ${ARCH}-unknown-linux-musl --color never --features single-binary && \
    rm -r /tmp/target/${ARCH}-unknown-linux-musl/release/.fingerprint/cli-*
