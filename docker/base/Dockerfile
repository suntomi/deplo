FROM alpine:3.11

# install common dependencies
RUN apk --update add git make python unzip expect curl bash openssh

# defiine on-demand command install path
ENV DEPLO_TOOLS_PATH=/deplo-tools
ENV PATH $DEPLO_TOOLS_PATH/cloud/google-cloud-sdk/bin:/bin:$PATH

# common utils
# to avoid dynamic link issue, install hub from edge
RUN apk add --no-cache -X http://dl-cdn.alpinelinux.org/alpine/edge/testing hub
#ENV HUB_VERSION=2.14.2
#RUN curl -L -o /hub.tgz https://github.com/github/hub/releases/download/v$HUB_VERSION/hub-linux-amd64-$HUB_VERSION.tgz && \
#	tar -zxvf /hub.tgz && /hub-linux-amd64-$HUB_VERSION/install && rm -rf /hub.tgz /hub-linux-amd64-$HUB_VERSION

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

ENV DOCKER_VERSION=19.03.9
USER root
RUN curl -o /docker.tgz https://download.docker.com/linux/static/stable/x86_64/docker-$DOCKER_VERSION.tgz && \
	tar -zxvf /docker.tgz && cp /docker/docker* /usr/bin && rm -rf /docker /docker.tgz

ENV HOME /