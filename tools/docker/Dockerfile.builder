FROM --platform=${BUILDPLATFORM} debian:bookworm-slim as zig
RUN apt update && apt install -y curl xz-utils
# 0.11.0 has lfs64 linker error with cargo-zigbuild https://github.com/ziglang/zig/issues/15610
ENV ZIG_VERSION="0.10.1"
RUN if [ "${BUILDPLATFORM}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    curl -LO https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz && \
    tar -xf zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz && mkdir -p /opt/zig && \
    mv zig-linux-${ARCH}-${ZIG_VERSION}/zig /opt/zig/ && \
    mv zig-linux-${ARCH}-${ZIG_VERSION}/lib /opt/zig/ && ls -al /opt/zig/ && \
    rm -rf zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz zig-linux-${ARCH}-${ZIG_VERSION}

FROM --platform=${BUILDPLATFORM} debian:bookworm-slim as builder-x86_64
COPY --from=zig /opt/zig/zig /usr/local/bin/zig
COPY --from=zig /opt/zig/lib /usr/local/bin/lib
RUN apt update && apt install -y curl perl make
ENV OPENSSL_VERSION="3.2.1" ARCH="x86_64"
RUN curl -L https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz \
    -o /tmp/opensslg.tgz && tar -zxvf /tmp/opensslg.tgz -C /tmp && cd /tmp/openssl-${OPENSSL_VERSION} && \
    CC="zig cc -target ${ARCH}-linux-musl" AR="zig ar" ./Configure no-shared no-async linux-${ARCH} && \
    make depend && make -j$(nproc) && make install_sw && \
    rm -f /tmp/opensslg.tgz && rm -rf /tmp/openssl-${OPENSSL_VERSION}    

FROM --platform=${BUILDPLATFORM} debian:bookworm-slim as builder-aarch64
COPY --from=zig /opt/zig/zig /usr/local/bin/zig
COPY --from=zig /opt/zig/lib /usr/local/bin/lib
RUN apt update && apt install -y curl perl make
ENV OPENSSL_VERSION="3.2.1" ARCH="aarch64"
RUN curl -L https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz \
    -o /tmp/opensslg.tgz && tar -zxvf /tmp/opensslg.tgz -C /tmp && cd /tmp/openssl-${OPENSSL_VERSION} && \
    CC="zig cc -target ${ARCH}-linux-musl" AR="zig ar" ./Configure no-shared no-async linux-${ARCH} && \
    make depend && make -j$(nproc) && make install_sw && \
    rm -f /tmp/opensslg.tgz && rm -rf /tmp/openssl-${OPENSSL_VERSION}    

FROM --platform=${BUILDPLATFORM} rust:slim
COPY --from=zig /opt/zig/zig /usr/local/bin/zig
COPY --from=zig /opt/zig/lib /usr/local/bin/lib
RUN rustup target add x86_64-unknown-linux-musl && cargo install cargo-zigbuild && \
    rustup target add aarch64-unknown-linux-musl && cargo install cargo-zigbuild
RUN mkdir -p /usr/local/x86_64/lib && mkdir -p /usr/local/aarch64/lib && \
    mkdir -p /usr/local/x86_64/include && mkdir -p /usr/local/aarch64/include
COPY --from=builder-x86_64 /usr/local/include/openssl /usr/local/x86_64/include/openssl
COPY --from=builder-x86_64 /usr/local/lib64/* /usr/local/x86_64/lib/
COPY --from=builder-aarch64 /usr/local/include/openssl /usr/local/aarch64/include/openssl
COPY --from=builder-aarch64 /usr/local/lib/* /usr/local/aarch64/lib/

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
RUN export ARCH="x86_64"; \
    export OPENSSL_LIB_DIR="/usr/local/${ARCH}/lib"; \
    export OPENSSL_DIR="/usr/local/${ARCH}"; \
    cargo zigbuild --release --target ${ARCH}-unknown-linux-musl --color never --features single-binary && \
    rm -r /tmp/target/${ARCH}-unknown-linux-musl/release/.fingerprint/cli-*
RUN export ARCH="aarch64"; \
    export OPENSSL_LIB_DIR="/usr/local/${ARCH}/lib"; \
    export OPENSSL_DIR="/usr/local/${ARCH}"; \
    cargo zigbuild --release --target ${ARCH}-unknown-linux-musl --color never --features single-binary && \
    rm -r /tmp/target/${ARCH}-unknown-linux-musl/release/.fingerprint/cli-*
# install dependent cli
RUN apt update && apt install -y git
