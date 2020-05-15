CMD=
REL=
ifeq ($(REL), 1)
DEPLO_PATH=$(CURDIR)/target/release/deplo
RELEASE=--release
else
DEPLO_PATH=$(CURDIR)/target/debug/deplo
RELEASE=
endif

build:
	cargo build $(RELEASE)

base_image: 
	docker build -t suntomi/deplo_base rsc/docker/base

image: base_image build
	cp $(DEPLO_PATH) rsc/docker/release/
	docker build -t suntomi/deplo rsc/docker/release

run:
	docker run --rm -ti -v $(CURDIR):/workdir -w /workdir suntomi/deplo $(CMD)
