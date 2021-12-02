#!/bin/bash

mkdir -p tools/docker/manifests
mkdir -p tools/docker/manifests/cli
mkdir -p tools/docker/manifests/core
cp Cargo.* tools/docker/manifests
cp cli/Cargo.* tools/docker/manifests/cli
cp core/Cargo.* tools/docker/manifests/core
