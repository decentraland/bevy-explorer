//! Hand-authored semantic overlay (host/build-time only). Carries what reflection can't:
//! semantic kinds, ranges, curated (runtime) defaults, units, refs, and per-component
//! placement/requires. Keyed by component name + camelCase dotted path (oneof cases as
//! `oneof.case.field`, repeated elements as `field[]`), matching CATALOG.md.

use std::collections::HashMap;

use serde_json::{json, Value};

pub struct FieldOverlay {
    pub path: &'static str,
    pub semantic: Option<&'static str>,
    pub range: Option<(Option<f64>, Option<f64>, bool)>, // (min, max, hard)
    pub default: Option<&'static str>,                   // JSON fragment
    pub notes: Option<&'static str>,
}

pub struct ComponentOverlay {
    pub placement: &'static str,
    pub requires: &'static [(&'static str, &'static str, bool)], // (component, locality, hard)
    pub fields: Vec<FieldOverlay>,
}

fn fo(path: &'static str) -> FieldOverlay {
    FieldOverlay {
        path,
        semantic: None,
        range: None,
        default: None,
        notes: None,
    }
}
impl FieldOverlay {
    fn sem(mut self, s: &'static str) -> Self {
        self.semantic = Some(s);
        self
    }
    fn min(mut self, min: f64, hard: bool) -> Self {
        self.range = Some((Some(min), None, hard));
        self
    }
    fn rng(mut self, min: f64, max: f64, hard: bool) -> Self {
        self.range = Some((Some(min), Some(max), hard));
        self
    }
    fn def(mut self, d: &'static str) -> Self {
        self.default = Some(d);
        self
    }
    fn note(mut self, n: &'static str) -> Self {
        self.notes = Some(n);
        self
    }
}

fn comp(
    map: &mut HashMap<&'static str, ComponentOverlay>,
    name: &'static str,
    placement: &'static str,
    requires: &'static [(&'static str, &'static str, bool)],
    fields: Vec<FieldOverlay>,
) {
    map.insert(
        name,
        ComponentOverlay {
            placement,
            requires,
            fields,
        },
    );
}

const NONE_REQ: &[(&str, &str, bool)] = &[];

pub fn overlay() -> HashMap<&'static str, ComponentOverlay> {
    let mut m = HashMap::new();

    comp(
        &mut m,
        "Tween",
        "any",
        NONE_REQ,
        vec![
            fo("duration")
                .sem("number:ms")
                .min(0.0, true)
                .def("1000")
                .note(
                    "set to 0 for continuous modes, else the tween terminates after this many ms",
                ),
            fo("currentTime").sem("number:unit01").rng(0.0, 1.0, true),
            fo("playing").def("true"), // both clients default unset->true
            // non-continuous modes: start/end seed from the entity's current Transform
            fo("mode.move.start").def(r#""@transform.position""#),
            fo("mode.move.end").def(r#""@transform.position""#),
            fo("mode.rotate.start").def(r#""@transform.rotation""#),
            fo("mode.rotate.end").def(r#""@transform.rotation""#),
            fo("mode.scale.start").def(r#""@transform.scale""#),
            fo("mode.scale.end").def(r#""@transform.scale""#),
            // continuous modes: speed defaults to 1 (duration/easing/currentTime are inert
            // for continuous — keep duration 0 there for true-continuous behaviour)
            fo("mode.rotateContinuous.speed")
                .sem("number:degrees")
                .def("90")
                .note("degrees per second"),
            fo("mode.moveContinuous.speed").def("1"),
            fo("mode.textureMoveContinuous.speed").def("1"),
        ],
    );

    comp(
        &mut m,
        "Material",
        "any",
        &[("MeshRenderer", "same", false), ("GltfNode", "same", false)],
        vec![
            fo("material.unlit.alphaTest")
                .sem("number:unit01")
                .rng(0.0, 1.0, true),
            fo("material.pbr.alphaTest")
                .sem("number:unit01")
                .rng(0.0, 1.0, true),
            fo("material.pbr.metallic")
                .sem("number:unit01")
                .rng(0.0, 1.0, false),
            fo("material.pbr.roughness")
                .sem("number:unit01")
                .rng(0.0, 1.0, false),
            fo("gltf.gltfSrc").sem("contentFile:gltf"),
        ],
    );

    comp(
        &mut m,
        "GltfContainer",
        "any",
        NONE_REQ,
        vec![
            fo("src").sem("contentFile:gltf"),
            fo("visibleMeshesCollisionMask").sem("bitmask:ColliderLayer"),
            fo("invisibleMeshesCollisionMask").sem("bitmask:ColliderLayer"),
        ],
    );

    comp(
        &mut m,
        "MeshRenderer",
        "any",
        NONE_REQ,
        vec![
            fo("mesh.gltf.gltfSrc").sem("contentFile:gltf"),
            fo("mesh.box.uvs").sem("uvArray:48"),
            fo("mesh.plane.uvs").sem("uvArray:16"),
        ],
    );

    comp(
        &mut m,
        "MeshCollider",
        "any",
        NONE_REQ,
        vec![
            fo("collisionMask").sem("bitmask:ColliderLayer"),
            fo("mesh.gltf.gltfSrc").sem("contentFile:gltf"),
        ],
    );

    comp(
        &mut m,
        "NftShape",
        "any",
        NONE_REQ,
        vec![fo("urn").sem("urn:nft")],
    );

    comp(
        &mut m,
        "Animator",
        "any",
        &[("GltfContainer", "same", true)],
        vec![fo("states[].clip").sem("gltfAnimationName")],
    );

    comp(
        &mut m,
        "GltfNode",
        "any",
        &[("GltfContainer", "ancestor", true)],
        vec![fo("path").sem("gltfNodePath")],
    );

    comp(
        &mut m,
        "GltfNodeModifiers",
        "any",
        &[("GltfContainer", "same", true)],
        vec![fo("modifiers[].path").sem("gltfNodePath")],
    );

    comp(
        &mut m,
        "AssetLoad",
        "any",
        NONE_REQ,
        vec![fo("assets[]").sem("contentFile:any")],
    );

    comp(
        &mut m,
        "MainCamera",
        "camera",
        &[("VirtualCamera", "same", false)],
        vec![fo("virtualCameraEntity").sem("entityRef:VirtualCamera")],
    );

    comp(
        &mut m,
        "VirtualCamera",
        "any",
        NONE_REQ,
        vec![fo("lookAtEntity").sem("entityRef:any")],
    );

    comp(
        &mut m,
        "CameraLayer",
        "any",
        NONE_REQ,
        vec![fo("layer").sem("cameraLayerId").min(1.0, true)],
    );

    comp(
        &mut m,
        "CameraLayers",
        "any",
        NONE_REQ,
        vec![fo("layers[]").sem("cameraLayerId")],
    );

    comp(
        &mut m,
        "TextureCamera",
        "any",
        NONE_REQ,
        vec![
            fo("width")
                .sem("uint:px")
                .rng(16.0, 2048.0, true)
                .def("256"),
            fo("height")
                .sem("uint:px")
                .rng(16.0, 2048.0, true)
                .def("256"),
            fo("layer").sem("cameraLayerId"),
            fo("farPlane")
                .sem("number:meters")
                .min(0.0, false)
                .def("240")
                .note("runtime default 240m (proto comment says infinity)"),
            fo("mode.perspective.fieldOfView")
                .sem("number:radians")
                .min(0.0, false),
            fo("mode.orthographic.verticalRange")
                .sem("number:meters")
                .min(0.0, false),
        ],
    );

    comp(
        &mut m,
        "AudioSource",
        "any",
        NONE_REQ,
        vec![fo("audioClipUrl").sem("urlOrContent:audio")],
    );

    comp(
        &mut m,
        "AudioStream",
        "any",
        NONE_REQ,
        vec![
            fo("url").sem("url"),
            fo("playing")
                .def("true")
                .note("runtime defaults unset->true"),
        ],
    );

    comp(
        &mut m,
        "VideoPlayer",
        "any",
        NONE_REQ,
        vec![fo("src").sem("urlOrContent:video")],
    );

    comp(
        &mut m,
        "LightSource",
        "any",
        NONE_REQ,
        vec![
            fo("active").def("true"),
            fo("color").def(r#"{"r":1,"g":1,"b":1}"#),
            fo("intensity")
                .sem("number:candela")
                .min(0.0, false)
                .def("16000")
                .note("runtime substitutes 16000 when unset (proto comment says 100)"),
            fo("range")
                .sem("number:meters")
                .def("-1")
                .note("negative (default -1) = auto pow(intensity,0.25); 0 = disabled (no reach); >0 = range (bevy caps at pow(intensity,0.25))"),
            fo("type.spot.innerAngle")
                .sem("number:degrees")
                .rng(0.0, 179.0, true)
                .def("21.8"),
            fo("type.spot.outerAngle")
                .sem("number:degrees")
                .rng(0.0, 179.0, true)
                .def("30"),
        ],
    );

    comp(
        &mut m,
        "Billboard",
        "any",
        NONE_REQ,
        vec![fo("billboardMode")
            .sem("bitmask:BillboardMode")
            .note("constrained: runtime only distinguishes {None, Y, X|Y, All}")],
    );

    comp(
        &mut m,
        "SkyboxTime",
        "root",
        NONE_REQ,
        vec![fo("fixedTime")
            .sem("number:seconds")
            .rng(0.0, 86400.0, false)
            .note("seconds-of-day; time-of-day picker")],
    );

    comp(&mut m, "GlobalLight", "root", NONE_REQ, vec![]);

    comp(
        &mut m,
        "Raycast",
        "any",
        NONE_REQ,
        vec![
            fo("maxDistance").sem("number:meters").min(0.0, true),
            fo("collisionMask").sem("bitmask:ColliderLayer"),
            fo("direction.targetEntity").sem("entityRef:any"),
        ],
    );

    comp(
        &mut m,
        "PointerEvents",
        "any",
        NONE_REQ,
        vec![
            fo("pointerEvents[].eventInfo.maxDistance").sem("number:meters"),
            fo("pointerEvents[].eventInfo.maxPlayerDistance").sem("number:meters"),
        ],
    );

    comp(
        &mut m,
        "CameraModeArea",
        "any",
        NONE_REQ,
        vec![
            fo("cinematicSettings.cameraEntity").sem("entityRef:any"),
            fo("cinematicSettings.yawRange").sem("number:radians"),
            fo("cinematicSettings.pitchRange").sem("number:radians"),
            fo("cinematicSettings.rollRange").sem("number:radians"),
        ],
    );

    comp(
        &mut m,
        "TriggerArea",
        "any",
        NONE_REQ,
        vec![fo("collisionMask").sem("bitmask:ColliderLayer")],
    );

    comp(
        &mut m,
        "AvatarAttach",
        "any",
        NONE_REQ,
        vec![fo("avatarId").sem("userRef")],
    );

    comp(
        &mut m,
        "AvatarShape",
        "any",
        NONE_REQ,
        vec![
            fo("id").sem("userRef"),
            fo("bodyShape").sem("urn:wearable"),
            fo("wearables[]").sem("urn:wearable"),
            fo("emotes[]").sem("urn:emote"),
        ],
    );

    comp(
        &mut m,
        "AvatarModifierArea",
        "any",
        NONE_REQ,
        vec![fo("excludeIds[]").sem("userRef")],
    );

    comp(
        &mut m,
        "AvatarMovement",
        "any",
        NONE_REQ,
        vec![
            fo("orientation")
                .sem("number:degrees")
                .rng(0.0, 360.0, true),
            fo("animation.src").sem("contentFile:gltf"),
            fo("animation.sounds[]").sem("contentFile:audio"),
        ],
    );

    comp(
        &mut m,
        "AvatarLocomotionSettings",
        "player",
        NONE_REQ,
        vec![],
    );
    comp(&mut m, "InputModifier", "player", NONE_REQ, vec![]);
    comp(&mut m, "PointerLock", "camera", NONE_REQ, vec![]);

    comp(
        &mut m,
        "TextShape",
        "any",
        NONE_REQ,
        vec![
            fo("width").sem("number:meters").min(0.0, false),
            fo("height").sem("number:meters").min(0.0, false),
            fo("outlineWidth").sem("number:unit01").rng(0.0, 1.0, false),
        ],
    );

    comp(
        &mut m,
        "UiCanvas",
        "uiRoot",
        NONE_REQ,
        vec![fo("width").sem("uint:px"), fo("height").sem("uint:px")],
    );

    comp(
        &mut m,
        "UiTransform",
        "uiEntity",
        &[("UiTransform", "ancestor", false)],
        vec![
            fo("parent").sem("entityRef:UiTransform"),
            fo("rightOf").sem("entityRef:UiTransform"),
        ],
    );

    comp(
        &mut m,
        "UiText",
        "uiEntity",
        &[("UiTransform", "same", true)],
        vec![],
    );
    comp(
        &mut m,
        "UiInput",
        "uiEntity",
        &[("UiTransform", "same", true)],
        vec![],
    );
    comp(
        &mut m,
        "UiDropdown",
        "uiEntity",
        &[("UiTransform", "same", true)],
        vec![],
    );
    comp(
        &mut m,
        "UiBackground",
        "uiEntity",
        &[("UiTransform", "same", true)],
        vec![fo("uvs").sem("uvArray:8")],
    );

    m
}

/// Transform is not a proto message — authored directly.
pub fn transform_schema() -> Option<Value> {
    Some(json!({
        "name": "Transform",
        "placement": "any",
        "readOnly": false,
        "requires": [],
        "root": { "kind": "message", "fields": [
            { "name": "position", "kind": "leaf", "semantic": "vector3", "optional": false,
              "default": { "x": 0, "y": 0, "z": 0 } },
            { "name": "rotation", "kind": "leaf", "semantic": "quaternion", "optional": false,
              "default": { "x": 0, "y": 0, "z": 0, "w": 1 } },
            { "name": "scale", "kind": "leaf", "semantic": "vector3", "optional": false,
              "default": { "x": 1, "y": 1, "z": 1 } },
            { "name": "parent", "kind": "leaf", "semantic": "entityRef:any", "optional": false,
              "default": 0, "notes": "parent entity; 0 = scene root" }
        ] },
        "enums": {}
    }))
}
