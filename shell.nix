let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  rust-overlay = (import (builtins.fetchGit {
    url = "https://github.com/oxalica/rust-overlay";
    ref = "master";
    rev = "7f3556887e3375dc26ff1601b57c93ee286f2c5e";
  }));
  pkgs = import nixpkgs { config = {}; overlays = [ rust-overlay ]; };
  system = builtins.currentSystem;
  extensions =
    (import (builtins.fetchGit {
      url = "https://github.com/nix-community/nix-vscode-extensions";
      ref = "master";
      rev = "d09b10414e1a10f7f9a9c9ec01d689d9d23d3f9c";
    })).extensions.${system};
  extensionsList = with extensions.vscode-marketplace; [
      rust-lang.rust-analyzer
      wgsl-analyzer.wgsl-analyzer
      tamasfe.even-better-toml
      usernamehw.errorlens
      fill-labs.dependi
      #vadimcn.vscode-lldb
      splo.vscode-bevy-inspector
      nefrob.vscode-just-syntax
      ms-vscode.hexeditor
  ];
  buildInputs = with pkgs; [
    udev
    alsa-lib
    libglvnd
    vulkan-loader
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    libxkbcommon
    wayland
    lldb
    typos
    taplo
    lld
    libva
    # FFMPEG
    clang
    libclang
    ffmpeg_6-full
    ffmpeg_6-full.dev
    nasm
    # Networking
    openssl
    openssl.dev
    protobuf
  ];
  buildInputsLdPath = pkgs.lib.makeLibraryPath buildInputs;
in
  pkgs.mkShell {
    nativeBuildInputs = with pkgs; [
      pkg-config
    ];
    inherit buildInputs;
    packages = with pkgs; [
      git
      (rust-bin.nightly."2025-06-03".default.override {
        extensions = ["rust-src" "clippy"];
        targets = [
          "x86_64-unknown-none"
          "wasm32-unknown-unknown"
        ];
      })
      (vscode-with-extensions.override {
        vscode = vscodium;
        vscodeExtensions = extensionsList;
      })
      just
      nodejs_24
      (obs-studio.override {
        cudaSupport = true;
      })
      wasm-pack
      wasm-bindgen-cli_0_2_100
    ];
    LD_LIBRARY_PATH = "${buildInputsLdPath}";
    LLDB_DEBUGSERVER_PATH = "${pkgs.lldb}/bin/lldb-server";
    NIXOS_OZONE_WL=1;
    FFMPEG_DIR = "${pkgs.ffmpeg_6-full.dev}";
  }
