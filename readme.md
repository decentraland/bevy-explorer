# bevy-explorer

A forward-looking implementation of the Decentraland protocol.

This implementation uses [rust](https://www.rust-lang.org/) and the [Bevy](https://bevyengine.org) engine, and targets desktop clients.

This project's goals are to:
- document current and future protocol standards
- experiment with changes to the protocol
- increase the field of alternative Explorers
- prioritize solid fundamentals, extensibility, and the use of modern open-source frameworks

# Building and Running

1. Clone the repo using `git clone https://github.com/decentraland/bevy-explorer`
2. Install [rust](https://www.rust-lang.org/tools/install)
3. Download and install third party libraries
    - on linux:
      - *note: livekit networking (main-realm transport) in the linux build is temporarily disabled due to conflicting imports in webrtc and deno. we hope this will be resolved soon*
      - Install alsa and udev: `sudo apt-get update; sudo apt-get install --no-install-recommends libasound2-dev libudev-dev`
      - Install ffmpeg deps: `sudo apt install -y --no-install-recommends clang curl pkg-config libavcodec-dev libavformat-dev libavutil-dev libavfilter-dev libavdevice-dev`
      - (*not needed currently*) Install Livekit deps: `sudo apt update -y; sudo apt install -y libssl-dev libx11-dev libgl1-mesa-dev libxext-dev`
    - on macos: 
      - `brew install ffmpeg@6 pkg-config`
      - `export PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@6/lib/pkgconfig`
    - on windows: 
      - download and unzip `https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-6.0-full_build-shared.7z`
      - set `LIBCLANG_PATH` = `path to LLVM\x64\bin` (this is packaged with visual studio, or can be downloaded separately)
      - set `FFMPEG_DIR` = `root folder where ffmpeg has been unzipped`
      - add `ffmpeg\bin` to your `PATH`
4. Install [protoc](https://github.com/protocolbuffers/protobuf/releases)
5. `cargo run --release --bin decentra-bevy`

We try to keep these instructions up to date, but the [github ci](.github/workflows/ci.yml) is the most accurate source of build information.

# Arguments

`cargo run --release --bin decentra-bevy -- [options]`

`--server https://sdk-test-scenes.decentraland.zone`
- specify the content server, defaults to the sdk test server.

`--location 52,-52`
- specify the parcel at which to spawn.

`--vsync (true|false)`
- disable/enable vsync. defaults to off.

`--fps (number)`
- set target fps. defaults to 60. if vsync is true this will be overridden by the vsync refresh rate. also accessible via console `/fps` command.

`--msaa [1,2,4,8]`
- set the number of multisamples. higher values make for nicer graphics but takes more gpu power. defaults to 4.

`--threads n`
- set the max simultaneous thread count for scene javascript execution. higher will allow better performance for distant scenes, but requires more cpu power. defaults to 4.
- also accessible via console command `/scene_threads`

`--distance n`
- set the distance (in meters) at which scenes will be loaded. defaults to 100.0.
- also accessible via console command `/scene_distance`

`--no_gltf`
- disable gltf loading.

`--no_avatar`
- disable avatar rendering.

`--no_fog`
- disable distance fog

`--inspect <scene_hash>`
- when the scene with the input hash is first loaded, the js runtime will pause waiting for a debugger session (such as `chrome://inspect`) to connect, and allow you to debug the scene code. requires a build with --features "inspect"

# Testing

`cargo test --all` executes all the tests.


Powered by the Decentraland DAO
![Decentraland DAO logo](https://bafkreibci6gg3wbjvxzlqpuh353upzrssalqqoddb6c4rez33bcagqsc2a.ipfs.nftstorage.link/)
