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

`cargo run --release -- [--server serverpath] [--vsync true|false] [--log_fps true|false]`

`--server https://sdk-test-scenes.decentraland.zone`
- specify the content server, defaults to the sdk test server.

`--vsync [true|false]`

`--log_fps [true|false]`

# Testing

`cargo test` executes all the tests.