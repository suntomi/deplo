FROM alpine:3.11

# install common dependencies
RUN apk --update add git make python unzip expect curl bash

# install gcloud command, TODO: move to on-demand installation
ENV HOME /
ENV CLOUDSDK_PYTHON_SITEPACKAGES 1
ENV CLOUDSDK_VERSION=292.0.0
RUN curl https://dl.google.com/dl/cloudsdk/channels/rapid/downloads/google-cloud-sdk-$CLOUDSDK_VERSION-linux-x86_64.tar.gz --output google-cloud-sdk.zip && tar -zxf google-cloud-sdk.zip && rm google-cloud-sdk.zip
RUN google-cloud-sdk/install.sh --usage-reporting=true --path-update=true --bash-completion=true --rc-path=/.bashrc --additional-components kubectl alpha beta bigtable
# Disable updater check for the whole installation. # Users won't be bugged with notifications to update to the latest version of gcloud.
RUN google-cloud-sdk/bin/gcloud config set --installation component_manager/disable_update_check true
# Disable updater completely. # Running `gcloud components update` doesn't really do anything in a union FS. # Changes are lost on a subsequent run.
RUN sed -i -- 's/\"disable_updater\": false/\"disable_updater\": true/g' /google-cloud-sdk/lib/googlecloudsdk/core/config.json
RUN ln -s /google-cloud-sdk /usr/lib
ENV PATH /google-cloud-sdk/bin:/bin:$PATH

# TODO: install aws command 

# TODO: install aliyun command

# other utils
ENV HUB_VERSION=2.14.2
RUN curl -L -o /hub.tgz https://github.com/github/hub/releases/download/v$HUB_VERSION/hub-linux-amd64-$HUB_VERSION.tgz && \
	tar -zxvf /hub.tgz && /hub-linux-amd64-$HUB_VERSION/install && rm -rf /hub.tgz /hub-linux-amd64-$HUB_VERSION

ENV TERRAFORM_VERSION=0.12.25
RUN curl -L -o /tf.zip https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_linux_amd64.zip && \
	unzip /tf.zip && mv /terraform /usr/local/bin && rm -rf /tf.zip

ENV DOCKER_COMPOSE_VERSION=1.25.5
RUN curl -L https://github.com/docker/compose/releases/download/${DOCKER_COMPOSE_VERSION}/docker-compose-`uname -s`-`uname -m` -o /usr/local/bin/docker-compose && \
	chmod +x /usr/local/bin/docker-compose

ENV GIT_LFS_VERSION=2.11.0
RUN mkdir /git-lfs && curl -o /git-lfs/package.tgz -L https://github.com/git-lfs/git-lfs/releases/download/v${GIT_LFS_VERSION}/git-lfs-linux-amd64-v${GIT_LFS_VERSION}.tar.gz && \
	cd /git-lfs && tar -zxvf package.tgz && bash install.sh && rm -rf /git-lfs

ENV JQ_VERSION=1.6
RUN curl -o /usr/bin/jq -L https://github.com/stedolan/jq/releases/download/jq-$JQ_VERSION/jq-linux64 && chmod +x /usr/bin/jq

ENV DOCKER_VERSION=18.06.1-ce
USER root
RUN curl -o /docker.tgz https://download.docker.com/linux/static/stable/x86_64/docker-$DOCKER_VERSION.tgz && \
	tar -zxvf /docker.tgz && cp /docker/docker* /usr/bin && rm -rf /docker /docker.tgz
