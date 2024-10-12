use bevy::{core::FrameCount, ecs::system::SystemParam, prelude::*};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{structs::ShowProfileEvent, util::TryPushChildrenEx};
use copypasta::{ClipboardContext, ClipboardProvider};
use ethers_core::types::Address;
use scene_runner::Toaster;
use ui_core::ui_actions::{Click, EventCloneExt, On, UiCaller};
use wallet::Wallet;

use crate::chat::friends::PendingProfileUiImage;

use super::{friends::PrivateChat, ChatBox};

#[derive(Component)]
pub struct ChatContainer(pub Option<Address>);

#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub struct ConversationManager<'w, 's> {
    chatbox: Query<'w, 's, (Entity, Option<&'static Children>), With<ChatBox>>,
    containers: Query<'w, 's, (&'static ChatContainer, &'static DuiEntities)>,
    commands: Commands<'w, 's>,
    dui: Res<'w, DuiRegistry>,
    wallet: Res<'w, Wallet>,
    frame: Res<'w, FrameCount>,
    asset_server: Res<'w, AssetServer>,
    added_this_frame: Local<
        's,
        Option<(
            u32,
            Option<(Option<Address>, Entity)>,
            Option<(Option<Address>, Entity)>,
        )>,
    >,
}

impl<'w, 's> ConversationManager<'w, 's> {
    fn existing_container(&self, address: Option<Address>, historic: bool) -> Option<Entity> {
        if let Some((frame, top, bottom)) = self.added_this_frame.as_ref() {
            if *frame == self.frame.0 {
                if let Some((existing_address, existing_entity)) =
                    if historic { top } else { bottom }
                {
                    if *existing_address == address {
                        return Some(*existing_entity);
                    } else {
                        return None;
                    }
                }
            }
        }

        let (_, children) = self.chatbox.get_single().ok()?;
        let children = children?;

        let potential_container = if historic {
            children.iter().next()
        } else {
            children.iter().last()
        }?;

        let (potential_container, entities) = self.containers.get(*potential_container).ok()?;
        (potential_container.0 == address).then_some(entities.named("content"))
    }

    pub fn clear(&mut self) {
        self.commands
            .entity(self.chatbox.single().0)
            .despawn_descendants();
    }

    pub fn add_history_button(&mut self, chat_ent: Entity) {
        let button = self
            .commands
            .spawn_template(
                &self.dui,
                "button",
                DuiProps::new()
                    .with_prop("label", "load more history".to_string())
                    .with_prop(
                        "onclick",
                        On::<Click>::new(
                            move |mut private_chats: Query<&mut PrivateChat>,
                                  caller: Res<UiCaller>,
                                  parent: Query<&Parent>,
                                  mut commands: Commands| {
                                if let Ok(parent) = parent.get(caller.0) {
                                    commands.entity(parent.get()).despawn_recursive();
                                }
                                if let Ok(mut chat) = private_chats.get_mut(chat_ent) {
                                    chat.wants_history_count = 10;
                                }
                            },
                        ),
                    ),
            )
            .unwrap()
            .root;

        self.commands
            .entity(self.chatbox.single().0)
            .insert_children(0, &[button]);
    }

    pub fn get_container(&mut self, address: Option<Address>, historic: bool) -> Entity {
        if let Some(content) = self.existing_container(address, historic) {
            return content;
        }

        let components = if let Some(address) = address {
            let components = self
                .commands
                .spawn_template(&self.dui, "other-chat-container", DuiProps::new())
                .unwrap();
            if address != Address::zero() {
                self.commands
                    .entity(components.named("image"))
                    .insert(ShowProfileEvent(address).send_value_on::<Click>());
            }
            components
        } else {
            self.commands
                .spawn_template(&self.dui, "me-chat-container", DuiProps::default())
                .unwrap()
        };
        if let Some(address) = address.or_else(|| self.wallet.address()) {
            if address != Address::zero() {
                self.commands
                    .entity(components.named("image"))
                    .insert(PendingProfileUiImage(address));
            } else {
                self.commands
                    .entity(components.named("image"))
                    .insert(UiImage::new(
                        self.asset_server
                            .load("images/backpack/wearable_categories/hat.png"),
                    ));
            }
        }
        self.commands
            .entity(components.root)
            .insert(ChatContainer(address));

        let content = components.named("content");

        let chatbox = self.chatbox.get_single().unwrap().0;

        let added = self.added_this_frame.get_or_insert_with(Default::default);
        if added.0 != self.frame.0 {
            added.0 = self.frame.0;
            added.1 = None;
            added.2 = None;
        }
        if historic {
            self.commands
                .entity(chatbox)
                .insert_children(0, &[components.root]);
            added.1 = Some((address, content));
        } else {
            self.commands
                .entity(chatbox)
                .push_children(&[components.root]);
            added.2 = Some((address, content));
        }

        debug!("{:?} -> new {}", address, content);
        content
    }

    pub fn add_message(&mut self, sender: Option<Address>, message: impl ToString, historic: bool) {
        let me_speaking = sender.is_none() || self.wallet.address() == sender;
        let container = self.get_container((!me_speaking).then(|| sender.unwrap()), historic);
        debug!("container: {container:?}");

        let message_body = message.to_string();
        let message = self
            .commands
            .spawn_template(
                &self.dui,
                if me_speaking {
                    "chat-content-me"
                } else {
                    "chat-content-other"
                },
                DuiProps::new()
                    .with_prop("text", message_body.clone())
                    .with_prop(
                        "copy",
                        On::<Click>::new(move |mut toaster: Toaster, frame: Res<FrameCount>| {
                            let Ok(mut ctx) = ClipboardContext::new() else {
                                warn!("failed to copy");
                                return;
                            };

                            if ctx.set_contents(message_body.clone()).is_ok() {
                                toaster.add_toast(
                                    format!("chatcopy {}", frame.0),
                                    "Message copied to clipboard",
                                );
                            } else {
                                toaster.add_toast(
                                    format!("chatcopy {}", frame.0),
                                    "Failed to copy message",
                                );
                            }
                        }),
                    ),
            )
            .unwrap()
            .root;
        if historic {
            self.commands
                .entity(container)
                .insert_children(0, &[message]);
        } else {
            self.commands
                .entity(container)
                .try_push_children(&[message]);
        }
        debug!("added");
    }
}
