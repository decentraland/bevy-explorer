use bevy::{prelude::*, utils::HashSet};
use bevy_mod_billboard::{text::BillboardTextBounds, BillboardTextBundle};
use common::{sets::SceneSets, util::TryInsertEx};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbTextShape, SceneComponentId};
use ui_core::TEXT_SHAPE_FONT;

use super::AddCrdtInterfaceExt;

pub struct TextShapePlugin;

impl Plugin for TextShapePlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTextShape, TextShape>(
            SceneComponentId::TEXT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.add_system(update_text_shapes.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct TextShape(pub PbTextShape);

impl From<PbTextShape> for TextShape {
    fn from(value: PbTextShape) -> Self {
        Self(value)
    }
}

fn update_text_shapes(
    mut commands: Commands,
    query: Query<(Entity, &TextShape), Changed<TextShape>>,
    existing: Query<&Parent, With<BillboardTextBounds>>,
    mut removed: RemovedComponents<TextShape>,
) {
    // remove changed and deleted nodes
    let old_parents = query
        .iter()
        .map(|(e, ..)| e)
        .chain(removed.iter())
        .collect::<HashSet<_>>();
    for par in existing.iter() {
        if old_parents.contains(&par.get()) {
            commands.entity(par.get()).despawn_recursive();
        }
    }

    // add new nodes
    for (ent, text_shape) in query.iter() {
        println!("text: {}", text_shape.0.text);
        println!("text style: {:?}", text_shape.0);
        commands.entity(ent).try_insert(BillboardTextBundle {
            text: Text::from_section(
                text_shape.0.text.as_str(),
                TextStyle {
                    font_size: text_shape.0.font_size.unwrap_or(10.0) * 10.0,
                    color: text_shape
                        .0
                        .text_color
                        .map(Into::into)
                        .unwrap_or(Color::WHITE),
                    font: TEXT_SHAPE_FONT.get().unwrap().clone(),
                },
            ),
            text_bounds: BillboardTextBounds {
                size: Vec2::new(
                    text_shape.0.width.map(|w| w * 100.0).unwrap_or(f32::MAX),
                    text_shape.0.height.map(|h| h * 100.0).unwrap_or(f32::MAX),
                ),
            },
            text_anchor: bevy::sprite::Anchor::BottomCenter,
            transform: Transform::from_scale(Vec3::splat(0.01)).with_translation(Vec3::Y),
            ..Default::default()
        });
    }
}
