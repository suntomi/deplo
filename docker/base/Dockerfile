FROM ubuntu:groovy-20200921

# install common dependencies
RUN apt-get update && \
	apt-get install -y \
		git=1:2.27.0-1ubuntu1 \
		unzip=6.0-25ubuntu1 \
		python3=3.8.6-0ubuntu1 \
		python-is-python3 \
		curl=7.68.0-1ubuntu4 \
		hub=2.7.0~ds1-1build1 && \
	rm -rf /var/lib/apt/lists/*

# install dependencies from github
ENV TERRAFORM_VERSION=0.12.25
RUN curl -L -o /tf.zip https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_linux_amd64.zip && \
	unzip /tf.zip && mv /terraform /usr/local/bin && rm -rf /tf.zip

ENV GIT_LFS_VERSION=2.11.0
RUN mkdir /git-lfs && curl -o /git-lfs/package.tgz -L https://github.com/git-lfs/git-lfs/releases/download/v${GIT_LFS_VERSION}/git-lfs-linux-amd64-v${GIT_LFS_VERSION}.tar.gz && \
	cd /git-lfs && tar -zxvf package.tgz && bash install.sh && rm -rf /git-lfs

ENV JQ_VERSION=1.6
RUN curl -o /usr/bin/jq -L https://github.com/stedolan/jq/releases/download/jq-$JQ_VERSION/jq-linux64 && chmod +x /usr/bin/jq

ENV DOCKER_VERSION=19.03.9
USER root
RUN curl -o /docker.tgz https://download.docker.com/linux/static/stable/x86_64/docker-$DOCKER_VERSION.tgz && \
	tar -zxvf /docker.tgz && cp /docker/docker* /usr/bin && rm -rf /docker /docker.tgz

# define HOME
ENV HOME /

# defiine on-demand command install path
ENV DEPLO_TOOLS_PATH=/deplo-tools
RUN mkdir -p ${DEPLO_TOOLS_PATH}/cloud
