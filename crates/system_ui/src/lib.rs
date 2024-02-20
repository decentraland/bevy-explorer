pub mod change_realm;
pub mod chat;
pub mod discover;
pub mod emote_select;
pub mod emotes;
pub mod login;
pub mod mic;
pub mod profile;
pub mod sysinfo;
pub mod toasts;
pub mod tooltip;
pub mod wearables;

use bevy::prelude::*;

use change_realm::ChangeRealmPlugin;
use common::{sets::SetupSets, structs::UiRoot};
use emote_select::EmoteUiPlugin;
use input_manager::MouseInteractionComponent;
use login::LoginPlugin;
use mic::MicUiPlugin;
use toasts::ToastsPlugin;
use tooltip::ToolTipPlugin;

use self::{chat::ChatPanelPlugin, profile::ProfileEditPlugin, sysinfo::SysInfoPanelPlugin};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER));
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Init).before(SetupSets::Main),
        );

        app.add_plugins((
            SysInfoPanelPlugin,
            ChatPanelPlugin,
            ProfileEditPlugin,
            ToastsPlugin,
            MicUiPlugin,
            ToolTipPlugin,
            LoginPlugin,
            EmoteUiPlugin,
            ChangeRealmPlugin,
        ));
    }
}

#[derive(Resource)]
struct SystemUiRoot(Entity);

#[allow(clippy::type_complexity)]
fn setup(mut commands: Commands, mut ui_root: ResMut<SystemUiRoot>) {
    let root = commands
        .spawn((
            NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            Interaction::default(),
            MouseInteractionComponent,
            UiRoot,
        ))
        .id();

    ui_root.0 = root;
}
