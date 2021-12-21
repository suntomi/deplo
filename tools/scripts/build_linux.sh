#!/bin/sh

docker run --rm -v $(pwd):/workdir -w /workdir -e DEPLO_RELEASE_VERSION=${DEPLO_RELEASE_VERSION} \
    ghcr.io/suntomi/deplo:builder \
    bash -c "cargo build --release --target=x86_64-unknown-linux-musl && \
        cp /tmp/target/x86_64-unknown-linux-musl/release/cli /workdir/tools/docker/bin"
