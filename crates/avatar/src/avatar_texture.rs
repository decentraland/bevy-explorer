use std::sync::Arc;

use anyhow::anyhow;
use bevy::{
    app::Propagate,
    diagnostic::FrameCount,
    ecs::system::SystemParam,
    math::FloatOrd,
    prelude::*,
    render::{
        camera::RenderTarget,
        render_asset::RenderAssetUsages,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        view::{
            screenshot::{Screenshot, ScreenshotCaptured},
            RenderLayers,
        },
    },
};
use bevy_dui::{DuiRegistry, DuiTemplate};
use collectibles::{urn::CollectibleUrn, Emote};
use common::{
    sets::SetupSets,
    structs::{AvatarDynamicState, EmoteCommand},
};
use platform::default_camera_components;
use ui_core::ui_actions::{DragData, Dragged, On};

use crate::{AvatarSelection, AvatarShape};

pub struct AvatarTexturePlugin;

const SNAPSHOT_FRAMES: u32 = 5;

impl Plugin for AvatarTexturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiveBooths>();
        app.add_systems(Startup, setup.in_set(SetupSets::Main));
        app.add_systems(Update, (update_booth_image, snapshot, clean_booths));
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
    pub snapshot_target: (Option<Handle<Image>>, Option<Handle<Image>>),
    pending_target: Option<(Handle<Image>, Handle<Image>)>,
}

#[derive(Resource, Default)]
pub struct LiveBooths(Vec<Arc<Entity>>);

impl PhotoBooth<'_, '_> {
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
                Transform::default(),
                Visibility::default(),
                AvatarSelection {
                    scene: None,
                    shape,
                    automatic_delete: false,
                },
                Propagate(render_layers.clone()),
                AvatarDynamicState::default(),
                BoothAvatar,
            ))
            .id();

        let pending_target = if snapshot {
            self.commands
                .entity(avatar)
                .try_insert(SnapshotTimer(self.frame.0 + SNAPSHOT_FRAMES));
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
            render_layers.clone(),
        );

        let avatar = Arc::new(avatar);
        self.live_booths.0.push(avatar.clone());

        BoothInstance {
            avatar,
            avatar_texture,
            camera,
            snapshot_target: (None, None),
            pending_target,
        }
    }

    pub fn update_shape(&mut self, instance: &BoothInstance, new_shape: AvatarShape) {
        if let Ok(mut selection) = self.selections.get_mut(*instance.avatar) {
            selection.shape = new_shape;
            if instance.pending_target.is_some() {
                self.commands
                    .entity(*instance.avatar)
                    .try_insert(SnapshotTimer(self.frame.0 + SNAPSHOT_FRAMES));
            }
        } else {
            error!("no booth avatar to update?");
        }
    }

    pub fn play_emote(&mut self, instance: &BoothInstance, emote: CollectibleUrn<Emote>) {
        self.commands
            .entity(*instance.avatar)
            .try_insert(EmoteCommand {
                urn: emote.to_string(),
                r#loop: false,
                timestamp: self.frame.0 as i64,
            });
    }
}

impl BoothInstance {
    pub fn image_bundle(&self) -> impl Bundle {
        (
            Node {
                width: Val::Percent(30.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ImageNode::new(self.avatar_texture.clone()),
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
        let Ok((instance, drag)) = q.single() else {
            return;
        };
        let drag = drag.delta_pixels;
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

fn setup(mut dui: ResMut<DuiRegistry>) {
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
            size: Extent3d {
                width: size.width.max(16),
                height: size.height.max(16),
                ..size
            },
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
    avatar_texture.resize(Extent3d {
        width: size.width.max(16),
        height: size.height.max(16),
        ..size
    });
    avatar_texture.data = None;
    let avatar_texture = images.add(avatar_texture);

    let mut camera = None;
    commands.entity(entity).with_children(|c| {
        camera = Some(
            c.spawn((
                Camera3d::default(),
                Transform::from_translation(Vec3::Z * -1.0 + Vec3::Y * 1.8)
                    .looking_at(Vec3::Y * 1.8, Vec3::Y),
                Camera {
                    // render before the "main pass" camera
                    order: -1,
                    target: RenderTarget::Image(avatar_texture.clone().into()),
                    is_active: true,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                render_layers.clone(),
                default_camera_components(),
            ))
            .id(),
        );

        c.spawn((
            Transform::from_xyz(1.0, 2.0, -1.0).looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Z),
            SpotLight {
                intensity: 30000.0,
                color: Color::WHITE,
                shadows_enabled: false,
                inner_angle: 0.6,
                outer_angle: 0.8,
                ..default()
            },
            render_layers.clone(),
        ));
    });

    (avatar_texture, camera.unwrap())
}

fn update_booth_image(
    q: Query<(&ComputedNode, &ImageNode), With<BoothImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (node, h_image) in q.iter() {
        let node_size = node.size();
        let Some(image) = images.get(h_image.image.id()) else {
            continue;
        };
        if image.size() != node_size.as_uvec2() {
            images
                .get_mut(h_image.image.id())
                .unwrap()
                .texture_descriptor
                .size = Extent3d {
                width: (node_size.x as u32).max(16),
                height: (node_size.y as u32).max(16),
                ..Default::default()
            };
        }
    }
}

struct SnapshotResult {
    image: Image,
    camera: Entity,
    target: Handle<Image>,
    source: Entity,
    index: usize,
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    mut commands: Commands,
    mut booths: Query<&mut BoothInstance>,
    avatars: Query<(Entity, &SnapshotTimer, &AvatarSelection, &RenderLayers)>,
    frame: Res<FrameCount>,
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
    for (ent, timer, _selection, render_layers) in avatars.iter() {
        if frame.0 >= timer.0 {
            let mut cam = |transform: Transform| -> (Entity, Handle<Image>) {
                let mut image = Image::new_fill(
                    Extent3d {
                        width: 256,
                        height: 256,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    &[0, 0, 0, 0],
                    TextureFormat::bevy_default(),
                    RenderAssetUsages::all(),
                );
                image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
                let image = images.add(image);

                let cam = commands
                    .spawn((
                        Camera3d::default(),
                        transform,
                        Camera {
                            clear_color: ClearColorConfig::Custom(Color::NONE),
                            target: RenderTarget::Image(bevy::render::camera::ImageRenderTarget {
                                handle: image.clone(),
                                scale_factor: FloatOrd(1.0),
                            }),
                            ..default()
                        },
                        render_layers.clone(),
                    ))
                    .id();
                (cam, image)
            };

            // find matching instance
            if let Some(instance) = booths.iter().find(|b| *b.avatar == ent) {
                let (face_cam, face_image) =
                    cam(Transform::from_translation(Vec3::new(0.0, 1.7, -1.0))
                        .looking_at(Vec3::Y * 1.7, Vec3::Y));

                let (body_cam, body_image) =
                    cam(Transform::from_translation(Vec3::new(0.0, 0.9, -3.0))
                        .looking_at(Vec3::Y * 0.9, Vec3::Y));

                // snap face
                let sender = local_sender.as_ref().unwrap().clone();
                let target = instance.pending_target.as_ref().unwrap().0.clone();
                commands.spawn(Screenshot::image(face_image)).observe(
                    move |mut trigger: Trigger<ScreenshotCaptured>| {
                        let _ = sender.blocking_send(SnapshotResult {
                            image: std::mem::take(&mut trigger.0),
                            camera: face_cam,
                            target: target.clone(),
                            source: ent,
                            index: 0,
                        });
                    },
                );

                // snap body
                let sender = local_sender.as_ref().unwrap().clone();
                let target = instance.pending_target.as_ref().unwrap().1.clone();
                commands.spawn(Screenshot::image(body_image)).observe(
                    move |mut trigger: Trigger<ScreenshotCaptured>| {
                        let _ = sender.blocking_send(SnapshotResult {
                            image: std::mem::take(&mut trigger.0),
                            camera: body_cam,
                            target: target.clone(),
                            source: ent,
                            index: 1,
                        });
                    },
                );
            } else {
                error!("no matching instance for timed snapshot");
            }

            commands.entity(ent).remove::<SnapshotTimer>();
        }
    }

    // process taken shots
    while let Ok(SnapshotResult {
        image,
        camera,
        target,
        source,
        index,
    }) = local_receiver.as_mut().unwrap().try_recv()
    {
        commands.entity(camera).despawn();

        let Some(target_img) = images.get_mut(&target) else {
            error!("target {:?} not found", target);
            continue;
        };

        *target_img = image;
        target_img.asset_usage = RenderAssetUsages::default();

        if let Some(mut instance) = booths.iter_mut().find(|b| *b.avatar == source) {
            if index == 0 {
                instance.snapshot_target.0 = Some(target.clone());
            } else {
                instance.snapshot_target.1 = Some(target.clone());
            }
        }
    }
}

#[derive(Component)]
pub struct BoothImage;

#[derive(Component)]
pub struct SnapshotTimer(u32);

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
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ImageNode::new(booth.avatar_texture.clone()),
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
                commands.entity(ent).despawn();
                debug!("cleaning booth");
            }
            Err(arc) => live.0.push(arc),
        }
    }
}
