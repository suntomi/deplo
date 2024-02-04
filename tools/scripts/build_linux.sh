#!/bin/sh
set -e

CWD=$(cd $(dirname "$0") && pwd)
ROOT=${CWD}/../..

export_bin() {
    local platform=$1
    local out=$2
    docker create --platform ${platform} --name deplo-bin ghcr.io/suntomi/deplo:${DEPLO_RELEASE_VERSION}
    mkdir -p ${out}
    docker cp deplo-bin:/usr/local/bin/deplo ${out}/cli
    docker rm deplo-bin
}

echo ${SUNTOMI_VCS_ACCOUNT_KEY} | docker login ghcr.io -u ${SUNTOMI_VCS_ACCOUNT} --password-stdin
docker buildx create --name mp --bootstrap --use
docker buildx build --push --platform linux/amd64,linux/arm64 -t ghcr.io/suntomi/deplo:${DEPLO_RELEASE_VERSION} -f ${ROOT}/tools/docker/Dockerfile ${ROOT}
export_bin amd64 ${ROOT}/tools/docker/bin/x86_64
export_bin arm64 ${ROOT}/tools/docker/bin/aarch64