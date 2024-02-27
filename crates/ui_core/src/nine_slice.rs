use anyhow::{anyhow, bail};
use bevy::prelude::*;
use bevy_dui::{DuiContext, DuiProps, DuiRegistry, DuiTemplate, NodeMap};
use bevy_ecss::StyleSheetAsset;
// use common::util::TryInsertEx;

/// specify a background image using 9-slice scaling
/// https://en.wikipedia.org/wiki/9-slice_scaling
/// must be added to an entity with `NodeBundle` components
#[derive(Component, Default)]
pub struct Ui9Slice {
    /// the image to be sliced
    pub image: Handle<Image>,
    /// rect defining the edges of the center / stretched region
    /// Val::Px uses so many pixels
    /// Val::Percent uses a percent of the image size
    /// Val::Auto and Val::Undefined are treated as zero.
    pub center_region: UiRect,
    pub tint: Option<BackgroundColor>,
}

impl Ui9Slice {
    pub fn new(image: Handle<Image>, center_region: UiRect, tint: Option<BackgroundColor>) -> Self {
        Self {
            image,
            center_region,
            tint,
        }
    }
}

#[derive(SystemSet, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Ui9SliceSet;

pub struct Ui9SlicePlugin;

impl Plugin for Ui9SlicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_systems(Update, update_slices.in_set(Ui9SliceSet));
    }
}

#[derive(Component)]
struct SliceInitMarker;

struct ItemData {
    grow: f32,
    start: Val,
    end: Val,
    outer_size: Val,
    inner_size: Val,
}

#[allow(clippy::type_complexity)]
fn update_slices(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    new_slices: Query<
        (Entity, &Ui9Slice, Option<&Children>),
        Or<(Changed<Ui9Slice>, Added<Ui9Slice>, Without<SliceInitMarker>)>,
    >,
    existing_slices: Query<(), With<SliceInitMarker>>,
    children_query: Query<&Children>,
    mut style_query: Query<(&mut Style, Option<&Children>)>,
    mut removed: RemovedComponents<Ui9Slice>,
) {
    // clean up removed slices
    for ent in removed.read() {
        if let Ok(children) = children_query.get(ent) {
            if let Some(slice_ent) = children
                .iter()
                .find(|child| existing_slices.get(**child).is_ok())
            {
                commands.entity(*slice_ent).despawn_recursive();
            }
        }
    }

    for (ent, slice, maybe_children) in new_slices.iter() {
        // need the image size to set the patch sizes
        let Some(image_data) = images.get(&slice.image) else {
            continue;
        };

        // calculate sizes
        let image_size = image_data.size_f32();

        let top_px = slice
            .center_region
            .top
            .resolve(image_size.y, Vec2::ZERO)
            .unwrap_or(0.0);
        let bottom_px = slice
            .center_region
            .bottom
            .resolve(image_size.y, Vec2::ZERO)
            .unwrap_or(0.0);
        let middle_height_px = (image_size.y - top_px - bottom_px).max(0.01);

        let row_data = [
            ItemData {
                grow: 0.0,
                start: Val::Px(0.0),
                end: Val::Auto,
                outer_size: Val::Px(top_px),
                inner_size: Val::Px(image_size.y),
            },
            ItemData {
                grow: 1.0,
                start: Val::Percent(-100.0 * top_px / middle_height_px),
                end: Val::Auto,
                outer_size: Val::Auto,
                inner_size: Val::Percent(100.0 * image_size.y / middle_height_px),
            },
            ItemData {
                grow: 0.0,
                start: Val::Auto,
                end: Val::Px(0.0),
                outer_size: Val::Px(bottom_px),
                inner_size: Val::Px(image_size.y),
            },
        ];

        let left_px = slice
            .center_region
            .left
            .resolve(image_size.x, Vec2::ZERO)
            .unwrap_or(0.0);
        let right_px = slice
            .center_region
            .right
            .resolve(image_size.x, Vec2::ZERO)
            .unwrap_or(0.0);
        let middle_width_px = (image_size.x - left_px - right_px).max(0.01);

        let col_data = [
            ItemData {
                grow: 0.0,
                start: Val::Px(0.0),
                end: Val::Auto,
                outer_size: Val::Px(left_px),
                inner_size: Val::Px(image_size.x),
            },
            ItemData {
                grow: 1.0,
                start: Val::Percent(-100.0 * left_px / middle_width_px),
                end: Val::Auto,
                outer_size: Val::Auto,
                inner_size: Val::Percent(100.0 * image_size.x / middle_width_px),
            },
            ItemData {
                grow: 0.0,
                start: Val::Auto,
                end: Val::Px(0.0),
                outer_size: Val::Px(right_px),
                inner_size: Val::Px(image_size.x),
            },
        ];

        // get or build tree
        let Some(container) = maybe_children.and_then(|children| {
            children
                .iter()
                .find(|child| existing_slices.get(**child).is_ok())
        }) else {
            // build
            commands
                .entity(ent)
                .try_insert(SliceInitMarker)
                .with_children(|c| {
                    // container
                    c.spawn((
                        NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                flex_grow: 1.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        SliceInitMarker,
                    ))
                    .with_children(|c| {
                        // row
                        for row in &row_data {
                            c.spawn(NodeBundle {
                                style: Style {
                                    flex_direction: FlexDirection::Row,
                                    flex_grow: row.grow,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|r| {
                                // column
                                for col in &col_data {
                                    r.spawn(NodeBundle {
                                        style: Style {
                                            width: col.outer_size,
                                            height: row.outer_size,
                                            flex_grow: col.grow,
                                            overflow: Overflow::clip(),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .with_children(|i| {
                                        // image
                                        i.spawn(ImageBundle {
                                            style: Style {
                                                width: col.inner_size,
                                                height: row.inner_size,
                                                left: col.start,
                                                top: row.start,
                                                right: col.end,
                                                bottom: row.end,
                                                position_type: PositionType::Absolute,
                                                ..Default::default()
                                            },
                                            image: UiImage {
                                                texture: slice.image.clone(),
                                                ..Default::default()
                                            },
                                            background_color: slice
                                                .tint
                                                .unwrap_or(Color::WHITE.into()),
                                            ..Default::default()
                                        });
                                    });
                                }
                            });
                        }
                    });
                });

            continue;
        };

        // update existing sizes and images
        let Ok(rows) = children_query
            .get_component::<Children>(*container)
            .map(|children| children.iter().copied().collect::<Vec<_>>())
        else {
            panic!("do not taunt happy fun 9slice");
        };
        assert_eq!(rows.len(), 3);
        for (row, row_ent) in row_data.iter().zip(rows.into_iter()) {
            let Ok(cols) = children_query
                .get_component::<Children>(row_ent)
                .map(|children| children.iter().copied().collect::<Vec<_>>())
            else {
                panic!("do not taunt happy fun 9slice");
            };
            assert_eq!(cols.len(), 3);

            for (col, col_ent) in col_data.iter().zip(cols.into_iter()) {
                let Ok((mut outer_style, Some(children))) = style_query.get_mut(col_ent) else {
                    panic!("do not taunt happy fun 9slice");
                };
                assert_eq!(children.len(), 1);

                outer_style.width = col.outer_size;
                outer_style.height = row.outer_size;

                let image_ent = children[0];
                if let Some(mut commands) = commands.get_entity(image_ent) {
                    commands.insert((
                        UiImage {
                            texture: slice.image.clone(),
                            ..Default::default()
                        },
                        slice.tint.unwrap_or(Color::WHITE.into()),
                    ));
                }
                let Ok(mut inner_style) = style_query.get_component_mut::<Style>(image_ent) else {
                    panic!("do not taunt happy fun 9slice");
                };
                inner_style.width = col.inner_size;
                inner_style.height = row.inner_size;
                inner_style.left = col.start;
                inner_style.right = col.end;
                inner_style.top = row.start;
                inner_style.bottom = row.end;
            }
        }
    }
}

pub struct Ui9SliceTemplate;
impl DuiTemplate for Ui9SliceTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<NodeMap, anyhow::Error> {
        let border = props
            .take::<String>("slice-border")?
            .ok_or(anyhow!("no slice-border specified"))?;

        let image = match (
            props.borrow::<String>("slice-image", ctx),
            props.borrow::<Handle<Image>>("slice-image", ctx),
        ) {
            (Ok(Some(img)), _) => ctx.asset_server().load(img),
            (_, Ok(Some(handle))) => handle.clone(),
            _ => bail!("no slice-image specified"),
        };

        let tint = props.take::<String>("slice-color")?;

        let border_sheet = if let Some(tint) = tint.as_ref() {
            format!("#whatever {{ border: {border}; color: {tint}; }}")
        } else {
            format!("#whatever {{ border: {border}; }}")
        };

        let sheet = StyleSheetAsset::parse("", &border_sheet);
        let properties = &sheet.iter().next().unwrap().properties;

        let center_region = properties
            .get("border")
            .unwrap()
            .rect()
            .ok_or(anyhow!("failed to parse slice-border value `{border}`"))?;
        let tint: Option<BackgroundColor> = if let Some(color) = properties.get("color") {
            Some(
                color
                    .color()
                    .ok_or(anyhow!(
                        "failed to parse slice-color value `{}`",
                        tint.unwrap()
                    ))?
                    .into(),
            )
        } else {
            None
        };

        debug!("border rect: {center_region:?}");

        commands.insert((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            Ui9Slice {
                image,
                center_region,
                tint,
            },
        ));

        Ok(NodeMap::default())
    }
}

pub fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("nineslice", Ui9SliceTemplate);
}
