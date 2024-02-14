use std::{path::PathBuf, sync::Arc};

use anyhow::anyhow;
use bevy::{
    core::FrameCount,
    core_pipeline::clear_color::ClearColorConfig,
    ecs::system::SystemParam,
    prelude::*,
    render::{
        camera::RenderTarget,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        view::{screenshot::ScreenshotManager, RenderLayers},
    },
    window::{EnabledButtons, WindowLevel, WindowRef, WindowResolution},
};
use bevy_dui::{DuiRegistry, DuiTemplate};
use common::{
    sets::SetupSets,
    structs::{AvatarTextureHandle, PrimaryPlayerRes},
};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};
use ui_core::ui_actions::{DragData, Dragged, On};

use crate::{AvatarDynamicState, AvatarSelection, AvatarShape};

pub struct AvatarTexturePlugin;

pub const PRIMARY_AVATAR_RENDERLAYER: RenderLayers = RenderLayers::layer(0).with(1);
pub const PROFILE_UI_RENDERLAYER: RenderLayers = RenderLayers::layer(2);

const SNAPSHOT_FRAMES: u32 = 5;

impl Plugin for AvatarTexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_primary_avatar_camera.in_set(SetupSets::Main));
        app.add_systems(
            Update,
            (
                load_foreign_textures,
                update_booth_image,
                snapshot,
                clean_booths,
            ),
        );
    }
}

#[derive(Component)]
pub struct BoothAvatar;

#[derive(SystemParam)]
pub struct PhotoBooth<'w, 's> {
    pub images: ResMut<'w, Assets<Image>>,
    commands: Commands<'w, 's>,
    selections: Query<'w, 's, &'static mut AvatarSelection, With<BoothAvatar>>,
    frame: Res<'w, FrameCount>,
    live_booths: ResMut<'w, LiveBooths>,
}

#[derive(Component, Clone)]
pub struct BoothInstance {
    pub avatar: Arc<Entity>,
    pub avatar_texture: Handle<Image>,
    pub camera: Entity,
    pub snapshot_target: Option<(Handle<Image>, Handle<Image>)>,
}

#[derive(Resource, Default)]
pub struct LiveBooths(Vec<Arc<Entity>>);

impl<'w, 's> PhotoBooth<'w, 's> {
    pub fn spawn_booth(
        &mut self,
        render_layers: RenderLayers,
        shape: AvatarShape,
        size: Extent3d,
        snapshot: bool,
    ) -> BoothInstance {
        let avatar = self
            .commands
            .spawn((
                SpatialBundle::default(),
                AvatarSelection {
                    scene: None,
                    shape: shape.0,
                    render_layers: Some(render_layers),
                    automatic_delete: false,
                },
                AvatarDynamicState {
                    velocity: Vec3::ZERO,
                    ground_height: 0.0,
                },
                BoothAvatar,
            ))
            .id();

        let snapshot_target = if snapshot {
            self.commands.entity(avatar).try_insert(SnapshotTimer(
                self.frame.0 + SNAPSHOT_FRAMES,
                None,
                None,
            ));
            Some((
                self.images.add(Image::default()),
                self.images.add(Image::default()),
            ))
        } else {
            None
        };

        let (avatar_texture, camera) = add_booth_camera(
            &mut self.commands,
            &mut self.images,
            avatar,
            size,
            render_layers,
        );

        let avatar = Arc::new(avatar);
        self.live_booths.0.push(avatar.clone());

        BoothInstance {
            avatar,
            avatar_texture,
            camera,
            snapshot_target,
        }
    }

    pub fn update_shape(&mut self, instance: &BoothInstance, new_shape: AvatarShape) {
        if let Ok(mut selection) = self.selections.get_mut(*instance.avatar) {
            selection.shape = new_shape.0;
            if instance.snapshot_target.is_some() {
                self.commands
                    .entity(*instance.avatar)
                    .try_insert(SnapshotTimer(self.frame.0 + SNAPSHOT_FRAMES, None, None));
            }
        } else {
            error!("no booth avatar to update?");
        }
    }
}

impl BoothInstance {
    pub fn image_bundle(&self) -> impl Bundle {
        (
            ImageBundle {
                style: Style {
                    width: Val::Percent(30.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                image: self.avatar_texture.clone().into(),
                ..Default::default()
            },
            Interaction::default(),
            BoothImage,
            self.clone(),
            On::<Dragged>::new(Self::drag_system),
        )
    }

    pub fn set_transform_for_distance(transform: &mut Transform, distance: f32) {
        let height = 1.8 - 0.9 * (distance - 0.75) / 3.25;
        transform.translation = (transform.translation * Vec3::new(1.0, 0.0, 1.0)).normalize()
            * distance
            + Vec3::Y * height;
        transform.look_at(Vec3::Y * height, Vec3::Y);
    }

    fn drag_system(
        mut transform: Query<&mut Transform>,
        q: Query<(&BoothInstance, &DragData), With<BoothImage>>,
    ) {
        let Ok((instance, drag)) = q.get_single() else {
            return;
        };
        let drag = drag.delta;
        let Ok(mut transform) = transform.get_mut(instance.camera) else {
            return;
        };

        let offset = transform.translation * Vec3::new(1.0, 0.0, 1.0);
        let new_offset = Quat::from_rotation_y(-drag.x / 50.0) * offset;

        let initial_distance = offset.length();
        let distance = (initial_distance * 1.0 + 0.01 * drag.y).clamp(0.75, 4.0);

        let target_height = 1.8 - 0.9 * (distance - 0.75) / 3.25;

        let expected_start_height = 1.8 - 0.9 * (initial_distance - 0.75) / 3.25;
        let height_error = transform.translation.y - expected_start_height;
        let height = if height_error.abs() > 0.02 {
            if distance > 3.0 {
                target_height
                    + height_error * (1.0 - ((initial_distance - distance).abs() * 2.0).min(1.0))
            } else {
                transform.translation.y
            }
        } else {
            target_height
        };

        transform.translation = new_offset.normalize() * distance + Vec3::Y * height;
        transform.look_at(Vec3::Y * height, Vec3::Y);
    }
}

fn setup_primary_avatar_camera(
    mut commands: Commands,
    player: Res<PrimaryPlayerRes>,
    mut images: ResMut<Assets<Image>>,
    mut dui: ResMut<DuiRegistry>,
) {
    let size = Extent3d {
        width: 512,
        height: 512,
        ..default()
    };

    let (avatar_texture, _) = add_booth_camera(
        &mut commands,
        &mut images,
        player.0,
        size,
        RenderLayers::layer(1),
    );

    commands
        .entity(player.0)
        .insert(AvatarTextureHandle(avatar_texture));

    dui.register_template("photobooth", DuiBooth);
}

fn add_booth_camera(
    commands: &mut Commands<'_, '_>,
    images: &mut Assets<Image>,
    entity: Entity,
    size: Extent3d,
    render_layers: RenderLayers,
) -> (Handle<Image>, Entity) {
    let mut avatar_texture = Image {
        texture_descriptor: TextureDescriptor {
            label: None,
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };
    avatar_texture.resize(size);
    let avatar_texture = images.add(avatar_texture);

    let mut camera = None;
    commands.entity(entity).with_children(|c| {
        camera = Some(
            c.spawn((
                Camera3dBundle {
                    transform: Transform::from_translation(Vec3::Z * -1.0 + Vec3::Y * 1.8)
                        .looking_at(Vec3::Y * 1.8, Vec3::Y),
                    camera: Camera {
                        // render before the "main pass" camera
                        order: -1,
                        target: RenderTarget::Image(avatar_texture.clone()),
                        is_active: true,
                        ..default()
                    },
                    camera_3d: Camera3d {
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                render_layers,
                UiCameraConfig { show_ui: false },
            ))
            .id(),
        );

        c.spawn((
            SpotLightBundle {
                transform: Transform::from_xyz(1.0, 2.0, -1.0)
                    .looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Z),
                spot_light: SpotLight {
                    intensity: 300.0,
                    color: Color::WHITE,
                    shadows_enabled: false,
                    inner_angle: 0.6,
                    outer_angle: 0.8,
                    ..default()
                },
                ..default()
            },
            render_layers,
        ));
    });

    (avatar_texture, camera.unwrap())
}

#[allow(clippy::type_complexity)]
fn load_foreign_textures(
    mut q: Query<
        (&mut AvatarTextureHandle, &UserProfile),
        (With<ForeignPlayer>, Changed<UserProfile>),
    >,
    ipfas: IpfsAssetServer,
) {
    for (mut texture, profile) in q.iter_mut() {
        if let Some(snapshots) = profile.content.avatar.snapshots.as_ref() {
            let url = format!("{}{}", profile.base_url, snapshots.face256);
            let ipfs_path = IpfsPath::new_from_url(&url, "png");
            texture.0 = ipfas.asset_server().load(PathBuf::from(&ipfs_path));
            debug!(
                "loaded remote avatar texture ({}) for {}",
                url, profile.content.eth_address
            );
        }
    }
}

fn update_booth_image(
    q: Query<(&Node, &UiImage), With<BoothImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (node, image) in q.iter() {
        let node_size = node.size();
        let Some(image) = images.get_mut(image.texture.id()) else {
            continue;
        };
        if image.size() != node_size.as_uvec2() {
            image.resize(Extent3d {
                width: (node_size.x as u32).max(1),
                height: (node_size.y as u32).max(1),
                ..Default::default()
            });
        }
    }
}

struct SnapshotResult {
    image: Image,
    window: Entity,
    camera: Entity,
    target: Handle<Image>,
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    mut commands: Commands,
    booths: Query<&BoothInstance>,
    mut avatars: Query<(Entity, &mut SnapshotTimer, &AvatarSelection)>,
    frame: Res<FrameCount>,
    mut screenshotter: ResMut<ScreenshotManager>,
    mut local_sender: Local<Option<tokio::sync::mpsc::Sender<SnapshotResult>>>,
    mut local_receiver: Local<Option<tokio::sync::mpsc::Receiver<SnapshotResult>>>,
    mut images: ResMut<Assets<Image>>,
) {
    if local_sender.is_none() {
        let (sx, rx) = tokio::sync::mpsc::channel(10);
        *local_sender = Some(sx);
        *local_receiver = Some(rx);
    }

    // take any pending shots
    for (ent, mut timer, selection) in avatars.iter_mut() {
        if frame.0 >= timer.0 {
            if timer.1.is_none() {
                // Spawn secondary windows
                let mut window = || {
                    commands
                        .spawn(Window {
                            title: "snapshot window".to_owned(),
                            resolution: WindowResolution::new(256.0, 256.0),
                            resizable: false,
                            enabled_buttons: EnabledButtons {
                                minimize: false,
                                maximize: false,
                                close: false,
                            },
                            decorations: false,
                            focused: false,
                            prevent_default_event_handling: true,
                            ime_enabled: false,
                            visible: false,
                            window_level: WindowLevel::AlwaysOnBottom,
                            ..Default::default()
                        })
                        .id()
                };

                let face_window = window();
                let body_window = window();

                timer.1 = Some(face_window);
                timer.2 = Some(body_window);
                // wait a frame after spawning, else it fails
                continue;
            }

            let mut cam = |window: Entity, transform: Transform| {
                commands
                    .spawn((
                        Camera3dBundle {
                            transform,
                            camera: Camera {
                                target: RenderTarget::Window(WindowRef::Entity(window)),
                                ..default()
                            },

                            camera_3d: Camera3d {
                                clear_color: ClearColorConfig::Custom(Color::NONE),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        selection.render_layers.unwrap_or_default(),
                        UiCameraConfig { show_ui: false },
                    ))
                    .id()
            };

            // second window cameras
            let face_window = timer.1.take().unwrap();
            let face_cam = cam(
                face_window,
                Transform::from_translation(Vec3::new(0.0, 1.8, -1.0))
                    .looking_at(Vec3::Y * 1.8, Vec3::Y),
            );

            let body_window = timer.2.take().unwrap();
            let body_cam = cam(
                body_window,
                Transform::from_translation(Vec3::new(0.0, 0.9, -3.0))
                    .looking_at(Vec3::Y * 0.9, Vec3::Y),
            );

            // find matching instance
            if let Some(instance) = booths.iter().find(|b| *b.avatar == ent) {
                // snap face
                let sender = local_sender.as_ref().unwrap().clone();
                let target = instance.snapshot_target.as_ref().unwrap().0.clone();
                let _ = screenshotter.take_screenshot(face_window, move |image| {
                    let _ = sender.blocking_send(SnapshotResult {
                        image,
                        window: face_window,
                        camera: face_cam,
                        target,
                    });
                });

                // snap body
                let sender = local_sender.as_ref().unwrap().clone();
                let target = instance.snapshot_target.as_ref().unwrap().1.clone();
                let _ = screenshotter.take_screenshot(body_window, move |image| {
                    let _ = sender.blocking_send(SnapshotResult {
                        image,
                        window: body_window,
                        camera: body_cam,
                        target,
                    });
                });
            } else {
                error!("no matching instance for timed snapshot");
            }

            commands.entity(ent).remove::<SnapshotTimer>();
        }
    }

    // process taken shots
    while let Ok(SnapshotResult {
        image,
        window,
        camera,
        target,
    }) = local_receiver.as_mut().unwrap().try_recv()
    {
        commands.entity(window).despawn_recursive();
        commands.entity(camera).despawn_recursive();

        let Some(target) = images.get_mut(target) else {
            error!("target not found");
            continue;
        };

        *target = image;
    }
}

#[derive(Component)]
pub struct BoothImage;

#[derive(Component)]
pub struct SnapshotTimer(u32, Option<Entity>, Option<Entity>);

pub struct DuiBooth;
impl DuiTemplate for DuiBooth {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        _: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let booth = props
            .take::<BoothInstance>("booth-instance")?
            .ok_or(anyhow!("no booth provided"))?;

        commands.insert((
            ImageBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                image: booth.avatar_texture.clone().into(),
                ..Default::default()
            },
            Interaction::default(),
            BoothImage,
            booth,
            On::<Dragged>::new(BoothInstance::drag_system),
        ));

        Ok(default())
    }
}

fn clean_booths(mut commands: Commands, mut live: ResMut<LiveBooths>) {
    let booths = std::mem::take(&mut live.0);
    for booth in booths {
        match Arc::try_unwrap(booth) {
            Ok(ent) => {
                commands.entity(ent).despawn_recursive();
                debug!("cleaning booth");
            }
            Err(arc) => live.0.push(arc),
        }
    }
}
