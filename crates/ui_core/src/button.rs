use std::marker::PhantomData;

use anyhow::anyhow;
use bevy::{
    ecs::system::{EntityCommand, EntityCommands},
    prelude::*,
    ui::FocusPolicy,
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiTemplate, NodeMap};

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
        props: &mut bevy_dui::DuiProps,
        dui_registry: &bevy_dui::DuiRegistry,
    ) -> Result<NodeMap, anyhow::Error> {
        println!("props: {props:?}");

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

        let base_props = DuiProps::new().with_prop("label", data.label);
        let components = dui_registry.apply_template(commands, "button-base", base_props)?;

        let mut button = commands.commands().entity(components.named("button-node"));

        let background_id = components.named("button-background");

        button.insert((
            Enabled(data.enabled),
            Interaction::default(),
            FocusPolicy::Block,
        ));
        if let Some(onclick) = data.onclick {
            println!("add on click");
            button.insert(onclick);
        }

        if data.enabled {
            button.insert((
                On::<HoverEnter>::new(move |mut q: Query<&mut Ui9Slice>| {
                    if let Ok(mut slice) = q.get_mut(background_id) {
                        println!("ok set tint");
                        slice.tint = Some(Color::rgb(1.0, 1.0, 1.0).into());
                    } else {
                        panic!();
                    }
                }),
                On::<HoverExit>::new(move |mut q: Query<&mut Ui9Slice>| {
                    if let Ok(mut slice) = q.get_mut(background_id) {
                        println!("ok set low tint");
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
                .entity(components.named("label"))
                .modify_component(|text: &mut Text| {
                    for section in text.sections.iter_mut() {
                        section.style.color = Color::rgb(0.5, 0.5, 0.5);
                    }
                });
        }

        Ok(components.named_nodes)
    }
}

pub(crate) struct DuiButtonSetTemplate;
impl DuiTemplate for DuiButtonSetTemplate {
    fn render(
        &self,
        commands: &mut EntityCommands,
        props: &mut DuiProps,
        dui_registry: &bevy_dui::DuiRegistry,
    ) -> Result<NodeMap, anyhow::Error> {
        let buttons = props
            .take::<Vec<DuiButton>>("buttons")?
            .ok_or(anyhow!("no buttons in set"))?;
        let mut results = NodeMap::default();

        commands.insert(NodeBundle::default()).with_children(|c| {
            c.spawn(NodeBundle {
                style: Style {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });
        });

        let mut children = Vec::default();
        for (i, button) in buttons.into_iter().enumerate() {
            let props = DuiProps::new().with_prop("button-data", button);
            let entities = commands
                .commands()
                .spawn_template(dui_registry, "button", props)?;
            results.insert(format!("button {i}"), entities.root);
            children.push(entities.root);
        }

        commands.push_children(&children);

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
