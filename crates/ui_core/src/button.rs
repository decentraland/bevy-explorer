use std::marker::PhantomData;

use anyhow::anyhow;
use bevy::{
    ecs::system::{EntityCommand, EntityCommands},
    prelude::*,
    ui::FocusPolicy,
};
use bevy_dui::{DuiContext, DuiProps, DuiTemplate, NodeMap};

use crate::{
    nine_slice::Ui9Slice,
    ui_actions::{Click, Enabled, HoverEnter, HoverExit, On},
};

pub struct DuiButton {
    pub label: String,
    pub onclick: Option<On<Click>>,
    pub enabled: bool,
}

impl DuiButton {
    pub fn new_disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            onclick: None,
            enabled: false,
        }
    }

    pub fn new_enabled<M, S: IntoSystem<(), (), M>>(label: impl Into<String>, onclick: S) -> Self {
        Self::new(label, true, onclick)
    }

    pub fn new<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        enabled: bool,
        onclick: S,
    ) -> Self {
        Self {
            label: label.into(),
            onclick: Some(On::<Click>::new(onclick)),
            enabled,
        }
    }
}

pub(crate) struct DuiButtonTemplate;
impl DuiTemplate for DuiButtonTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<NodeMap, anyhow::Error> {
        debug!("props: {props:?}");

        let data = if let Some(button) = props.take::<DuiButton>("button-data")? {
            button
        } else {
            DuiButton {
                label: props
                    .take::<String>("label")?
                    .unwrap_or("lorem ipsum".to_owned()),
                onclick: props.take::<On<Click>>("onclick")?,
                enabled: props.take::<bool>("enabled")?.unwrap_or(true),
            }
        };

        let button_props = DuiProps::new().with_prop("label", data.label);
        let components = ctx.render_template(commands, "button-base", button_props)?;

        let mut button = commands.commands().entity(components["button-node"]);

        let background_id = components["button-background"];

        button.insert((
            Enabled(data.enabled),
            Interaction::default(),
            FocusPolicy::Block,
        ));
        if let Some(onclick) = data.onclick {
            debug!("add on click");
            button.insert(onclick);
        }

        if data.enabled {
            button.insert((
                On::<HoverEnter>::new(move |mut q: Query<&mut Ui9Slice>| {
                    if let Ok(mut slice) = q.get_mut(background_id) {
                        slice.tint = Some(Color::rgb(1.0, 1.0, 1.0).into());
                    } else {
                        panic!();
                    }
                }),
                On::<HoverExit>::new(move |mut q: Query<&mut Ui9Slice>| {
                    if let Ok(mut slice) = q.get_mut(background_id) {
                        slice.tint = Some(Color::rgb(0.7, 0.7, 1.0).into());
                    } else {
                        panic!();
                    }
                }),
            ));
            commands
                .commands()
                .entity(background_id)
                .modify_component(|slice: &mut Ui9Slice| {
                    slice.tint = Some(Color::rgb(0.7, 0.7, 1.0).into());
                });
        } else {
            // delayed modification
            commands
                .commands()
                .entity(components["label"])
                .modify_component(|text: &mut Text| {
                    for section in text.sections.iter_mut() {
                        section.style.color = Color::rgb(0.5, 0.5, 0.5);
                    }
                });
        }

        Ok(components)
    }
}

pub(crate) struct DuiButtonSetTemplate;
impl DuiTemplate for DuiButtonSetTemplate {
    fn render(
        &self,
        commands: &mut EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<NodeMap, anyhow::Error> {
        let buttons = props
            .take::<Vec<DuiButton>>("buttons")?
            .ok_or(anyhow!("no buttons in set"))?;
        let mut results = NodeMap::default();
        let mut err = None;

        commands
            .insert(NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    margin: UiRect::horizontal(Val::Px(20.0)),
                    ..Default::default()
                },
                ..Default::default()
            })
            .with_children(|c| {
                c.spawn(NodeBundle {
                    style: Style {
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                });

                for (i, button) in buttons.into_iter().enumerate() {
                    let button_props = DuiProps::new().with_prop("button-data", button);
                    match ctx.spawn_template("button", c, button_props) {
                        Ok(nodes) => {
                            results.insert(format!("button {i}"), nodes["root"]);
                        }
                        Err(e) => err = Some(e),
                    }
                }
            });

        if let Some(err) = err {
            return Err(err);
        }
        Ok(results)
    }
}

pub struct ModifyComponent<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> {
    func: F,
    _p: PhantomData<fn() -> C>,
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> EntityCommand
    for ModifyComponent<C, F>
{
    fn apply(self, id: Entity, world: &mut World) {
        if let Some(mut c) = world.get_mut::<C>(id) {
            (self.func)(&mut *c)
        }
    }
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> ModifyComponent<C, F> {
    fn new(func: F) -> Self {
        Self {
            func,
            _p: PhantomData,
        }
    }
}

pub trait ModifyComponentExt {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self;
}

impl<'w, 's, 'a> ModifyComponentExt for EntityCommands<'w, 's, 'a> {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self {
        self.add(ModifyComponent::new(func))
    }
}
