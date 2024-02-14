use bevy::{prelude::*, ui::widget::UiImageSize};
use bevy_dui::*;

#[derive(Component)]
pub struct Spinner;

pub struct SpinnerPlugin;

impl Plugin for SpinnerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, spin_spinners);
    }
}

fn setup(
    mut dui: ResMut<DuiRegistry>,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
) {
    dui.register_template("spinner", DuiSpinnerTemplate);

    let texture = asset_server.load::<Image>("images/spinner_atlas.png");
    let texture_atlas = TextureAtlas::from_grid(texture, Vec2::new(34.0, 34.0), 8, 1, None, None);
    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    dui.set_default_prop("spinner-atlas", texture_atlas_handle);
}

fn spin_spinners(mut q: Query<&mut UiTextureAtlasImage, With<Spinner>>, time: Res<Time>) {
    for mut t in q.iter_mut() {
        t.index = (time.elapsed_seconds() * 8.0) as usize % 8;
    }
}

pub struct DuiSpinnerTemplate;
impl DuiTemplate for DuiSpinnerTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        ctx.render_template(commands, "spinner-base", DuiProps::new())?;
        commands.insert((
            props
                .borrow::<Handle<TextureAtlas>>("spinner-atlas", ctx)?
                .unwrap()
                .clone(),
            BackgroundColor(Color::WHITE),
            UiTextureAtlasImage::default(),
            UiImageSize::default(),
            Spinner,
        ));
        Ok(Default::default())
    }
}
