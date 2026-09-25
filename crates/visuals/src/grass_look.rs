//! One palette for tufts, ground and the grassy coastal apron.
use bevy::prelude::*;
use common::structs::ParcelGrassConfig;

/// Build-time meadow palette. `Green` keeps this repository's existing LIME
/// roots and tips; `Red` is the painted red meadow. Both use the same shader,
/// geometry and generated textures.
pub(super) const GRASS_COLOR: GrassColor = GrassColor::Green;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum GrassColor {
    Green,
    Red,
}

impl GrassColor {
    pub fn palette(self) -> (Srgba, Srgba) {
        match self {
            Self::Green => (ParcelGrassConfig::ROOT_COLOR, ParcelGrassConfig::TIP_COLOR),
            Self::Red => (
                Srgba::new(0.66, 0.28, 0.21, 1.0),
                Srgba::new(0.84, 0.40, 0.28, 1.0),
            ),
        }
    }
}

pub(super) struct GrassLookPlugin;

impl Plugin for GrassLookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrassLook>()
            .add_systems(PreStartup, apply_grass_color)
            .add_systems(
                PreUpdate,
                apply_grass_color.run_if(resource_changed::<ParcelGrassConfig>),
            );
    }
}

#[derive(Clone, Copy, Resource)]
pub(super) struct GrassLook {
    pub root: Srgba,
    pub tip: Srgba,
    // Height, static sweep, patch height variation, wind strength.
    pub form: Vec4,
    // Thin-leaf lighting blend, hue variation, root-to-tip blend, coverage.
    pub shading: Vec4,
    // Leaf count, half width, curvature, height variation.
    pub leaves: Vec4,
}

impl Default for GrassLook {
    fn default() -> Self {
        let (root, tip) = GRASS_COLOR.palette();
        Self {
            root,
            tip,
            form: Vec4::new(0.12, 0.012, 0.38, 0.12),
            shading: Vec4::new(0.28, 0.12, 0.72, 0.08),
            leaves: Vec4::new(8.0, 0.22, 1.20, 0.36),
        }
    }
}

impl GrassLook {
    // Fleck scale (8 / x metres per tile), moss amount, color-patch contrast,
    // micro-relief strength. Fine 3.5 x 9.5 cm flecks, not broad leaf-shaped
    // patches; distant tint variation stays quiet so terrain shape carries depth.
    pub fn ground(&self) -> Vec4 {
        Vec4::new(2.5, 0.40, 0.24, 0.12)
    }
}

// Quality presets reset the palette to the repository default; the build-time
// choice is re-applied before materials and cutouts read it.
fn apply_grass_color(look: Res<GrassLook>, mut grass: ResMut<ParcelGrassConfig>) {
    let (root, tip) = (Color::from(look.root), Color::from(look.tip));
    if grass.root_color != root || grass.tip_color != tip {
        grass.root_color = root;
        grass.tip_color = tip;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_palette_survives_quality_changes_without_rewriting_unchanged_config() {
        let mut app = App::new();
        app.init_resource::<ParcelGrassConfig>()
            .add_plugins(GrassLookPlugin);
        app.update();
        let (root, tip) = GRASS_COLOR.palette();
        let grass = app.world().resource::<ParcelGrassConfig>();
        assert_eq!(grass.root_color, Color::from(root));
        assert_eq!(grass.tip_color, Color::from(tip));
        app.world_mut()
            .resource_mut::<ParcelGrassConfig>()
            .root_color = Color::WHITE;
        app.update();
        assert_eq!(
            app.world().resource::<ParcelGrassConfig>().root_color,
            Color::from(root)
        );
        app.update();
        assert!(!app
            .world()
            .get_resource_ref::<ParcelGrassConfig>()
            .unwrap()
            .is_changed());
    }
}
