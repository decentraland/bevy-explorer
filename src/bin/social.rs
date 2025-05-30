use bevy::prelude::*;
use bevy_simple_text_input::{
    TextInputBundle, TextInputPlaceholder, TextInputPlugin, TextInputSettings, TextInputSubmitEvent,
};
use common::util::{AsH160, TryPushChildrenEx};
use social::{
    DirectChatMessage, FriendshipEventBody, SocialClient, SocialClientHandler, SocialPlugin,
};
use tokio::sync::mpsc::unbounded_channel;
use wallet::{Wallet, WalletPlugin};

#[derive(Resource)]
struct WalletSeed(Option<u32>);

#[derive(Resource)]
struct SocEvents(
    tokio::sync::mpsc::UnboundedReceiver<FriendshipEventBody>,
    tokio::sync::mpsc::UnboundedReceiver<DirectChatMessage>,
);

fn main() {
    let mut args = pico_args::Arguments::from_env();

    let mut app = App::new();

    app.insert_resource(WalletSeed(args.opt_value_from_str("--seed").unwrap()));
    app.add_plugins(DefaultPlugins)
        .add_plugins((WalletPlugin, TextInputPlugin, SocialPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .run();
}

#[derive(Component)]
pub struct OutputMarker;

fn setup(
    mut commands: Commands,
    mut wallet: ResMut<Wallet>,
    seed: Res<WalletSeed>,
    mut client: ResMut<SocialClient>,
) {
    match seed.0 {
        Some(seed) => {
            let mut seed_bytes: [u8; 32] = [43; 32];
            for (to, from) in seed_bytes.iter_mut().zip(seed.to_le_bytes().into_iter()) {
                *to = from;
            }
            wallet.finalize_as_guest_with_seed(seed_bytes)
        }
        None => wallet.finalize_as_guest(),
    }
    println!("{:#x}", wallet.address().unwrap());
    let (sx, rx) = unbounded_channel();
    let (sx_c, rx_c) = unbounded_channel();
    commands.insert_resource(SocEvents(rx, rx_c));
    client.0 = SocialClientHandler::connect(
        wallet.clone(),
        move |ev: &FriendshipEventBody| {
            let _ = sx.send(ev.clone());
        },
        move |ev: DirectChatMessage| {
            let _ = sx_c.send(ev);
        },
    );

    commands.spawn(Camera3dBundle::default());

    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|c| {
            c.spawn((
                NodeBundle {
                    style: Style {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                OutputMarker,
            ))
            .with_children(|c| {
                c.spawn(TextBundle {
                    text: Text::from_section(
                        format!("address: {:#x}", wallet.address().unwrap()),
                        TextStyle::default(),
                    ),
                    ..Default::default()
                });
                c.spawn(TextBundle {
                    text: Text::from_section(
                        "valid commands:\n- clear\nfriend management:\n- request <address> [...message]\n- cancel <address>\n- accept <address>\n- reject <address>\n- delete <address>\ninfo:\n- friends\n- received\n- sent\nconversation:\n- chat <address> <message>\n- history <address> [count]\n".to_owned(),
                        TextStyle::default(),
                    ),
                    ..Default::default()
                });
            });
            c.spawn((
                NodeBundle {
                    style: Style {
                        height: Val::Px(100.0),
                        ..Default::default()
                    },
                    background_color: Color::srgba(0.0, 0.0, 1.0, 0.2).into(),
                    ..Default::default()
                },
                TextInputBundle {
                    settings: TextInputSettings {
                        multiline: true,
                        retain_on_submit: false,
                        mask_character: None,
                    },
                    placeholder: TextInputPlaceholder {
                        value: "Type here ...".into(),
                        text_style: None,
                    },
                    ..Default::default()
                },
            ));
        });
}

fn update(
    mut commands: Commands,
    mut c2: Commands,
    mut client: ResMut<SocialClient>,
    q: Query<(Entity, Option<&Children>), With<OutputMarker>>,
    mut input: EventReader<TextInputSubmitEvent>,
    mut evs: ResMut<SocEvents>,
) {
    let client = client.0.as_mut().unwrap();

    let (output, children) = q.single();

    let mut reply = |msg: String| {
        let text = Text::from_section(msg, TextStyle::default());
        let text = c2
            .spawn(TextBundle {
                text,
                ..Default::default()
            })
            .id();
        c2.entity(output).try_push_children(&[text]);
    };

    for ev in input.read() {
        let text = Text::from_section(format!("> {}", ev.value), TextStyle::default());
        let text = commands
            .spawn(TextBundle {
                text,
                background_color: Color::srgba(0.0, 0.0, 1.0, 0.2).into(),
                ..Default::default()
            })
            .id();
        commands.entity(output).try_push_children(&[text]);

        if ev.value.starts_with('/') {
            let mut words = ev.value[1..].trim().split(' ');
            let Some(cmd) = words.next() else {
                continue;
            };
            match cmd {
                "request" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_owned());
                        continue;
                    };
                    let message = ev.value[1..]
                        .trim()
                        .splitn(3, ' ')
                        .nth(2)
                        .map(ToOwned::to_owned);
                    if let Err(e) = client.friend_request(target, message) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "cancel" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    if let Err(e) = client.cancel_request(target) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "accept" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    if let Err(e) = client.accept_request(target) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "reject" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    if let Err(e) = client.reject_request(target) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "delete" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    if let Err(e) = client.delete_friend(target) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "friends" => {
                    reply("friends:".to_owned());
                    for friend in client.friends.iter() {
                        reply(format!(" - {friend:#x?}"));
                    }
                }
                "received" => {
                    reply("received requests:".to_owned());
                    for (address, message) in client.received_requests.iter() {
                        reply(format!(
                            " - {address:#x?}: {}",
                            message
                                .as_ref()
                                .map(String::as_str)
                                .unwrap_or("(no message)")
                        ));
                    }
                }
                "sent" => {
                    reply("sent requests:".to_owned());
                    for address in client.sent_requests.iter() {
                        reply(format!(" - {address:#x?}",));
                    }
                }
                "chat" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    let Some(message) = ev.value[1..]
                        .trim()
                        .splitn(3, ' ')
                        .nth(2)
                        .map(ToOwned::to_owned)
                    else {
                        reply("expected message".to_string());
                        continue;
                    };
                    if let Err(e) = client.chat(target, message) {
                        reply(format!("error: {e}"));
                    } else {
                        reply("ok".to_owned());
                    }
                }
                "history" => {
                    let Some(target) = words.next().and_then(|a| a.as_h160()) else {
                        reply("expected address".to_string());
                        continue;
                    };
                    let count = words.next().and_then(|n| n.parse::<usize>().ok());
                    match client.get_chat_history(target) {
                        Err(e) => {
                            reply(format!("error: {e}"));
                        }
                        Ok(mut rx) => {
                            reply("chat history:".to_owned());
                            let mut read = 0;
                            while let Some(next) = rx.blocking_recv() {
                                reply(format!(
                                    " [with {:#x}] {} : {}",
                                    next.partner,
                                    if next.me_speaking { " me " } else { "them" },
                                    next.message
                                ));
                                read += 1;
                                if count.is_some_and(|c| read == c) {
                                    reply("[truncated]".to_owned());
                                    break;
                                }
                            }
                        }
                    }
                }
                "clear" => {
                    if let Some(children) = children {
                        for child in children.iter().rev().skip(1) {
                            commands.entity(*child).despawn_recursive();
                        }
                    }
                }
                _ => {
                    reply(format!("unrecognised commands `{cmd}`"));
                }
            }
        }
    }

    while let Ok(ev) = evs.0.try_recv() {
        reply(format!(" event -> {ev:?}"));
    }
    while let Ok(ev) = evs.1.try_recv() {
        reply(format!(" chat -> {ev:?}"));
    }
}
