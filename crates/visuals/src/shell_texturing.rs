use bevy::{
    asset::{embedded_asset, embedded_path},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

#[derive(Clone, Asset, TypePath, AsBindGroup)]
pub struct ShellTexture {
    #[uniform(0)]
    subdivisions: u32,
    #[uniform(1)]
    layers: u32,
    #[uniform(2)]
    padding: Vec2,
    #[uniform(3)]
    root_color: LinearRgba,
    #[uniform(4)]
    tip_color: LinearRgba,
}

impl Material for ShellTexture {
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("shell_texturing.wgsl").display()
            )
            .into(),
        )
    }

    fn prepass_fragment_shader() -> ShaderRef {
        Self::fragment_shader()
    }
}

pub(crate) struct ShellTexturingPlugin;

impl Plugin for ShellTexturingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ShellTexture>::default());

        embedded_asset!(app, "shell_texturing.wgsl");
    }
}
