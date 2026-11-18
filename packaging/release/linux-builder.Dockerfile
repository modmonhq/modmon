FROM rust:1.99-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c

RUN printf '%s\n' \
        'Types: deb' \
        'URIs: http://snapshot.debian.org/archive/debian/20261101T000000Z' \
        'Suites: bookworm' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg' \
        'Check-Valid-Until: no' \
        >/etc/apt/sources.list.d/debian.sources \
    && rm -f /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross qemu-user \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add --toolchain 1.99.1 aarch64-unknown-linux-gnu

ENV RUSTUP_TOOLCHAIN=1.99.1 \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar \
    QEMU_LD_PREFIX=/usr/aarch64-linux-gnu
