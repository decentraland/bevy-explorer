pub mod app_settings;
pub mod change_realm;
pub mod chat;
pub mod crash_report;
pub mod discover;
pub mod emote_select;
pub mod emotes;
pub mod foreign_profile;
pub mod login;
pub mod map;
pub mod mic;
pub mod oow;
pub mod permission_manager;
pub mod permissions;
pub mod profile;
pub mod profile_detail;
pub mod sysinfo;
pub mod toasts;
pub mod tooltip;
pub mod version_check;
pub mod wearables;

use bevy::prelude::*;

use change_realm::ChangeRealmPlugin;
use common::{
    inputs::SystemAction,
    sets::SetupSets,
    structs::{ActiveDialog, UiRoot},
};
use emote_select::EmoteUiPlugin;
use foreign_profile::ForeignProfilePlugin;
use input_manager::{InputManager, InputPriority, MouseInteractionComponent};
use login::LoginPlugin;
use map::MapPlugin;
use mic::MicUiPlugin;
use oow::OowUiPlugin;
use permission_manager::PermissionPlugin;
use profile_detail::ProfileDetailPlugin;
use scene_runner::Toaster;
use toasts::ToastsPlugin;
use tooltip::ToolTipPlugin;

use self::{chat::ChatPanelPlugin, profile::ProfileEditPlugin, sysinfo::SysInfoPanelPlugin};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER))
            .init_resource::<ActiveDialog>();
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Init).before(SetupSets::Main),
        );
        app.add_systems(Update, toggle_system_ui);

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
            MapPlugin,
            ProfileDetailPlugin,
            OowUiPlugin,
            PermissionPlugin,
            ForeignProfilePlugin,
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
                z_index: ZIndex::Global((1 << 18) + 2),
                ..Default::default()
            },
            UiRoot,
        ))
        .id();
    ui_root.0 = root;

    // interaction component
    commands.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..Default::default()
            },
            z_index: ZIndex::Global((1 << 18) + 1),
            ..Default::default()
        },
        Interaction::default(),
        MouseInteractionComponent,
    ));
}

fn toggle_system_ui(
    mut toast: Toaster,
    input_manager: InputManager,
    mut root: Query<&mut Style, With<UiRoot>>,
) {
    if input_manager.just_down(SystemAction::HideUi, InputPriority::None) {
        let Ok(mut root) = root.get_single_mut() else {
            warn!("no root");
            return;
        };

        if root.display == Display::Flex {
            toast.add_toast("hide ui", "System ui hidden (press PageUp to toggle)");
            root.display = Display::None;
        } else {
            root.display = Display::Flex;
        }
    }
}
