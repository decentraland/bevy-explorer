use bevy::prelude::SystemSet;

// setup sets
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SetupSets {
    Init,
    Main,
}

// running sets used for ordering
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneSets {
    UiActions,
    Init,     // setup the scene
    PostInit, // used for adding data to new scenes
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    RunLoop, // run the scripts
    PostLoop, // do anything after the script loop
}

// sets within the scene processing loop (SceneSets::RunLoop)
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneLoopSets {
    SendToScene,      // pass data to the scene
    ReceiveFromScene, // receive data from the scene
    Lifecycle,        // manage bevy entity lifetimes
    UpdateWorld,      // systems which handle events from the current frame
}
