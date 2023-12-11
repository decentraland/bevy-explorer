use std::path::PathBuf;

use bevy::{
    core_pipeline::clear_color::ClearColorConfig,
    ecs::system::SystemParam,
    prelude::*,
    render::{
        camera::RenderTarget,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        view::RenderLayers,
    },
};
use common::{sets::SetupSets, structs::PrimaryPlayerRes};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};

use crate::{AvatarDynamicState, AvatarSelection, AvatarShape};

pub struct AvatarTexturePlugin;

pub const PRIMARY_AVATAR_RENDERLAYER: RenderLayers = RenderLayers::layer(0).with(1);
pub const PROFILE_UI_RENDERLAYER: RenderLayers = RenderLayers::layer(2);

#[derive(Component)]
pub struct AvatarTexture(pub Handle<Image>);

impl Plugin for AvatarTexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_primary_avatar_camera.in_set(SetupSets::Main));
        app.add_systems(Update, load_foreign_textures);
    }
}

#[derive(Component)]
pub struct BoothAvatar;

#[derive(SystemParam)]
pub struct PhotoBooth<'w, 's> {
    images: ResMut<'w, Assets<Image>>,
    commands: Commands<'w, 's>,
    selections: Query<'w, 's, &'static mut AvatarSelection, With<BoothAvatar>>,
}

#[derive(Component, Clone)]
pub struct BoothInstance {
    pub avatar: Entity,
    pub avatar_texture: Handle<Image>,
    pub camera: Entity,
}

impl<'w, 's> PhotoBooth<'w, 's> {
    pub fn spawn_booth(
        &mut self,
        render_layers: RenderLayers,
        shape: AvatarShape,
        size: Extent3d,
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

        let (avatar_texture, camera) = add_booth_camera(
            &mut self.commands,
            &mut self.images,
            avatar,
            size,
            render_layers,
        );

        self.commands.entity(avatar).with_children(|c| {
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

        BoothInstance {
            avatar,
            avatar_texture,
            camera,
        }
    }

    pub fn update_shape(&mut self, instance: &BoothInstance, new_shape: AvatarShape) {
        if let Ok(mut selection) = self.selections.get_mut(instance.avatar) {
            selection.shape = new_shape.0;
        } else {
            error!("no booth avatar to update?");
        }
    }
}

fn setup_primary_avatar_camera(
    mut commands: Commands,
    player: Res<PrimaryPlayerRes>,
    mut images: ResMut<Assets<Image>>,
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
        PRIMARY_AVATAR_RENDERLAYER,
    );

    commands
        .entity(player.0)
        .insert(AvatarTexture(avatar_texture));
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
    });

    (avatar_texture, camera.unwrap())
}

#[allow(clippy::type_complexity)]
fn load_foreign_textures(
    mut q: Query<(&mut AvatarTexture, &UserProfile), (With<ForeignPlayer>, Changed<UserProfile>)>,
    ipfas: IpfsAssetServer,
) {
    for (mut texture, profile) in q.iter_mut() {
        if let Some(snapshots) = profile.content.avatar.snapshots.as_ref() {
            let url = format!("{}{}", profile.base_url, snapshots.face256);
            let ipfs_path = IpfsPath::new_from_url(&url, "png");
            texture.0 = ipfas.asset_server().load(PathBuf::from(&ipfs_path));
        }
    }
}
