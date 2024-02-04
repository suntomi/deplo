FROM ghcr.io/suntomi/deplo:builder as builder

ARG TARGETARCH
ENV TARGETARCH=${TARGETARCH}
ADD ./core /workdir/core
ADD ./cli /workdir/cli
ADD ./Cargo.* /workdir/
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    cargo zigbuild --release --target=${ARCH}-unknown-linux-musl && \
    cp /tmp/target/${ARCH}-unknown-linux-musl/release/cli /tmp/cli

FROM ghcr.io/suntomi/deplo:base

COPY --from=builder /tmp/cli /usr/local/bin/deplo

ENTRYPOINT ["deplo"]