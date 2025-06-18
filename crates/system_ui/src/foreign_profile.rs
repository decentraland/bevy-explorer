use avatar::{avatar_texture::PhotoBooth, AvatarShape};
use bevy::{prelude::*, render::render_resource::Extent3d};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    profile::SerializedProfile,
    structs::{ActiveDialog, ShowProfileEvent, PROFILE_UI_RENDERLAYER},
    util::FireEventEx,
};
use comms::profile::{ProfileManager, UserProfile};
use ethers_core::types::Address;
use social::{FriendshipEvent, FriendshipState, SocialClient};
use ui_core::button::DuiButton;

pub struct ForeignProfilePlugin;

impl Plugin for ForeignProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ShowProfileEvent>();
        app.add_systems(
            Update,
            (show_foreign_profiles, update_profile_friend_buttons).chain(),
        );
    }
}

#[derive(Component)]
pub struct ProfileDialog(Address);

#[allow(clippy::too_many_arguments)]
fn show_foreign_profiles(
    mut commands: Commands,
    mut evs: EventReader<ShowProfileEvent>,
    mut cache: ProfileManager,
    mut pending_events: Local<Vec<Address>>,
    active_dialog: Res<ActiveDialog>,
    mut photo_booth: PhotoBooth,
    dui: Res<DuiRegistry>,
) {
    pending_events.extend(evs.read().map(|ev| ev.0));

    *pending_events = pending_events
        .drain(..)
        .filter(|&address| {
            let default_profile = UserProfile {
                content: SerializedProfile {
                    name: "Profile could not be loaded ...".to_owned(),
                    ..Default::default()
                },
                ..Default::default()
            };
            let profile = match cache.get_data(address) {
                Ok(None) => return true,
                Err(_) => &default_profile,
                Ok(Some(profile)) => profile,
            };

            let Some(permit) = active_dialog.try_acquire() else {
                return true;
            };

            // display profile
            let instance = photo_booth.spawn_booth(
                PROFILE_UI_RENDERLAYER,
                AvatarShape::from(profile),
                Extent3d::default(),
                false,
            );

            let components = commands
                .spawn_template(
                    &dui,
                    "foreign-profile",
                    DuiProps::new()
                        .with_prop("title", format!("{} profile", profile.content.name))
                        .with_prop("booth-instance", instance)
                        .with_prop("eth-address", profile.content.eth_address.clone())
                        .with_prop(
                            "buttons",
                            vec![
                            DuiButton::new_enabled(
                                "Add Friend",
                                move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                    let Some(client) = client.0.as_mut() else {
                                        warn!("not connected");
                                        return;
                                    };

                                    if let Err(e) = client.friend_request(address, None) {
                                        warn!("error: {e}");
                                    } else {
                                        commands.fire_event(FriendshipEvent(None));
                                    }
                                },
                            ),
                            DuiButton::new_enabled(
                                "Cancel Friend Request",
                                move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                    let Some(client) = client.0.as_mut() else {
                                        warn!("not connected");
                                        return;
                                    };

                                    if let Err(e) = client.cancel_request(address) {
                                        warn!("error: {e}");
                                    } else {
                                        commands.fire_event(FriendshipEvent(None));
                                    }
                                },
                            ),
                            DuiButton::new_enabled(
                                "Reject Friend Request",
                                move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                    let Some(client) = client.0.as_mut() else {
                                        warn!("not connected");
                                        return;
                                    };

                                    if let Err(e) = client.reject_request(address) {
                                        warn!("error: {e}");
                                    } else {
                                        commands.fire_event(FriendshipEvent(None));
                                    }
                                },
                            ),
                            DuiButton::new_enabled(
                                "Accept Friend Request",
                                move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                    let Some(client) = client.0.as_mut() else {
                                        warn!("not connected");
                                        return;
                                    };

                                    if let Err(e) = client.accept_request(address) {
                                        warn!("error: {e}");
                                    } else {
                                        commands.fire_event(FriendshipEvent(None));
                                    }
                                },
                            ),
                            DuiButton::new_enabled(
                                "End Friendship",
                                move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                    let Some(client) = client.0.as_mut() else {
                                        warn!("not connected");
                                        return;
                                    };

                                    if let Err(e) = client.delete_friend(address) {
                                        warn!("error: {e}");
                                    } else {
                                        commands.fire_event(FriendshipEvent(None));
                                    }
                                },
                            ),
                            DuiButton::close_happy("Ok"),
                        ],
                        ),
                )
                .unwrap();

            commands
                .entity(components.root)
                .insert((ProfileDialog(address), permit));
            false
        })
        .collect();
}

fn update_profile_friend_buttons(
    q: Query<(Ref<ProfileDialog>, &DuiEntities)>,
    client: Res<SocialClient>,
    mut events: EventReader<FriendshipEvent>,
    children: Query<&Children>,
    mut style: Query<&mut Node>,
) {
    let Ok((profile, components)) = q.single() else {
        events.clear();
        return;
    };

    if events.is_empty() && !profile.is_added() {
        return;
    }
    events.clear();

    let Some(button_set) = components.get_named("button-set") else {
        warn!("no button-set, only: {:?}", components.named_nodes);
        return;
    };
    let Ok(buttons) = children.get(button_set) else {
        warn!("no children");
        return;
    };

    let state = client.get_state(profile.0);
    for (index, req_state) in [
        // add
        (0, FriendshipState::NotFriends),    //add
        (1, FriendshipState::SentRequest),   // cancel
        (2, FriendshipState::RecdRequested), // reject
        (3, FriendshipState::RecdRequested), // accept
        (4, FriendshipState::Friends),       // delete
    ] {
        let Some(mut style) = buttons.get(index).and_then(|b| style.get_mut(*b).ok()) else {
            warn!("button not found");
            continue;
        };
        style.display = if state == req_state {
            Display::Flex
        } else {
            Display::None
        };
    }
}
