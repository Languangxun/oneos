CARGO ?= $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
MKOSI ?= mkosi
SUDO ?= sudo
MUSL := x86_64-unknown-linux-musl
BIN_DIR := mkosi.extra/usr/bin
DEV_SOCKET ?= /tmp/oneos-dev.sock

.PHONY: all deps genkey build image run run-serial debug ssh shell dev fmt lint test clean

all: image

deps:
	sudo apt-get update
	sudo apt-get install -y mkosi qemu-system-x86 systemd-boot-efi systemd-ukify systemd-repart \
		ovmf mtools squashfs-tools debian-archive-keyring zstd xz-utils gnupg python3 \
		dosfstools e2fsprogs

genkey:
	$(SUDO) $(MKOSI) genkey

build:
	$(CARGO) build --release --target $(MUSL) -p oneosd -p oneos -p oneos-splash
	mkdir -p $(BIN_DIR)
	cp target/$(MUSL)/release/oneosd $(BIN_DIR)/
	cp target/$(MUSL)/release/oneos $(BIN_DIR)/
	cp target/$(MUSL)/release/oneos-splash $(BIN_DIR)/

image: build
	$(SUDO) $(MKOSI) -f

run:
	$(SUDO) env PIPEWIRE_RUNTIME_DIR=/run/user/$(shell id -u) $(MKOSI) vm

run-serial:
	$(SUDO) $(MKOSI) --console=interactive vm

debug:
	$(SUDO) $(MKOSI) --console=interactive --qemu-args="-device virtio-vga" vm

ssh:
	$(SUDO) $(MKOSI) ssh

shell:
	$(SUDO) $(MKOSI) shell

dev:
	ONEO_SOCKET=$(DEV_SOCKET) ./scripts/dev.sh $(ARGS)

fmt:
	$(CARGO) fmt --all

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test:
	$(CARGO) test --workspace

clean:
	$(CARGO) clean
	rm -rf $(BIN_DIR) mkosi.output
