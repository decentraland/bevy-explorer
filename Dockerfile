# Base image
FROM ubuntu:22.04

# Set environment variables
ENV DEBIAN_FRONTEND=noninteractive
ENV RUST_VERSION=stable
ENV TARGET_DIR=/usr/src/app/target

# Install dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    clang \
    pkg-config \
    libssl-dev \
    libasound2-dev \
    libudev-dev \
    libx11-dev \
    libgl1-mesa-dev \
    libxext-dev \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    libavfilter-dev \
    libavdevice-dev \
    libegl1-mesa \
    libgl1-mesa-dri \
    libxcb-xfixes0-dev \
    mesa-vulkan-drivers \
    xvfb \
    git \
    unzip \
    ca-certificates \
    xfce4 xfce4-terminal \
    x11vnc novnc \
    supervisor \
    nvidia-driver-525 \
    && apt-get clean

# Update CA certificates
RUN update-ca-certificates

# Install Protobuf
RUN curl -LO https://github.com/protocolbuffers/protobuf/releases/download/v21.12/protoc-21.12-linux-x86_64.zip && \
    unzip protoc-21.12-linux-x86_64.zip -d /usr/local && \
    rm protoc-21.12-linux-x86_64.zip

# Install Rust
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y --default-toolchain $RUST_VERSION
ENV PATH="/root/.cargo/bin:${PATH}"

# Create app directory and set up cache for the target folder
WORKDIR /usr/src/app

# Copy the Rust project
COPY . /usr/src/app

# Pre-build dependencies to speed up builds
RUN cargo fetch

# Build the project in release mode
RUN cargo build --release

# Copy Supervisor configuration
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf

# Expose necessary ports
EXPOSE 8080

# Start Supervisor to manage services
CMD ["/usr/bin/supervisord", "-n", "--loglevel", "debug"]
