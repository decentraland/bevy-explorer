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
    Init,              // setup the scene
    PostInit,          // used for adding data to new scenes
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    RunLoop, // run the scripts
    PostLoop, // do anything after the script loop
    RestrictedActions, // can do crazy stuff like modify player position, oow etc
}

// sets within the scene processing loop (SceneSets::RunLoop)
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneLoopSets {
    SendToScene,      // pass data to the scene
    ReceiveFromScene, // receive data from the scene
    Lifecycle,        // manage bevy entity lifetimes
    UpdateWorld,      // systems which handle events from the current frame
}

// set for systems that deal with changes to realms
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct RealmLifecycle;

// PostUpdate ordering for systems that need to slot in around the avatar /
// camera / collider transform pipeline. Variants are chained in the order
// declared here by `TransformAndParentPlugin`.
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum PostUpdateSets {
    EarlyTransformPropagate,
    ColliderUpdate,
    PlayerUpdate,
    CameraUpdate,
    /// IK chain: foot-IK (pelvis drop / leg rotations) and head-IK (head
    /// bone gaze) plus a transform-propagate for the avatar subtree. Producers
    /// run `.in_set(InverseKinematics)`; consumers that need post-IK bone
    /// globals (e.g. nametag) run `.after(InverseKinematics)`.
    InverseKinematics,
    /// Per-frame nametag positioning. Reads post-IK head/position globals,
    /// so it sits after `InverseKinematics` in the chain.
    Nametag,
    AttachSync,
    Billboard,
}
