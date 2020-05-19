# command line argument
CMD=
REL=
TARGET=x86_64-unknown-linux-musl

# settings
TARGET_PATH=$(CURDIR)/target/$(TARGET)
INFRA_SCRIPT_PATH=$(CURDIR)/rsc/infra
ifeq ($(REL), 1)
DEPLO_PATH=$(TARGET_PATH)/release/deplo
RELEASE=--release
else
DEPLO_PATH=$(TARGET_PATH)/debug/deplo
RELEASE=
endif

build:
	cargo build $(RELEASE) --target $(TARGET)

base: 
	docker build -t suntomi/deplo_base rsc/docker/base

shell: 
	cp $(CURDIR)/Cargo.* rsc/docker/shell
	docker build -t suntomi/deplo_shell rsc/docker/shell

image: base build
	cp $(DEPLO_PATH) rsc/docker/release/
	-rm -r rsc/docker/release/rsc
	cp -r $(INFRA_SCRIPT_PATH) rsc/docker/release/rsc
	docker build -t suntomi/deplo rsc/docker/release

run:
	docker run --rm -ti -v $(CURDIR):/workdir -w /workdir suntomi/deplo $(CMD)

sh:
	docker run --rm -ti -w /workdir \
		-v $(CURDIR):/workdir \
		-v $(HOME)/.cargo/registry:/.cargo/registry \
		suntomi/deplo_shell sh
