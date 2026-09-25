use bevy::{prelude::*, render::camera::Exposure};
#[cfg(test)]
use common::structs::AppConfig;
use common::structs::{SceneGlobalLight, TimeOfDay};

#[path = "unity_sun_colors.rs"]
mod colors;

const CURVE: &[u8] = include_bytes!("../tests/fixtures/sun-cycle.bin");
const STRIDE: usize = 20;

fn read(index: usize) -> (f32, Vec3, f32) {
    let offset = index * STRIDE;
    let value = |axis: usize| {
        let start = offset + axis * 4;
        f32::from_le_bytes(CURVE[start..start + 4].try_into().unwrap())
    };
    (value(0), Vec3::new(value(1), value(2), value(3)), value(4))
}

fn sample(hours: f32) -> (Vec3, f32, Color) {
    let hours = hours.rem_euclid(24.);
    let mut low = 0;
    let mut high = CURVE.len() / STRIDE - 1;
    while high - low > 1 {
        let middle = (low + high) / 2;
        if read(middle).0 <= hours {
            low = middle;
        } else {
            high = middle;
        }
    }
    let (start, a, intensity_a) = read(low);
    let (end, b, intensity_b) = read(high);
    let fraction = ((hours - start) / (end - start)).clamp(0., 1.);
    let direction = a.lerp(b, fraction).normalize() * Vec3::new(1., 1., -1.);
    let day = hours / 24.;
    let keys = colors::COLORS;
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
    (
        direction,
        intensity_a + fraction * (intensity_b - intensity_a),
        Color::srgb(srgb.x, srgb.y, srgb.z),
    )
}

pub(super) fn world_light(
    light: &SceneGlobalLight,
    time: &TimeOfDay,
    primary_exposure: Option<Exposure>,
) -> SceneGlobalLight {
    let mut next = light.clone();
    let Some(exposure) = primary_exposure else {
        return next;
    };
    if light.source.is_some() || !time.time.is_finite() {
        return next;
    }
    let (direction, intensity, color) = sample(time.time / 3600.);
    next.dir_direction = direction;
    next.dir_color = color;
    next.dir_illuminance = intensity * std::f32::consts::PI / exposure.exposure();
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_matches_independent_runtime_midpoints_throughout_the_day() {
        let bytes = include_bytes!("../tests/fixtures/sun-holdouts.bin");
        assert_eq!(bytes.len() % 32, 0);
        assert!(bytes.len() / 32 > 192);
        let mut max_angle = 0.0_f64;
        for row in bytes.chunks_exact(32) {
            let value = |i: usize| f32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap());
            let (direction, intensity, color) = sample(value(0));
            let expected = Vec3::new(value(1), value(2), -value(3))
                .as_dvec3()
                .normalize();
            let actual = direction.as_dvec3().normalize();
            let angle = actual
                .cross(expected)
                .length()
                .atan2(actual.dot(expected))
                .to_degrees();
            max_angle = max_angle.max(angle);
            assert!(angle < 0.0012, "{}h: {angle} degrees", value(0));
            assert!(
                (intensity - value(4)).abs() < 0.000012,
                "{}h intensity",
                value(0)
            );
            let expected_color = Vec3::new(value(5), value(6), value(7));
            assert!(
                (color.to_srgba().to_vec3() - expected_color)
                    .abs()
                    .max_element()
                    < 0.000002
            );
        }
        eprintln!("Maximum held-out sun direction error: {max_angle} degrees");
    }

    #[test]
    fn world_sun_preserves_baseline_scene_overrides_and_exposure_independence() {
        let light = SceneGlobalLight::default();
        let time = TimeOfDay { time: 43200. };
        let exposure = Exposure::default();
        let next = world_light(&light, &time, Some(exposure));
        assert!((next.dir_illuminance - 3762.7325).abs() < 0.01);
        assert!(
            (next.dir_direction - Vec3::new(0.10926124, -0.98798984, -0.1092612)).length() < 1e-6
        );
        let unit = Exposure {
            ev100: -1.2_f32.log2(),
        };
        let unit_light = world_light(&light, &time, Some(unit));
        assert!(
            (next.dir_illuminance * exposure.exposure() - unit_light.dir_illuminance).abs() < 1e-6
        );
        assert_eq!(world_light(&light, &time, None), light);
        let overridden = SceneGlobalLight {
            source: Some(Entity::PLACEHOLDER),
            ..light.clone()
        };
        assert_eq!(world_light(&overridden, &time, Some(exposure)), overridden);
        assert_eq!(sample(0.), sample(24.));
        assert_eq!(sample(-12.), sample(12.));
    }

    #[test]
    fn production_light_system_updates_camera_exposure_and_keeps_scene_overrides() {
        use crate::{apply_global_light, BlendedGlobalLight};
        use common::structs::{PrimaryCamera, SceneLoadDistance};
        let baseline = SceneGlobalLight {
            dir_direction: Vec3::NEG_Y,
            dir_color: Color::WHITE,
            dir_illuminance: 7000.,
            ..default()
        };
        let mut clock = Time::<()>::default();
        clock.advance_by(std::time::Duration::from_secs(10));
        let mut app = App::new();
        app.insert_resource(AppConfig::default())
            .insert_resource(clock)
            .insert_resource(TimeOfDay { time: 43200. })
            .insert_resource(baseline.clone())
            .init_resource::<BlendedGlobalLight>()
            .init_resource::<AmbientLight>()
            .insert_resource(SceneLoadDistance {
                load: 100.,
                unload: 10.,
                load_imposter: 1000.,
            })
            .add_systems(Update, apply_global_light);
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), PrimaryCamera::default()))
            .id();
        app.update();
        assert!(
            (app.world()
                .resource::<BlendedGlobalLight>()
                .0
                .dir_illuminance
                - 3762.7325)
                .abs()
                < 0.01
        );
        let unit = Exposure {
            ev100: -1.2_f32.log2(),
        };
        app.world_mut().entity_mut(camera).insert(unit);
        app.update();
        assert!(
            (app.world()
                .resource::<BlendedGlobalLight>()
                .0
                .dir_illuminance
                - 1.2 * std::f32::consts::PI)
                .abs()
                < 1e-5
        );
        app.world_mut().resource_mut::<SceneGlobalLight>().source = Some(camera);
        app.update();
        assert_eq!(
            app.world().resource::<BlendedGlobalLight>().0.source,
            Some(camera)
        );
    }
}
