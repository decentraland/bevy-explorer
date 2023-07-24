use std::str::FromStr;

use bevy::{prelude::*, ui::FocusPolicy, utils::HashMap};
use urn::Urn;

use avatar::{WearableCategory, WearableMetas, WearablePointerResult, WearablePointers};
use common::util::TryInsertEx;
use comms::profile::{AvatarColor, CurrentUserProfile};
use ipfs::IpfsLoaderExt;
use ui_core::{
    color_picker::ColorPicker,
    dialog::SpawnDialog,
    interact_style::Active,
    scrollable::{ScrollDirection, Scrollable, SpawnScrollable, StartPosition},
    textentry::TextEntry,
    ui_actions::{Click, DataChanged, On},
    ui_builder::{SpawnButton, SpawnSpacer},
    TITLE_TEXT_STYLE,
};

pub struct ProfileEditPlugin;

impl Plugin for ProfileEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // profile button
    commands.spawn((
        ImageBundle {
            image: asset_server.load("images/profile_button.png").into(),
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                right: Val::Px(10.0),
                ..Default::default()
            },
            focus_policy: bevy::ui::FocusPolicy::Block,
            ..Default::default()
        },
        Interaction::default(),
        On::<Click>::new(toggle_profile_ui),
    ));
}

#[derive(Component, Default, Clone)]
pub struct EditWindow {
    name: String,
    bodyshape: String,
    wearables: HashMap<WearableCategory, String>,
    eyes: Color,
    hair: Color,
    skin: Color,
    modified: bool,
}

#[derive(Component)]
pub struct NameEntry;
#[derive(Component, Default)]
pub struct HairColorEntry;
#[derive(Component, Default)]
pub struct EyeColorEntry;
#[derive(Component, Default)]
pub struct SkinColorEntry;
#[derive(Component, Default)]
pub struct MaleEntry;
#[derive(Component, Default)]
pub struct FemaleEntry;
#[derive(Component)]
pub struct WearableButton(String);
fn toggle_profile_ui(
    mut root_commands: Commands,
    window: Query<(Entity, &EditWindow)>,
    current_profile: Res<CurrentUserProfile>,
    wearable_pointers: Res<WearablePointers>,
    wearable_metas: Res<WearableMetas>,
    asset_server: Res<AssetServer>,
) {
    if let Ok((ent, edit)) = window.get_single() {
        if !edit.modified {
            root_commands.entity(ent).despawn_recursive();
        } else {
            // spawn confirm
            root_commands.spawn_dialog_two(
                "Discard Changes".to_owned(),
                "Are you sure you want to discard your changes?".to_owned(),
                "Discard",
                move |mut commands: Commands| commands.entity(ent).despawn_recursive(),
                "Cancel",
                || {},
            );
        }
    } else {
        let content = &current_profile.0.content;
        let edit_window = EditWindow {
            name: content.name.to_owned(),
            bodyshape: content.avatar.body_shape.clone().unwrap(),
            wearables: content
                .avatar
                .wearables
                .iter()
                .flat_map(|wearable| {
                    Urn::from_str(wearable)
                        .ok()
                        .and_then(|urn| wearable_pointers.0.get(&urn))
                        .and_then(WearablePointerResult::hash)
                        .and_then(|hash| wearable_metas.0.get(hash))
                        .map(|meta| (meta.data.category, wearable.to_owned()))
                })
                .collect(),
            eyes: content.avatar.eyes.unwrap().color.into(),
            hair: content.avatar.hair.unwrap().color.into(),
            skin: content.avatar.skin.unwrap().color.into(),
            modified: false,
        };

        let build_copy = edit_window.clone();
        // collect wearables before closures to capture
        let mut wearables = HashMap::new();
        for body_shape in [
            "urn:decentraland:off-chain:base-avatars:basefemale".to_owned(),
            "urn:decentraland:off-chain:base-avatars:basemale".to_owned(),
        ] {
            let mut cats = HashMap::default();
            for category in WearableCategory::iter() {
                let items = wearable_metas
                    .0
                    .iter()
                    .filter(|(_, meta)| &meta.data.category == category)
                    .flat_map(|(hash, meta)| {
                        if meta.data.representations.iter().any(|rep| {
                            rep.body_shapes
                                .iter()
                                .any(|shape| shape.to_lowercase() == body_shape)
                        }) {
                            asset_server
                                .load_content_file::<Image>(&meta.thumbnail, hash)
                                .ok()
                                .map(|thumb| (thumb, meta.id.clone()))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                cats.insert(*category, items);
            }
            wearables.insert(body_shape, cats);
        }

        let window = root_commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        top: Val::Px(20.0),
                        bottom: Val::Px(20.0),
                        left: Val::Px(20.0),
                        right: Val::Px(20.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    focus_policy: FocusPolicy::Block,
                    z_index: ZIndex::Local(1),
                    background_color: Color::rgb(0.9, 0.9, 1.0).into(),
                    ..Default::default()
                },
                edit_window,
            ))
            .id();

        root_commands.entity(window).with_children(move |commands| {
            // title
            commands.spawn(
                TextBundle::from_section(
                    "Edit Profile", TITLE_TEXT_STYLE.get().unwrap().clone(),
                )
                .with_text_alignment(TextAlignment::Center),
            );

            // content
            commands.spawn_scrollable(
                (
                    NodeBundle {
                        style: Style {
                            width: Val::Percent(100.0),
                            height: Val::Percent(80.0),
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            overflow: Overflow::clip(),
                            ..Default::default()
                        },
                        focus_policy: FocusPolicy::Block,
                        ..Default::default()
                    },
                    Interaction::default(),
                ),
                Scrollable::new()
                    .with_wheel(true)
                    .with_drag(true)
                    .with_direction(ScrollDirection::Vertical(StartPosition::Start)),
                move |commands| {
                    commands.spawn(NodeBundle{
                        style: Style {
                            flex_direction: FlexDirection::Column,
                            ..Default::default()
                        },
                        ..Default::default()
                    }).with_children(|commands| {
                        // name
                        commands
                            .spawn(NodeBundle::default())
                            .with_children(|commands| {
                                commands.spawn(TextBundle::from_section(
                                    "Name: ",
                                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                                ));
                                commands.spawn((
                                    NodeBundle {
                                        style: Style {
                                            width: Val::Px(100.0),
                                            height: Val::Px(20.0),
                                            ..Default::default()
                                        },
                                        background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.2, 0.8)),
                                        ..Default::default()
                                    },
                                    TextEntry{
                                        content: build_copy.name.clone(),
                                        enabled: true,
                                        accept_line: false,
                                        ..Default::default()
                                    },
                                    Interaction::default(),
                                    NameEntry,
                                    On::<DataChanged>::new(
                                        |
                                            mut window: Query<&mut EditWindow>,
                                            name: Query<&TextEntry, With<NameEntry>>,
                                        | {
                                            let mut window = window.single_mut();
                                            let name = name.single();
                                            if !name.content.is_empty() && name.content != window.name {
                                                window.name = name.content.clone();
                                                window.modified = true;
                                            }
                                        }
                                    ),
                                ));
                            });

                        // gender
                        let body_shape = build_copy.bodyshape.to_lowercase();
                        let is_female = &body_shape == "urn:decentraland:off-chain:base-avatars:basefemale";
                        commands.spawn(NodeBundle{
                            style: Style {
                                // justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                ..Default::default()
                            },
                            ..Default::default()
                        }).with_children(|commands| {
                            commands.spawn(TextBundle::from_section("Gender", TITLE_TEXT_STYLE.get().unwrap().clone()));
                            let mut male = commands.spawn_button("Male", |mut male: Query<&mut Active, (With<MaleEntry>, Without<FemaleEntry>)>, mut female: Query<&mut Active, With<FemaleEntry>>, mut window: Query<&mut EditWindow>| {
                                male.single_mut().0 = true;
                                female.single_mut().0 = false;
                                let mut window = window.single_mut();
                                window.bodyshape = "urn:decentraland:off-chain:base-avatars:BaseMale".into();
                                window.modified = true;
                            });
                            male.insert(MaleEntry);
                            if !is_female {
                                male.insert(Active(true));
                            }
                            let mut female = commands.spawn_button("Female", |mut male: Query<&mut Active, (With<MaleEntry>, Without<FemaleEntry>)>, mut female: Query<&mut Active, With<FemaleEntry>>, mut window: Query<&mut EditWindow>| {
                                male.single_mut().0 = false;
                                female.single_mut().0 = true;
                                let mut window = window.single_mut();
                                window.bodyshape = "urn:decentraland:off-chain:base-avatars:BaseFemale".into();
                                window.modified = true;
                            });
                            female.insert(FemaleEntry);
                            if is_female {
                                female.insert(Active(true));
                            }
                        });

                        fn color_setting<T: Component + Default>(commands: &mut ChildBuilder, label: &str, color: Color, setter: impl Fn(&mut EditWindow, Color) + Send + Sync + 'static) {
                            commands.spawn(NodeBundle::default()).with_children(move |commands| {
                                commands.spawn(TextBundle::from_section(label, TITLE_TEXT_STYLE.get().unwrap().clone()));
                                commands.spawn((
                                    NodeBundle{
                                        style: Style {
                                            width: Val::Px(40.0),
                                            height: Val::Px(40.0),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    },
                                    ColorPicker::new_linear(color),
                                    T::default(),
                                    On::<DataChanged>::new(
                                        move |
                                            mut window: Query<&mut EditWindow>,
                                            picker: Query<&ColorPicker, With<T>>,
                                        | {
                                            let mut window = window.single_mut();
                                            let picker = picker.single();
                                            setter(&mut window, picker.get_linear());
                                            window.modified = true;
                                        }
                                    ),
                                    Interaction::default(),
                                ));
                            });
                        }

                        // colors
                        commands.spawn(NodeBundle{
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                ..Default::default()
                            },
                            ..Default::default()
                        }).with_children(|commands| {
                            color_setting::<HairColorEntry>(commands, "Hair color: ", build_copy.hair, |w, c| w.hair = c);
                            color_setting::<EyeColorEntry>(commands, "Eye color: ", build_copy.eyes, |w, c| w.eyes = c);
                            color_setting::<SkinColorEntry>(commands, "Skin color: ", build_copy.skin, |w, c| w.skin = c);
                        });

                        // wearables
                        let cats = wearables.get(body_shape.as_str()).unwrap();
                        for category in WearableCategory::iter() {
                            let Some(data) = cats.get(category) else {
                                continue;
                            };
                            if data.is_empty() {
                                continue;
                            }

                            let data = data.clone();
                            commands.spawn(NodeBundle{
                                style: Style {
                                    flex_wrap: FlexWrap::Wrap,
                                    align_content: AlignContent::Center,
                                    // align_self: AlignSelf::FlexStart,
                                    // max_size: Size::width(Val::Px(20.0)),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }).with_children(|commands| {
                                commands.spawn(TextBundle::from_section(
                                    format!("{}: ", category.slot),
                                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                                ));

                                for (thumb, id) in data.into_iter() {
                                    let color = if build_copy.wearables.values().any(|v| v == &id) {
                                        Color::rgb(1.0,1.0,0.5)
                                    } else {
                                        Color::NONE
                                    };
                                    commands.spawn(NodeBundle{
                                        style: Style {
                                            margin: UiRect::all(Val::Px(2.0)),
                                            border: UiRect::all(Val::Px(2.0)),
                                            ..Default::default()
                                        },
                                        background_color: color.into(),
                                        ..Default::default()
                                    }).with_children(|commands| {
                                        commands.spawn((
                                            ImageBundle {
                                                image: thumb.clone().into(),
                                                style: Style {
                                                    width: Val::Px(100.0),
                                                    height: Val::Px(100.0),
                                                    max_width: Val::Px(100.0),
                                                    max_height: Val::Px(100.0),
                                                    ..Default::default()
                                                },
                                                focus_policy: FocusPolicy::Block,
                                                ..Default::default()
                                            },
                                            Interaction::default(),
                                            WearableButton(id.clone()),
                                            On::<Click>::new(move |mut commands: Commands, mut window: Query<&mut EditWindow>, q: Query<(&WearableButton, &Parent)>| {
                                                let mut window = window.single_mut();
                                                if window.wearables.get(category) == Some(&id) {
                                                    window.wearables.remove(category);
                                                } else {
                                                    window.wearables.insert(*category, id.clone());
                                                }
                                                window.modified = true;
                                                for (w, p) in q.iter() {
                                                    if window.wearables.values().any(|v| v == &w.0) {
                                                        commands.entity(p.get()).try_insert(BackgroundColor(Color::rgb(1.0,1.0,0.5)));
                                                    } else {
                                                        commands.entity(p.get()).try_insert(BackgroundColor(Color::NONE));
                                                    }
                                                }
                                            }),
                                        ));
                                    });
                                }
                            });
                        }
                    });
                });

            // buttons
            commands
                .spawn(NodeBundle {
                    style: Style {
                        justify_content: JustifyContent::FlexEnd,
                        width: Val::Percent(100.0),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(move |commands| {
                    commands.spacer();

                    commands.spawn_button("Apply", move |mut commands: Commands, q: Query<&EditWindow>, mut profile: ResMut<CurrentUserProfile>| {
                        let edit = q.single();
                        if edit.modified {
                            profile.0.content.name = edit.name.clone();
                            profile.0.content.avatar.body_shape = Some(edit.bodyshape.clone());
                            profile.0.content.avatar.hair = Some(AvatarColor{ color: edit.hair.into() });
                            profile.0.content.avatar.eyes = Some(AvatarColor{ color: edit.eyes.into() });
                            profile.0.content.avatar.skin = Some(AvatarColor{ color: edit.skin.into() });
                            profile.0.content.avatar.wearables = edit.wearables.values().cloned().collect();
                            profile.0.version += 1;
                            profile.0.content.version = profile.0.version as i64;
                        }
                        commands.entity(window).despawn_recursive()
                    });
                    commands.spawn_button("Cancel", move |mut commands: Commands, q: Query<(Entity, &EditWindow)>| {
                        let (ent, edit) = q.single();
                        if !edit.modified {
                            commands.entity(ent).despawn_recursive()
                        } else {
                            // spawn confirm
                            commands.spawn_dialog_two(
                                "Discard Changes".to_owned(),
                                "Are you sure you want to discard your changes?".to_owned(),
                                "Discard",
                                move |mut commands: Commands| {
                                    commands.entity(ent).despawn_recursive()
                                },
                                "Cancel",
                                || {},
                            );
                        }
                    });
                });
        });
    }
}
