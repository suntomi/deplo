#!/bin/sh
set -e

base="${1:-"ghcr.io/suntomi/deplo:builder"}"

echo ${SUNTOMI_VCS_ACCOUNT_KEY} | docker login ghcr.io -u ${SUNTOMI_VCS_ACCOUNT} --password-stdin

# create manifest list
docker manifest create ${base} ${base}-amd64 ${base}-arm64

# add architecture information to manifest
docker manifest annotate ${base} ${base}-amd64 --os linux --arch amd64
docker manifest annotate ${base} ${base}-arm64 --os linux --arch arm64

# push manifest
docker manifest push ${base}