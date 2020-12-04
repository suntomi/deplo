#!/bin/bash

# install settings
CLOUDSDK_PYTHON_SITEPACKAGES=1
CLOUDSDK_VERSION="292.0.0"
INSTALL_DIR=${DEPLO_TOOLS_PATH:-"/tmp/deplo-tools"}

echo "-----------------------------------------------"
echo "install gcloud sdk"
echo "CAUTION: it takes sooooooo long time on container in docker mac"
echo "-----------------------------------------------"
echo "download gcloud CLI..."
cd /tmp
if [ ! -e google-cloud-sdk.zip ]; then
    curl https://dl.google.com/dl/cloudsdk/channels/rapid/downloads/google-cloud-sdk-$CLOUDSDK_VERSION-linux-x86_64.tar.gz \
        --output google-cloud-sdk.zip.tmp
    mv google-cloud-sdk.zip.tmp google-cloud-sdk.zip
fi
if [ ! -e google-cloud-sdk ]; then
    tar -zxf google-cloud-sdk.zip
fi
google-cloud-sdk/install.sh --usage-reporting=true --path-update=true --bash-completion=true --rc-path=/.bashrc \
    --additional-components kubectl alpha beta

echo "disable auto upgrade..."
google-cloud-sdk/bin/gcloud config set --installation component_manager/disable_update_check true
sed -i -- 's/\"disable_updater\": false/\"disable_updater\": true/g' google-cloud-sdk/lib/googlecloudsdk/core/config.json

echo "move destination path"
rm google-cloud-sdk.zip
mkdir -p $INSTALL_DIR
mv google-cloud-sdk $INSTALL_DIR/cloud
