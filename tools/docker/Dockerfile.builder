FROM rust:slim

RUN apt-get update && apt-get install -y git curl make musl-tools && apt-get clean
RUN ln -s /usr/include/x86_64-linux-gnu/asm /usr/include/x86_64-linux-musl/asm && \
    ln -s /usr/include/asm-generic /usr/include/x86_64-linux-musl/asm-generic && \
    ln -s /usr/include/linux /usr/include/x86_64-linux-musl/linux
RUN curl -L https://github.com/openssl/openssl/archive/OpenSSL_1_1_1g.tar.gz -o /tmp/opensslg.tgz && \
    tar -zxvf /tmp/opensslg.tgz -C /tmp && cd /tmp/openssl-OpenSSL_1_1_1g && \
    CC="musl-gcc -fPIE -pie" ./Configure no-shared no-async linux-x86_64 && \
    make depend && make -j$(nproc) && make install_sw && \
    rm -f /tmp/opensslg.tgz && rm -rf /tmp/openssl-OpenSSL_1_1_1g
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
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV OPENSSL_STATIC=true
ENV OPENSSL_DIR=/usr/local
# remove build result cache to force rebuild with actual project
RUN cargo build --release --target x86_64-unknown-linux-musl --color never && \
    rm -r /tmp/target/x86_64-unknown-linux-musl/release/.fingerprint/cli-*
