FROM --platform=${BUILDPLATFORM} ghcr.io/suntomi/deplo:builder as builder

ARG TARGETARCH
ENV TARGETARCH=${TARGETARCH}
ARG DEPLO_RELEASE_VERSION
ENV DEPLO_RELEASE_VERSION=${DEPLO_RELEASE_VERSION}
ADD ./core /workdir/core
ADD ./cli /workdir/cli
ADD ./Cargo.* /workdir/
ADD ./Deplo.toml /workdir/
ADD ./tools/modules /workdir/tools/modules
ADD ./.deplo/env /workdir/.env
ADD ./.git /workdir/.git
RUN echo "DEPLO_RELEASE_VERSION=[${DEPLO_RELEASE_VERSION}]"
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    cargo zigbuild --release --target=${ARCH}-unknown-linux-musl && \
    cp /tmp/target/${ARCH}-unknown-linux-musl/release/cli /tmp/cli
# upload symbol files (cli.debug) to github release here, so that avoiding unnecessary copy to release image
RUN if [ "${TARGETARCH}" = "arm64" ]; then export ARCH="aarch64"; else export ARCH="x86_64"; fi && \
    cp /tmp/cli /tmp/cli.debug && \
    ${ARCH}-linux-gnu-strip /tmp/cli && \
    ls -al /tmp/cli && \
    CI=true GITHUB_OUTPUT=true GITHUB_ENV=true \
        /tmp/cli vcs release-assets ${DEPLO_RELEASE_VERSION} \
        /tmp/cli.debug --replace -o name=deplo-Linux-${ARCH}.debug

FROM ghcr.io/suntomi/deplo:base

COPY --from=builder /tmp/cli /usr/local/bin/deplo

ENTRYPOINT ["deplo"]