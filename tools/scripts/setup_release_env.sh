#!/bin/bash

if [ "${DEPLO_CI_RELEASE_TARGET}" = "prod" ]; then
    export DEPLO_RELEASE_TAG=${DEPLO_CI_TAG_NAME}
    export DEPLO_RELEASE_NAME=${DEPLO_CI_TAG_NAME}
    export DEPLO_RELEASE_VERSION=${DEPLO_CI_TAG_NAME}
else
    if [ -z "${DEPLO_CI_RELEASE_TARGET}" ]; then
        echo "DEPLO_CI_RELEASE_TARGET should exists, please use -r option if you want to release product from your local branch."
        exit 1
    fi
    export DEPLO_RELEASE_TAG=${DEPLO_CI_RELEASE_TARGET}
    export DEPLO_RELEASE_NAME=${DEPLO_CI_RELEASE_TARGET}
    export DEPLO_RELEASE_VERSION=${DEPLO_CI_RELEASE_TARGET}
fi
