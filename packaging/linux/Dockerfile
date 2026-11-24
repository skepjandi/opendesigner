# A Linux build and test environment for OpenDesigner: Ubuntu 24.04 with the distribution's Qt 6.
# Used by scripts/test-linux.sh. It is a check that the code builds and passes on Linux, not a
# release artifact.
FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config curl ca-certificates git lld \
        qt6-base-dev qt6-base-dev-tools qt6-declarative-dev qt6-multimedia-dev qt6-webengine-dev \
        qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
        qml6-module-qtquick-dialogs qml6-module-qtquick-window qml6-module-qtquick-templates \
        qml6-module-qtqml qml6-module-qtqml-workerscript qml6-module-qt-labs-folderlistmodel \
        qml6-module-qt-labs-platform qml6-module-qtcore qml6-module-qtmultimedia \
        qml6-module-qtwebengine \
        libgl1-mesa-dev libxkbcommon-dev libfontconfig1 fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup component add rustfmt clippy

ENV QMAKE=/usr/lib/qt6/bin/qmake6 QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    RUSTFLAGS="-C link-arg=-fuse-ld=lld"
WORKDIR /work
