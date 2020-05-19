FROM suntomi/deplo_base

# all rust toolchain requires gcc/clang installed (https://github.com/rust-lang/rustup/issues/2213)
RUN apk --update add gcc g++

RUN curl https://sh.rustup.rs -sSf | \
    # 1.44がreleaseされたらbetaは外す. (https://github.com/rust-lang/rust/issues/40174#issuecomment-618999438)
    sh /dev/stdin -y --default-toolchain beta \
    --target x86_64-apple-darwin
ENV PATH=$PATH:$HOME/.cargo/bin

# store build artifacts at container local FS for faster build
ENV CARGO_BUILD_TARGET_DIR=/tmp/target

# precompile dependency crates and store above path
RUN USER=root cargo new --bin workdir
WORKDIR /workdir
COPY ./Cargo.toml Cargo.toml
COPY ./Cargo.lock Cargo.lock
# remove build result cache to force rebuild with actual project 
RUN cargo build --color never && rm src/*.rs && rm -r /tmp/target/debug/.fingerprint/deplo-*
