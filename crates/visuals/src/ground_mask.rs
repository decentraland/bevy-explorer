use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use common::{structs::LandscapeTerrain, terrain::TerrainField};
use scene_runner::initialize_scene::{PointerResult, ScenePointers};

const SIDE: u32 = 1024;

#[derive(Resource)]
pub(super) struct GroundMask(pub Handle<Image>);

impl FromWorld for GroundMask {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Image>>().add(Image::new_fill(
            Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0],
            TextureFormat::R8Uint,
            RenderAssetUsages::all(),
        )))
    }
}

fn occupied_pixels(pointers: &ScenePointers) -> Vec<u8> {
    let mut pixels = vec![0; (SIDE * SIDE) as usize];
    for (parcel, pointer) in pointers.iter() {
        if !matches!(pointer, PointerResult::Exists { .. }) {
            continue;
        }
        let pixel = *parcel + IVec2::splat(SIDE as i32 / 2);
        if pixel.cmpge(IVec2::ZERO).all() && pixel.cmplt(IVec2::splat(SIDE as i32)).all() {
            pixels[(pixel.y as u32 * SIDE + pixel.x as u32) as usize] = 1;
        }
    }
    pixels
}

pub(super) fn update(
    pointers: Res<ScenePointers>,
    mut mask: ResMut<GroundMask>,
    terrain: Res<LandscapeTerrain>,
    mut images: ResMut<Assets<Image>>,
    mut applied_generation: Local<Option<u64>>,
) {
    // rendered_generation changes as the camera moves; occupancy does not.
    if terrain.field.is_some() && *applied_generation == Some(terrain.generation) {
        return;
    }
    *applied_generation = Some(terrain.generation);
    let (side, pixels) = terrain
        .field
        .as_ref()
        .map(|field| terrain_pixels(field))
        .unwrap_or_else(|| (SIDE, occupied_pixels(&pointers)));
    if images.get(&mask.0).and_then(|image| image.data.as_ref()) != Some(&pixels) {
        if let Some(image) = images.get_mut(&mask.0) {
            let resized = image.width() != side || image.height() != side;
            *image = Image::new(
                Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::R8Uint,
                RenderAssetUsages::all(),
            );
            if resized {
                mask.set_changed();
            }
        }
    }
}

fn terrain_pixels(field: &TerrainField) -> (u32, Vec<u8>) {
    (
        field.size as u32,
        field
            .pixels
            .iter()
            .map(|value| u8::from(*value == 0))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occupied() -> PointerResult {
        PointerResult::Exists {
            realm: "test".into(),
            hash: "scene".into(),
            urn: None,
        }
    }

    #[test]
    fn holes_follow_scene_pointer_changes_including_negative_parcels() {
        let mut pointers = ScenePointers::default();
        let parcel = IVec2::new(-2, 3);
        let index = ((parcel.y + 512) * 1024 + parcel.x + 512) as usize;
        assert_eq!(occupied_pixels(&pointers)[index], 0);
        pointers.insert(parcel, occupied());
        let pixels = occupied_pixels(&pointers);
        assert_eq!(pixels[index], 1);
        assert_eq!(pixels.iter().filter(|pixel| **pixel != 0).count(), 1);
        pointers.insert(parcel, PointerResult::Nothing);
        assert_eq!(occupied_pixels(&pointers)[index], 0);
    }

    #[test]
    fn parcels_outside_the_global_ground_do_not_wrap_into_its_mask() {
        let mut pointers = ScenePointers::default();
        for parcel in [
            IVec2::new(-513, 0),
            IVec2::new(512, 0),
            IVec2::new(0, -513),
            IVec2::new(0, 512),
        ] {
            pointers.insert(parcel, occupied());
        }
        assert!(occupied_pixels(&pointers).iter().all(|pixel| *pixel == 0));
    }
}
