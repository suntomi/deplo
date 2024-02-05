FROM rust:slim

ARG TARGETARCH
ENV TARGETARCH=${TARGETARCH}
RUN apt update && apt install -y git musl-tools musl-dev curl perl make xz-utils
# 0.11.0 has lfs64 linker error with cargo-zigbuild https://github.com/ziglang/zig/issues/15610
ENV ZIG_VERSION="0.10.1"
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    curl -LO https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz && \
    tar -xf zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz && \
    mv zig-linux-${ARCH}-${ZIG_VERSION}/zig /usr/local/bin/ && \
    mv zig-linux-${ARCH}-${ZIG_VERSION}/lib /usr/local/bin/ && \
    rm -rf zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz zig-linux-${ARCH}-${ZIG_VERSION}
ENV OPENSSL_VERSION="3.2.1"
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    curl -L https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz \
    -o /tmp/opensslg.tgz && tar -zxvf /tmp/opensslg.tgz -C /tmp && cd /tmp/openssl-${OPENSSL_VERSION} && \
    CC="zig cc -target ${ARCH}-linux-musl" AR="zig ar" ./Configure no-shared no-async linux-${ARCH} && \
    make depend && make -j$(nproc) && make install_sw && \
    rm -f /tmp/opensslg.tgz && rm -rf /tmp/openssl-${OPENSSL_VERSION}    
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    rustup target add ${ARCH}-unknown-linux-musl && cargo install cargo-zigbuild

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
ENV OPENSSL_DIR=/usr/local
# remove build result cache to force rebuild with actual project
RUN if [ "${TARGETARCH}" = "arm64" ]; then \
        export ARCH="aarch64"; \
        export OPENSSL_LIB_DIR="/usr/local/lib"; \
    else \
        export ARCH="x86_64"; \
        export OPENSSL_LIB_DIR="/usr/local/lib64"; \
    fi && \
    cargo zigbuild --release --target ${ARCH}-unknown-linux-musl --color never --features single-binary && \
    rm -r /tmp/target/${ARCH}-unknown-linux-musl/release/.fingerprint/cli-*
