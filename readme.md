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
4. `cargo run -- --scene scenes/example_scene/index.js`

# Arguments

The `--scene` argument takes 
- a pointer with format `x,y`, to load a single scene by parcel address
- a hash like `b64-L3Vzci9zcmMvYXBwL2dpdGh1Yi5jb21fZGVjZW50cmFsYW5kLXNjZW5lc19zZGs3LWdvZXJsaS1wbGF6YS0xNjc5OTk3ODkxMzA5LXJvdGF0aW5nLXBsYXRmb3Jtcw==` to load a scene by entity id
- a js file like `scenes/example_scene/index.js` to load a raw js file from the assets folder

`--server https://sdk-test-scenes.decentraland.zone`
- specify the content server, defaults to the sdk test server.

`--vsync [true|false]`

`--log_fps [true|false]`

# Testing

`cargo test` executes all the tests.