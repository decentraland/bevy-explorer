use bevy::{
    asset::LoadedFolder,
    prelude::*,
    render::{
        camera::RenderTarget, render_asset::RenderAssetUsages, view::screenshot::ScreenshotManager,
    },
    platform::collections::{HashMap, HashSet},
    window::{EnabledButtons, WindowLevel, WindowRef, WindowResolution},
};
use common::{
    profile::SerializedProfile,
    rpc::{CompareSnapshot, CompareSnapshotResult, RpcCall, RpcResultSender},
    sets::SceneSets,
    structs::PrimaryUser,
};
use comms::profile::{CurrentUserProfile, UserProfile};
use dcl_component::transform_and_parent::DclTranslation;
use ipfs::IpfsAssetServer;
use wallet::Wallet;

use crate::{
    initialize_scene::{TestingData, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    ContainingScene, OutOfWorld, Toaster,
};

pub struct AutomaticTestingPlugin;

impl Plugin for AutomaticTestingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, automatic_testing.in_set(SceneSets::PostLoop));
    }
}

struct SnapshotResult {
    request: CompareSnapshot,
    image: Image,
    window: Entity,
    camera: Entity,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn automatic_testing(
    mut commands: Commands,
    mut testing_data: ResMut<TestingData>,
    player: Query<(Entity, Option<&OutOfWorld>), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    mut toaster: Toaster,
    mut current_profile: ResMut<CurrentUserProfile>,
    ipfas: IpfsAssetServer,
    scenes: Query<&RendererSceneContext>,
    mut fails: Local<Vec<(String, String, bool)>>,
    mut rpcs: EventReader<RpcCall>,
    mut plans: Local<HashMap<Entity, HashSet<String>>>,
    mut snapshot_in_progress: Local<Option<(CompareSnapshot, Entity)>>,
    (mut local_sender, mut local_receiver, mut screenshots, mut screenshot_in_progress): (
        Local<Option<tokio::sync::mpsc::Sender<SnapshotResult>>>,
        Local<Option<tokio::sync::mpsc::Receiver<SnapshotResult>>>,
        Local<Handle<LoadedFolder>>,
        Local<bool>,
    ),
    (mut wallet, folders, images, mut screenshotter): (
        ResMut<Wallet>,
        Res<Assets<LoadedFolder>>,
        Res<Assets<Image>>,
        ResMut<ScreenshotManager>,
    ),
    ui_roots: Query<(Entity, Option<&mut TargetCamera>), (With<ComputedNode>, Without<Parent>)>,
) {
    // load screenshots before entering any scenes (to ensure we don't have to async wait later)
    if screenshots.is_weak() {
        *screenshots = ipfas.asset_server().load_folder("images/screenshots");
    }

    match ipfas.asset_server().load_state(screenshots.id()) {
        bevy::asset::LoadState::Loaded => (),
        bevy::asset::LoadState::Loading => {
            debug!("waiting for screenshots");
            return;
        }
        bevy::asset::LoadState::NotLoaded | bevy::asset::LoadState::Failed(_) => {
            panic!("failed to load screenshots");
        }
    }

    // init channels
    if local_sender.is_none() {
        let (sx, rx) = tokio::sync::mpsc::channel(10);
        *local_sender = Some(sx);
        *local_receiver = Some(rx);
    }

    // process pending snapshots (code run before spawning new snapshot windows in this function as we need 1 frame lag for new windows)
    if let Some((snapshot, window)) = snapshot_in_progress.take() {
        if let Ok(context) = scenes.get(snapshot.scene) {
            let base_position =
                Vec3::new(context.base.x as f32, 0.0, -context.base.y as f32) * PARCEL_SIZE;

            let mut cam = |window: Entity, transform: Transform| {
                commands
                    .spawn((Camera3dBundle {
                        transform,
                        projection: Projection::Perspective(PerspectiveProjection {
                            fov: std::f32::consts::PI / 2.0,
                            aspect_ratio: 1.0,
                            near: 0.1,
                            far: 1000.0,
                        }),
                        camera: Camera {
                            target: RenderTarget::Window(WindowRef::Entity(window)),
                            clear_color: ClearColorConfig::Custom(Color::NONE),
                            ..default()
                        },
                        ..Default::default()
                    },))
                    .id()
            };

            let snapshot_cam = cam(
                window,
                Transform::from_translation(
                    DclTranslation(snapshot.camera_position).to_bevy_translation() + base_position,
                )
                .looking_at(
                    DclTranslation(snapshot.camera_target).to_bevy_translation() + base_position,
                    Vec3::Y,
                ),
            );

            // set ui to render to the snapshot camera
            for (ent, target) in ui_roots.iter() {
                if target.is_none() {
                    debug!("added {snapshot_cam:?} on {ent:?}");
                    commands.entity(ent).insert(TargetCamera(snapshot_cam));
                }
            }

            let sender = local_sender.as_ref().unwrap().clone();
            let _ = screenshotter.take_screenshot(window, move |image| {
                let _ = sender.blocking_send(SnapshotResult {
                    request: snapshot,
                    image,
                    window,
                    camera: snapshot_cam,
                });
            });
        } else {
            warn!("scene not found for snapshot");
        };
    }

    // process received snapshots
    if let Ok(result) = local_receiver.as_mut().unwrap().try_recv() {
        let mut error = None;
        let name = urlencoding::encode(&result.request.name);
        let screenshots: &LoadedFolder = folders.get(screenshots.id()).unwrap();
        let existing_image = screenshots
            .handles
            .iter()
            .find(|h| {
                let p = h
                    .path()
                    .unwrap()
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy();
                p == format!("{name}.png")
            })
            .and_then(|h| {
                let img = images.get(h.id().typed());
                img
            });

        let similarity = match existing_image {
            Some(saved_image) => {
                let path = format!("assets/images/screenshots/{name}_output.png");
                let _ = result
                    .image
                    .clone()
                    .try_into_dynamic()
                    .unwrap()
                    .save_with_format(&path, image::ImageFormat::Png);
                let image2 = std::fs::read(path).unwrap();
                let image2 =
                    image::load_from_memory_with_format(&image2, image::ImageFormat::Png).unwrap();
                let image2 = Image::from_dynamic(image2, false, RenderAssetUsages::default());
                compute_image_similarity(saved_image.clone(), image2)
            }
            None => {
                let dy_img = result.image.try_into_dynamic().unwrap();
                error = match dy_img.save_with_format(
                    format!("assets/images/screenshots/{name}.png"),
                    image::ImageFormat::Png,
                ) {
                    Ok(_) => Some("image not found (it has been created)".to_owned()),
                    Err(e) => Some(format!(
                        "image not found and failed to create new screenshot: {e}"
                    )),
                };
                0.0
            }
        };

        result.request.response.send(CompareSnapshotResult {
            error,
            found: existing_image.is_some(),
            similarity,
        });

        commands.entity(result.window).despawn_recursive();
        commands.entity(result.camera).despawn_recursive();

        // set ui to render to the snapshot camera
        for (ent, target) in ui_roots.iter() {
            if target == Some(&TargetCamera(result.camera)) {
                debug!("removed {:?} from {ent:?}", result.camera);
                commands.entity(ent).remove::<TargetCamera>();
            } else {
                debug!(
                    "skipping remove from {ent:?}, {:?} != {:?}",
                    result.camera, target
                );
            }
        }

        *screenshot_in_progress = false;
    }

    // process events
    for event in rpcs.read() {
        match event {
            RpcCall::TestPlan { scene, plan } => {
                plans.insert(*scene, HashSet::from_iter(plan.iter().cloned()));
            }
            RpcCall::TestResult {
                scene,
                name,
                success,
                error,
            } => {
                let Some(plan) = plans.get_mut(scene) else {
                    warn!("unregistered plan for scene {:?}", scene);
                    continue;
                };

                if !plan.remove(name) {
                    warn!("test {} not registered for scene {:?}", name, scene);
                };

                info!("test {}: {} [{} remaining]", name, success, plan.len());

                if !success {
                    if let Some(location) = scenes.get(*scene).ok().map(|ctx| ctx.base) {
                        if let Some(scene) = testing_data
                            .test_scenes
                            .as_ref()
                            .unwrap()
                            .0
                            .iter()
                            .find(|ts| ts.location == location)
                        {
                            let expected = scene.allow_failures.contains(name);
                            let location = format!("({},{})", location.x, location.y);
                            fails.push((
                                format!("[{location} : {name}]"),
                                error.clone().unwrap_or_default(),
                                expected,
                            ));
                        } else {
                            warn!("location {location} wasn't part of the required set, ignoring this failure");
                        }
                    } else {
                        warn!("scene entity {scene:?} not found(?), ignoring this failure");
                    }
                }
            }
            RpcCall::TestSnapshot(snapshot) => {
                if *screenshot_in_progress {
                    snapshot.response.send(CompareSnapshotResult {
                        error: Some("snapshot already in progress".to_owned()),
                        found: false,
                        similarity: 0.0,
                    });
                    continue;
                }
                *screenshot_in_progress = true;
                let snapshot_window = commands
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
                    .id();

                *snapshot_in_progress = Some((snapshot.clone(), snapshot_window));
            }
            _ => (),
        }
    }

    if wallet.address().is_none() {
        wallet.finalize_as_guest();
        current_profile.profile = Some(UserProfile {
            version: 0,
            content: SerializedProfile {
                eth_address: format!("{:#x}", wallet.address().unwrap()),
                user_id: Some(format!("{:#x}", wallet.address().unwrap())),
                ..Default::default()
            },
            base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
        });
        current_profile.is_deployed = true;
        toaster.add_toast(
            "testing login",
            "AUTO TESTING: Automatically logged in as guest",
        );
        return;
    }

    let (player_ent, oow) = player.single();

    if oow.is_some() {
        return;
    }

    let Some(next_test_scene) = testing_data.test_scenes.as_ref().unwrap().0.front() else {
        if fails.is_empty() {
            info!("all tests passed!");
            std::process::exit(0);
        } else {
            info!("some tests failed:\n {:#?}", *fails);

            if fails.iter().all(|(_, _, expected)| *expected) {
                info!("all failures were allowed");
                std::process::exit(0);
            } else {
                std::process::exit(1);
            }
        }
    };

    let Some(current_scene) = containing_scene.get(player_ent).into_iter().find(|scene| {
        scenes
            .get(*scene)
            .map(|ctx| ctx.base == next_test_scene.location)
            .unwrap_or(false)
    }) else {
        info!("moving to next scene {:?}", next_test_scene.location);
        let to = next_test_scene.location;
        commands.add(move |w: &mut World| {
            w.send_event(RpcCall::TeleportPlayer {
                scene: None,
                to,
                response: RpcResultSender::default(),
            });
        });
        return;
    };
    let context = scenes.get(current_scene).unwrap();

    let Some(plan) = plans.get(&current_scene) else {
        warn!("waiting for plan for scene @ {:?}", context.base);
        return;
    };

    if plan.is_empty() {
        info!("plan completed for scene @ {:?}", context.base);
        testing_data.test_scenes.as_mut().unwrap().0.pop_front();
    }
}

fn compute_image_similarity(img_a: Image, img_b: Image) -> f64 {
    let width = img_a.width() as usize;
    let height = img_a.height() as usize;
    let pixel_count = width * height;

    let a_data = img_a.data.as_slice();
    let b_data = img_b.data.as_slice();

    let mut data_diff = Vec::with_capacity(a_data.len());
    for index in 0..a_data.len() {
        data_diff.push((a_data[index] as i32 - b_data[index] as i32) as i16);
    }

    let mut data_diff_factor = Vec::with_capacity(pixel_count);
    for pixel_index in 0..pixel_count {
        let index = pixel_index * 3;
        let [r, g, b] = &data_diff[index..index + 3] else {
            panic!("Invalid index");
        };
        let diff_sum_i =
            ((*r as i32) * (*r as i32)) + ((*g as i32) * (*g as i32)) + ((*b as i32) * (*b as i32));
        let diff_factor_i = (diff_sum_i as f64) / (3. * (u8::MAX as f64).powi(2));
        data_diff_factor.push(1.0 - diff_factor_i);
    }

    let score: f64 = (data_diff_factor.iter().sum::<f64>() / (pixel_count as f64)).sqrt();

    score
}
