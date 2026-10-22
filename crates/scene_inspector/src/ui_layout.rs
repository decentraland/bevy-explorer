use bevy::{
    prelude::*,
    ui::{CalculatedClip, ComputedNode, UiStack},
    window::PrimaryWindow,
};
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::PbUiCanvasInformation, DclReader, FromDclReader,
    SceneComponentId, SceneEntityId,
};
use scene_runner::{
    update_world::scene_ui::{ui_text::UiText, UiLink, UiTransform},
    MobileUiEmulation, SceneEntity,
};
use serde_json::{json, Value};

use crate::active_scene::SceneResolver;

pub fn add_ui_layout_command(app: &mut App) {
    app.add_console_command::<UiLayoutCommand, _>(ui_layout_cmd);
    app.add_console_command::<MobileUiCommand, _>(mobile_ui_cmd);
}

/// Lay scene UI out as on a 1600x720 phone: the scene is told it runs on mobile and gets the
/// mobile client's interactable area, inside a phone-shaped root fitted in the window. Reload the
/// scene after switching, it reads the platform when it starts. Native touch controls are not drawn.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/mobile_ui")]
struct MobileUiCommand {
    /// on or off (omit to read the current state)
    state: Option<String>,
}

fn mobile_ui_cmd(
    mut input: ConsoleCommand<MobileUiCommand>,
    mut mobile: ResMut<MobileUiEmulation>,
) {
    let Some(Ok(cmd)) = input.take() else {
        return;
    };
    match cmd.state.as_deref() {
        None => {}
        Some("on") => mobile.0 = true,
        Some("off") => mobile.0 = false,
        Some(other) => {
            input.reply_failed(format!("expected on or off, got '{other}'"));
            return;
        }
    }
    input.reply_ok(if mobile.0 { "on" } else { "off" });
}

/// Return the computed layout of the active scene's screen UI as JSON: the canvas the scene was
/// told about, and every on-screen UI node with its rect after layout, in canvas units
/// (logical px, origin at the scene UI root's top-left). Reads the last completed layout, so call
/// it on a settled frame.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/ui_layout")]
struct UiLayoutCommand;

#[allow(clippy::type_complexity)]
fn ui_layout_cmd(
    mut input: ConsoleCommand<UiLayoutCommand>,
    resolver: SceneResolver,
    window: Query<&Window, With<PrimaryWindow>>,
    scene_nodes: Query<(&SceneEntity, &UiLink, &UiTransform, Option<&UiText>)>,
    ui_nodes: Query<(
        &ComputedNode,
        &GlobalTransform,
        &InheritedVisibility,
        Option<&CalculatedClip>,
    )>,
    hierarchy: Query<(&Node, Option<&ChildOf>)>,
    links: Query<&UiLink>,
    stack: Res<UiStack>,
    mobile: Res<MobileUiEmulation>,
) {
    let Some(Ok(_)) = input.take() else {
        return;
    };
    let (scene_ent, ctx) = match resolver.resolve() {
        Ok(found) => found,
        Err(e) => {
            input.reply_failed(e);
            return;
        }
    };

    let canvas = ctx
        .crdt_store
        .get(
            SceneComponentId::CANVAS_INFO,
            CrdtType::LWW_ROOT,
            SceneEntityId::ROOT,
        )
        .and_then(|data| PbUiCanvasInformation::from_reader(&mut DclReader::new(data)).ok())
        .map(|info| {
            let area = info.interactable_area.unwrap_or_default();
            json!({
                "width": info.width,
                "height": info.height,
                "devicePixelRatio": info.device_pixel_ratio,
                "interactableArea": { "left": area.left, "top": area.top, "right": area.right, "bottom": area.bottom },
            })
        });

    let window = window.single().ok().map(|w| {
        json!({
            "width": w.resolution.width(),
            "height": w.resolution.height(),
            "scaleFactor": w.resolution.scale_factor(),
        })
    });

    // under /mobile_ui (or a constrained UI) the scene's root is a box inside the window
    let origin = links
        .get(scene_ent)
        .ok()
        .and_then(|root| ui_nodes.get(root.ui_entity).ok())
        .map(|(computed, global, ..)| {
            (global.translation().truncate() - computed.size() / 2.0)
                * computed.inverse_scale_factor()
        })
        .unwrap_or_default();

    let order = |ui_entity: Entity| stack.uinodes.iter().position(|e| *e == ui_entity);
    // a node inside a display:none ancestor keeps its last computed size, so walk up
    let displayed = |mut ui_entity: Entity| loop {
        let Ok((node, parent)) = hierarchy.get(ui_entity) else {
            return true;
        };
        if node.display == Display::None {
            return false;
        }
        match parent {
            Some(parent) => ui_entity = parent.parent(),
            None => return true,
        }
    };
    let round = |v: f32| (v * 10.0).round() / 10.0;
    let rect_json = |r: Rect| {
        json!([
            round(r.min.x),
            round(r.min.y),
            round(r.width()),
            round(r.height())
        ])
    };
    let entity_id = |id: SceneEntityId| id.as_proto_u32().unwrap_or_default();

    let mut nodes: Vec<(usize, Value)> = Vec::new();
    for (scene_entity, link, transform, text) in scene_nodes.iter() {
        if scene_entity.root != scene_ent || !link.is_window_ui() {
            continue;
        }
        let Ok((computed, global, visibility, clip)) = ui_nodes.get(link.ui_entity) else {
            continue;
        };
        // bevy_ui works in physical px; the scene's canvas is logical px
        let scale = computed.inverse_scale_factor();
        let rect = Rect::from_center_size(global.translation().truncate(), computed.size());
        let logical = |r: Rect| Rect::from_corners(r.min * scale - origin, r.max * scale - origin);
        let visible_rect = match clip {
            Some(clip) => rect.intersect(clip.clip),
            None => rect,
        };
        let visible = visibility.get() && displayed(link.ui_entity) && !visible_rect.is_empty();
        let mut pointer: Vec<&str> = link.interactors().collect();
        pointer.sort_unstable();
        let stack_order = order(link.ui_entity);

        nodes.push((
            stack_order.unwrap_or(usize::MAX),
            json!({
                "entity": entity_id(scene_entity.id),
                "parent": entity_id(transform.parent()),
                "elementId": transform.element_id(),
                "order": stack_order,
                "zIndex": transform.zindex(),
                "rect": rect_json(logical(rect)),
                "visibleRect": if visible { rect_json(logical(visible_rect)) } else { Value::Null },
                "visible": visible,
                "clipped": visible && visible_rect != rect,
                "pointer": pointer,
                "text": text.map(|t| t.text.as_str()),
                "fontSize": text.map(|t| t.font_size),
            }),
        ));
    }
    nodes.sort_by_key(|(order, _)| *order);

    let out = json!({
        "scene": { "hash": ctx.hash, "title": ctx.title },
        "window": window,
        "mobileEmulation": mobile.0,
        "canvas": canvas,
        "nodes": nodes.into_iter().map(|(_, node)| node).collect::<Vec<_>>(),
    });
    input.reply_ok(out.to_string());
}
