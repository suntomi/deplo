# command line argument
CMD=
REL=

# settings
LINUX_TARGET=x86_64-unknown-linux-musl
DARWIN_TARGET=x86_64-apple-darwin
RESOURCE_FILE_PATH=$(CURDIR)/rsc
IMAGE_BUILD_ROOT_PATH=$(CURDIR)/docker/release
GIT_HASH=$(shell git rev-parse HEAD)
ifeq ($(REL), 1)
BUILD_PROFILE=release
RELEASE=--release
else
BUILD_PROFILE=debug
RELEASE=
endif
DEPLO_LINUX=$(CURDIR)/target/$(LINUX_TARGET)/$(BUILD_PROFILE)/deplo
DEPLO_DARWIN=$(CURDIR)/target/$(DARWIN_TARGET)/$(BUILD_PROFILE)/deplo


build:
	[ -z "$(shell git diff --name-status)" ] || (echo "you have uncommited changes" && exit 1)
	cross build $(RELEASE) --target $(LINUX_TARGET)
	cross build $(RELEASE) --target $(DARWIN_TARGET)

base: 
	docker build -t suntomi/deplo_base docker/base

shell:
	cp $(CURDIR)/Cargo.* docker/shell
	docker build -t suntomi/deplo_shell docker/shell

image: base build
	cp $(DEPLO_LINUX) $(IMAGE_BUILD_ROOT_PATH)
	-rm -r $(IMAGE_BUILD_ROOT_PATH)/rsc
	cp -r $(RESOURCE_FILE_PATH) $(IMAGE_BUILD_ROOT_PATH)/
	mkdir -p $(IMAGE_BUILD_ROOT_PATH)/rsc/bin
	cp $(DEPLO_DARWIN) $(IMAGE_BUILD_ROOT_PATH)/rsc/bin/deplo_darwin
	docker build -t suntomi/deplo $(IMAGE_BUILD_ROOT_PATH)

deploy:
	docker tag suntomi/deplo:latest suntomi/deplo:$(GIT_HASH)
	docker push suntomi/deplo:$(GIT_HASH)

dev:
	cargo run -- -vv -w /workdir/test/projects/dev $(CMD)

sh:
	docker run --rm -ti -w /workdir \
		-v $(CURDIR):/workdir \
		-v $(CURDIR)/.deplo-tools:/deplo-tools:delegated \
		-v $(HOME)/.cargo/registry:/.cargo/registry \
		-v /var/run/docker.sock:/var/run/docker.sock \
		suntomi/deplo_shell sh

run:
	DEPLO_CI_TYPE=Circle cargo run -- \
		-w test/projects/dev -d skip_rebase -d force_set_release_target_to=dev -vvv \
		$(CMD)