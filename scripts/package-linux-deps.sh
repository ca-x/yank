#!/usr/bin/env bash
# Ubuntu 24.04: GTK 4.14 + GStreamer for NSH, desktop clipboard/input libraries.
set -euo pipefail
sudo apt-get -o Acquire::Retries=3 -o Dpkg::Lock::Timeout=120 update
sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
  build-essential pkg-config cmake clang llvm libclang-dev dpkg-dev \
  libgtk-4-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  libx11-dev libxcb1-dev libxcb-randr0-dev libxdo-dev libxtst-dev \
  libxfixes-dev libxrandr-dev libwayland-dev libxkbcommon-dev libdbus-1-dev \
  xvfb xauth dbus-x11
