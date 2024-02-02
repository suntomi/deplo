FROM --platform=${BUILDPLATFORM} ghcr.io/suntomi/deplo:builder as builder

ARG TARGETARCH
ENV TARGETARCH=${TARGETARCH}
ADD . /workdir
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    cargo build --release --target=${ARCH}-unknown-linux-musl && \
    cp /tmp/target/${ARCH}-unknown-linux-musl/release/cli /tmp/cli

FROM ghcr.io/suntomi/deplo:base

COPY --from=builder /tmp/cli /usr/local/bin/deplo

ENTRYPOINT ["deplo"]