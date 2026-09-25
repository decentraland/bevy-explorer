use bevy::{asset::weak_handle, prelude::*, render::storage::ShaderStorageBuffer};
use common::structs::{AppConfig, PrimaryCamera, SceneGlobalLight, TimeOfDay};

pub const UNITY_AMBIENT_BUFFER: Handle<ShaderStorageBuffer> =
    weak_handle!("d9af3025-ed73-4b83-b748-3bd75ec464a3");

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct UnityAmbientLight {
    pub enabled: bool,
    pub constant: Vec3,
    pub linear: Vec3,
    pub quadratic: Vec3,
}

impl UnityAmbientLight {
    pub fn day_cycle(day: f32, brightness: f32) -> Self {
        let mut light = gradients::day_cycle(day);
        light.constant *= brightness;
        light.linear *= brightness;
        light.quadratic *= brightness;
        light
    }

    pub fn trilight(sky: Vec3, equator: Vec3, ground: Vec3) -> Self {
        let l0 = (sky + ground) / 6. + equator * (2. / 3.);
        let l2 = (equator * 2. - sky - ground) * (5. / 192.);
        Self {
            enabled: true,
            constant: l0 + l2 * 2.,
            linear: (sky - ground) * (6_f32.sqrt() / 9.),
            quadratic: l2 * -6.,
        }
    }

    #[cfg(test)]
    pub fn irradiance(&self, normal: Vec3) -> Vec3 {
        (self.constant
            + self.linear * normal.y
            + self.quadratic * (normal.y * normal.y + (1. - normal.length_squared()) * 0.5))
            .max(Vec3::ZERO)
    }

    fn buffer_data(&self) -> [Vec4; 3] {
        [
            self.constant.extend(if self.enabled { 1. } else { 0. }),
            self.linear.extend(0.),
            self.quadratic.extend(0.),
        ]
    }
}

pub(super) fn sync_ambient_buffer(
    ambient: Res<UnityAmbientLight>,
    buffers: Option<ResMut<Assets<ShaderStorageBuffer>>>,
) {
    let Some(mut buffers) = buffers else {
        return;
    };
    if !ambient.is_changed() && buffers.contains(UNITY_AMBIENT_BUFFER.id()) {
        return;
    }
    if let Some(buffer) = buffers.get_mut(&UNITY_AMBIENT_BUFFER) {
        buffer.set_data(ambient.buffer_data());
        return;
    }
    buffers.insert(
        UNITY_AMBIENT_BUFFER.id(),
        ShaderStorageBuffer::from(ambient.buffer_data()),
    );
}

mod gradients {
    use bevy::prelude::*;

    use super::UnityAmbientLight;

    const SKY: &[(f32, Vec3)] = &[
        (0.0, Vec3::new(0.353_631_5, 0.0, 1.0)),
        (0.250_003_8, Vec3::new(1.0, 0.596_765_4, 0.526_415_1)),
        (
            0.500_007_6,
            Vec3::new(0.518_688_74, 0.678_820_8, 0.738_000_04),
        ),
        (0.700_007_6, Vec3::new(1.0, 0.499_815_37, 0.458_490_5)),
        (1.0, Vec3::new(0.352_941_2, 0.0, 1.0)),
    ];

    const EQUATOR: &[(f32, Vec3)] = &[
        (0.0, Vec3::new(0.617_166_2, 0.620_549_6, 0.766_037_7)),
        (
            0.300_007_64,
            Vec3::new(0.901_886_76, 0.524_185_66, 0.474_766_8),
        ),
        (
            0.500_007_6,
            Vec3::new(0.731_771_9, 0.648_929_8, 0.786_999_94),
        ),
        (
            0.700_007_6,
            Vec3::new(0.737_254_9, 0.517_647_1, 0.617_148_7),
        ),
        (1.0, Vec3::new(0.615_686_3, 0.619_607_87, 0.764_705_9)),
    ];

    const GROUND: &[(f32, Vec3)] = &[
        (0.0, Vec3::new(0.424_995_06, 0.184_263_57, 0.203_042_49)),
        (
            0.135_301_75,
            Vec3::new(0.547_169_8, 0.183_250_28, 0.183_250_28),
        ),
        (
            0.300_007_64,
            Vec3::new(0.566_037_8, 0.101_909_02, 0.082_769_67),
        ),
        (
            0.500_007_6,
            Vec3::new(0.584_905_6, 0.101_782_7, 0.091_046_64),
        ),
        (
            0.700_007_6,
            Vec3::new(0.528_301_9, 0.092_203_63, 0.105_418_72),
        ),
        (1.0, Vec3::new(0.56078434, 0.24313724, 0.260_830_13)),
    ];

    fn sample(keys: &[(f32, Vec3)], day: f32) -> Vec3 {
        let srgb = if day <= keys[0].0 {
            keys[0].1
        } else {
            keys.windows(2)
                .find_map(|pair| {
                    let [(start, from), (end, to)] = pair else {
                        unreachable!()
                    };
                    (day <= *end).then(|| from.lerp(*to, (day - start) / (end - start)))
                })
                .unwrap_or(keys.last().unwrap().1)
        };
        let linear = Color::srgb(srgb.x, srgb.y, srgb.z).to_linear();
        Vec3::new(linear.red, linear.green, linear.blue)
    }

    pub(super) fn day_cycle(day: f32) -> UnityAmbientLight {
        let day = day.rem_euclid(1.);
        UnityAmbientLight::trilight(sample(SKY, day), sample(EQUATOR, day), sample(GROUND, day))
    }
}

pub(super) fn update_world_ambient(
    config: Res<AppConfig>,
    time: Res<TimeOfDay>,
    light: Res<SceneGlobalLight>,
    cameras: Query<(), With<PrimaryCamera>>,
    mut ambient: ResMut<UnityAmbientLight>,
) {
    let next = if light.source.is_none() && !cameras.is_empty() {
        UnityAmbientLight::day_cycle(
            time.time / 86400.,
            config.graphics.ambient_brightness.max(0) as f32 / 50.,
        )
    } else {
        UnityAmbientLight::default()
    };
    ambient.set_if_neq(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_ambient_tracks_clock_brightness_and_custom_scene_overrides() {
        let mut app = App::new();
        app.init_resource::<AppConfig>()
            .init_resource::<SceneGlobalLight>()
            .init_resource::<UnityAmbientLight>()
            .insert_resource(TimeOfDay { time: 43200. })
            .add_systems(Update, update_world_ambient);
        let camera = app.world_mut().spawn(PrimaryCamera::default()).id();
        app.update();
        let noon = app.world().resource::<UnityAmbientLight>().clone();
        assert_eq!(noon, UnityAmbientLight::day_cycle(0.5, 1.));
        app.world_mut().resource_mut::<TimeOfDay>().time = 0.;
        app.update();
        assert_ne!(app.world().resource::<UnityAmbientLight>(), &noon);
        app.world_mut()
            .resource_mut::<AppConfig>()
            .graphics
            .ambient_brightness = 0;
        app.update();
        assert_eq!(
            app.world()
                .resource::<UnityAmbientLight>()
                .irradiance(Vec3::Y),
            Vec3::ZERO
        );
        app.world_mut().resource_mut::<SceneGlobalLight>().source = Some(camera);
        app.update();
        assert!(!app.world().resource::<UnityAmbientLight>().enabled);
        app.world_mut().resource_mut::<SceneGlobalLight>().source = None;
        app.world_mut().despawn(camera);
        app.update();
        assert!(!app.world().resource::<UnityAmbientLight>().enabled);
    }
}
