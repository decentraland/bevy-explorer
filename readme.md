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
3. Install [protoc](https://github.com/protocolbuffers/protobuf/releases)
4. `cargo run --release`

# Arguments

`cargo run --release -- [--server serverpath] [--vsync true|false] [--log_fps true|false] [--msaa 1|2|4|8]`

`--server https://sdk-test-scenes.decentraland.zone`
- specify the content server, defaults to the sdk test server.

`--vsync [true|false]`
- disable/enable vsync. defaults to off.

`--msaa [1,2,4,8]`
- set the number of multisamples. higher values make for nicer graphics but takes more gpu power. defaults to 4.

`--threads n`
- set the max simultaneous thread count for scene javascript execution. higher will allow better performance for distant scenes, but requires more cpu power. defaults to 4.
- also accessible via console command `/scene_threads`

`--millis n`
- set the max time per frame which the renderer will wait for scene execution. higher values will increase distant scene performance at the possible cost of frame rate (particularly with lower thread count).
- note: if used together with `--vsync true` the time should be at least equal to 1000/monitor sync rate (i.e. 17 for 60hz, 6 for 144hz) to avoid very jerky behaviour.
- also accessible via console command `/scene_millis`
- defaults to 12, around 80fps

# Testing

`cargo test` executes all the tests.