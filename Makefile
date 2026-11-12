HOST ?= 192.168.1.154
HTTP_PORT ?= 8080
ARCH := armv7-unknown-linux-musleabihf
RUST := $(HOME)/.cargo/bin/cargo
MIRAKC_SRC ?= $(HOME)/projects/mirakc-BS4K

.PHONY: all build mirakc image serve deploy up down check clean

all: build mirakc

build:
	cd rust && PATH=$(HOME)/.cargo/bin:$$PATH $(RUST) +1.99.0 build --release --target $(ARCH)
	mkdir -p .tmp/http
	cp rust/target/$(ARCH)/release/t4s-tuner rust/target/$(ARCH)/release/t4s-cas .tmp/http/

mirakc:
	PATH=$(HOME)/.cargo/bin:$$PATH $(RUST) +1.99.0 build -p mirakc --release --target $(ARCH) \
		--manifest-path $(MIRAKC_SRC)/Cargo.toml
	cp $(MIRAKC_SRC)/target/$(ARCH)/release/mirakc .tmp/http/mirakc

image: scripts/build-image.sh
	sh scripts/build-image.sh

serve:
	cd .tmp/http && python3 -m http.server $(HTTP_PORT) --bind 0.0.0.0

deploy: build mirakc
	sh scripts/deploy.sh $(HOST)

up:
	sh scripts/telnet-run.sh $(HOST) 'sh /media/usb/ondevice-up.sh'

down:
	sh scripts/telnet-run.sh $(HOST) 'sh /media/usb/ondevice-down.sh'

check:
	cd rust && PATH=$(HOME)/.cargo/bin:$$PATH $(RUST) +1.99.0 check

clean:
	rm -rf rust/target .tmp/img .tmp/http
