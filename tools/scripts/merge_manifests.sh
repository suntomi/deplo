#!/bin/sh
set -e

base="${1:-"ghcr.io/suntomi/deplo:builder"}"

echo ${SUNTOMI_VCS_ACCOUNT_KEY} | docker login ghcr.io -u ${SUNTOMI_VCS_ACCOUNT} --password-stdin

# create manifest if not exists
set +e
docker manifest inspect ${base} > /dev/null 2>&1
if [ $? -ne 0 ]; then
    docker manifest create ${base} ${base}-amd64 ${base}-arm64
fi
set -e
# add architecture information to manifest
docker manifest annotate ${base} ${base}-amd64 --os linux --arch amd64
docker manifest annotate ${base} ${base}-arm64 --os linux --arch arm64

# push manifest
docker manifest push ${base}