#!/bin/bash
set -eo pipefail

echo "====== test envref variables and oidc token generation ======"
if [ -z "${ACTIONS_ID_TOKEN_REQUEST_TOKEN}" ]; then
    echo "ACTIONS_ID_TOKEN_REQUEST_TOKEN is not set"
    exit 1
fi
if [ "${TEST_ID_TOKEN_REQUEST_URL}" != "${ACTIONS_ID_TOKEN_REQUEST_URL}" ]; then
    echo "TEST_ID_TOKEN_REQUEST_URL is not correctly imported"
    exit 1
fi
if [ "${TEST_ID_TOKEN_REQUEST_TOKEN}" != "${ACTIONS_ID_TOKEN_REQUEST_TOKEN}" ]; then
    echo "TEST_ID_TOKEN_REQUEST_TOKEN is not correctly imported"
    exit 1
fi
deplo ci token oidc --aud "sts.amazonaws.com" --out /tmp/token.json
eval $(bash tools/scripts/aws_credentials.sh /tmp/token.json)
aws sts get-caller-identity

echo "====== unit test ======"
cargo test