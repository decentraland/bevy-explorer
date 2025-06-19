use bevy::{diagnostic::FrameCount, ecs::system::SystemParam, prelude::*, tasks::IoTaskPool};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{structs::ShowProfileEvent, util::TryPushChildrenEx};
use copypwasmta::{ClipboardContext, ClipboardProvider};
use ethers_core::types::Address;
use scene_runner::Toaster;
use ui_core::ui_actions::{Click, EventCloneExt, On, UiCaller};
use wallet::Wallet;

use crate::chat::friends::PendingProfileUiImage;

use super::friends::PrivateChat;

#[derive(Component)]
pub struct ChatBubble(pub Option<Address>, pub Color);

#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub struct ConversationManager<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    containers: Query<'w, 's, (&'static ChatBubble, &'static DuiEntities)>,
    commands: Commands<'w, 's>,
    dui: Res<'w, DuiRegistry>,
    wallet: Res<'w, Wallet>,
    frame: Res<'w, FrameCount>,
    asset_server: Res<'w, AssetServer>,
    added_this_frame: Local<
        's,
        Option<(
            u32,
            Option<(Option<Address>, Color, (Entity, Entity))>,
            Option<(Option<Address>, Color, (Entity, Entity))>,
        )>,
    >,
}

impl ConversationManager<'_, '_> {
    fn existing_bubble(
        &self,
        container: Entity,
        address: Option<Address>,
        color: Color,
        historic: bool,
    ) -> Option<(Entity, Entity)> {
        if let Some((frame, top, bottom)) = self.added_this_frame.as_ref() {
            if *frame == self.frame.0 {
                if let Some((existing_address, existing_color, existing_entity)) =
                    if historic { top } else { bottom }
                {
                    if *existing_address == address && *existing_color == color {
                        return Some(*existing_entity);
                    } else {
                        return None;
                    }
                }
            }
        }

        let children = self.children.get(container).ok()?;

        let potential_container = if historic {
            children.iter().next()
        } else {
            children.iter().last()
        }?;

        let (potential_container, entities) = self.containers.get(potential_container).ok()?;
        (potential_container.0 == address && potential_container.1 == color)
            .then_some((entities.root, entities.named("content")))
    }

    pub fn clear(&mut self, container: Entity) {
        self.commands
            .entity(container)
            .despawn_related::<Children>();
    }

    pub fn add_history_button(&mut self, container: Entity, private_chat_ent: Entity) {
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
                                  parent: Query<&ChildOf>,
                                  mut commands: Commands| {
                                if let Ok(parent) = parent.get(caller.0) {
                                    commands.entity(parent.parent()).despawn();
                                }
                                if let Ok(mut chat) = private_chats.get_mut(private_chat_ent) {
                                    chat.wants_history_count = 10;
                                }
                            },
                        ),
                    ),
            )
            .unwrap()
            .root;

        self.commands
            .entity(container)
            .insert_children(0, &[button]);
    }

    pub fn get_bubble(
        &mut self,
        container: Entity,
        address: Option<Address>,
        color: Color,
        historic: bool,
    ) -> (Entity, Entity) {
        if let Some((bubble, content)) = self.existing_bubble(container, address, color, historic) {
            return (bubble, content);
        }

        let components = if let Some(address) = address {
            let components = self
                .commands
                .spawn_template(
                    &self.dui,
                    "chat-container-other",
                    DuiProps::new().with_prop("color", color),
                )
                .unwrap();
            if address != Address::zero() {
                self.commands
                    .entity(components.named("image"))
                    .insert(ShowProfileEvent(address).send_value_on::<Click>());
            }
            components
        } else {
            self.commands
                .spawn_template(
                    &self.dui,
                    "chat-container-me",
                    DuiProps::new().with_prop("color", color),
                )
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
                    .insert(ImageNode::new(
                        self.asset_server
                            .load("images/backpack/wearable_categories/hat.png"),
                    ));
            }
        }

        let bubble = components.root;
        let content = components.named("content");

        self.commands
            .entity(bubble)
            .insert(ChatBubble(address, color));

        let added = self.added_this_frame.get_or_insert_with(Default::default);
        if added.0 != self.frame.0 {
            added.0 = self.frame.0;
            added.1 = None;
            added.2 = None;
        }
        if historic {
            self.commands
                .entity(container)
                .insert_children(0, &[bubble]);
            added.1 = Some((address, color, (bubble, content)));
        } else {
            self.commands.entity(container).try_push_children(&[bubble]);
            added.2 = Some((address, color, (bubble, content)));
        }

        debug!("{:?} -> new {:?}", address, (bubble, content));
        (bubble, content)
    }

    pub fn add_message(
        &mut self,
        container: Entity,
        sender: Option<Address>,
        color: Color,
        message: impl ToString,
        historic: bool,
    ) -> (Entity, Entity) {
        let me_speaking = sender.is_none() || self.wallet.address() == sender;
        let (bubble, content) = self.get_bubble(
            container,
            (!me_speaking).then(|| sender.unwrap()),
            color,
            historic,
        );
        debug!("container: {content:?}");

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

                            let message_body = message_body.clone();
                            IoTaskPool::get()
                                .spawn(async move {
                                    if let Err(e) = ctx.set_contents(message_body.clone()).await {
                                        error!("failed to set clipboard content: {e:?}");
                                    }
                                })
                                .detach();

                            toaster.add_toast(
                                format!("chatcopy {}", frame.0),
                                "Message copied to clipboard",
                            );
                        }),
                    ),
            )
            .unwrap()
            .root;
        if historic {
            self.commands.entity(content).insert_children(0, &[message]);
        } else {
            self.commands.entity(content).try_push_children(&[message]);
        }
        debug!("added");
        (bubble, message)
    }
}
