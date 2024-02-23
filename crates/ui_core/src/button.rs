use anyhow::anyhow;
use bevy::{ecs::system::EntityCommands, prelude::*, ui::FocusPolicy};
use bevy_dui::{DuiContext, DuiProps, DuiTemplate, NodeMap};
use common::util::TryPushChildrenEx;

use crate::{
    combo_box::PropsExt,
    interact_style::{Active, InteractStyle, InteractStyles},
    ui_actions::{close_ui, Click, DataChanged, Enabled, On, UiCaller},
    ModifyComponentExt,
};

pub struct DuiButton {
    pub label: Option<String>,
    pub onclick: Option<On<Click>>,
    pub enabled: bool,
    pub styles: Option<InteractStyles>,
    pub children: Option<Entity>,
    pub image: Option<Handle<Image>>,
}

impl Default for DuiButton {
    fn default() -> Self {
        Self {
            label: Default::default(),
            onclick: Default::default(),
            enabled: true,
            styles: Default::default(),
            children: Default::default(),
            image: None,
        }
    }
}

impl DuiButton {
    pub fn new_disabled(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            onclick: None,
            enabled: false,
            styles: None,
            children: None,
            image: None,
        }
    }

    pub fn new_enabled<M, S: IntoSystem<(), (), M>>(label: impl Into<String>, onclick: S) -> Self {
        Self::new(label, true, onclick)
    }

    pub fn new_enabled_and_close<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        onclick: S,
    ) -> Self {
        Self::new(label, true, onclick.pipe(close_ui))
    }

    pub fn close(label: impl Into<String>) -> Self {
        Self::new(
            label,
            true,
            move |mut commands: Commands, parents: Query<&Parent>, c: Res<UiCaller>| {
                let mut ent = c.0;
                while let Ok(p) = parents.get(ent) {
                    ent = **p;
                }
                if let Some(commands) = commands.get_entity(ent) {
                    commands.despawn_recursive();
                }
            },
        )
    }

    pub fn close_dialog(mut commands: Commands, parents: Query<&Parent>, c: Res<UiCaller>) {
        let mut ent = c.0;
        while let Ok(p) = parents.get(ent) {
            ent = **p;
        }
        if let Some(commands) = commands.get_entity(ent) {
            commands.despawn_recursive();
        }
    }

    pub fn new<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        enabled: bool,
        onclick: S,
    ) -> Self {
        Self {
            label: Some(label.into()),
            onclick: Some(On::<Click>::new(onclick)),
            enabled,
            styles: None,
            children: None,
            image: None,
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

        let mut data = props.take::<DuiButton>("button-data")?.unwrap_or_default();

        if let Some(label) = props.take::<String>("label")? {
            data.label = Some(label)
        }
        if let Some(onclick) = props.take::<On<Click>>("onclick")? {
            data.onclick = Some(onclick);
        }
        if let Some(enabled) = props.take::<bool>("enabled")? {
            data.enabled = enabled;
        }
        if let Some(styles) = props.take::<InteractStyles>("styles")? {
            data.styles = Some(styles);
        }
        if let Some(children) = props.take::<Entity>("children")? {
            data.children = Some(children);
        }
        if let Some(image) = props.take::<Handle<Image>>("image")? {
            data.image = Some(image)
        };

        let styles = data.styles.unwrap_or(InteractStyles {
            active: Some(InteractStyle {
                background: Some(Color::WHITE),
                ..Default::default()
            }),
            hover: Some(InteractStyle {
                background: Some(Color::rgb(0.9, 0.9, 1.0)),
                ..Default::default()
            }),
            inactive: Some(InteractStyle {
                background: Some(Color::rgb(0.7, 0.7, 1.0)),
                ..Default::default()
            }),
            disabled: Some(InteractStyle {
                background: Some(Color::rgb(0.4, 0.4, 0.4)),
                ..Default::default()
            }),
        });

        let mut components = match (data.label, data.image) {
            (Some(label), _) => ctx.render_template(
                commands,
                "button-base-text",
                DuiProps::new().with_prop("label", label),
            ),
            (None, Some(img)) => ctx.render_template(
                commands,
                "button-base-image",
                DuiProps::new().with_prop("image", img),
            ),
            (None, None) => ctx.render_template(commands, "button-base-notext", DuiProps::new()),
        }?;

        let mut button = commands.commands().entity(components["button-background"]);

        button.insert((
            Enabled(data.enabled),
            Interaction::default(),
            FocusPolicy::Block,
            styles,
        ));
        if let Some(onclick) = data.onclick {
            debug!("add on click");
            button.insert(onclick);
        }

        if let Some(entity) = data.children {
            commands
                .commands()
                .entity(components["button-node"])
                .try_push_children(&[entity]);
            components.insert("label".to_owned(), entity);
        }

        if !data.enabled {
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

        if let Some(text_label) = props.take::<String>("label-name")? {
            components.insert(text_label, components["label"]);
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

        let children = buttons
            .into_iter()
            .map(|button| {
                ctx.render_template(
                    &mut commands.commands().spawn_empty(),
                    "button",
                    DuiProps::new().with_prop("button-data", button),
                )
                .map(|nodes| nodes["root"])
            })
            .collect::<Result<Vec<_>, _>>()?;

        commands.try_push_children(&children);

        Ok(NodeMap::from_iter(
            children
                .into_iter()
                .enumerate()
                .map(|(i, c)| (format!("button {i}"), c)),
        ))
    }
}

#[derive(Component)]
pub struct TabSelection {
    pub selected: Option<usize>,
    active_entities: Vec<NodeMap>,
}

impl TabSelection {
    pub fn selected_entity(&self) -> Option<&NodeMap> {
        self.selected.and_then(|ix| self.active_entities.get(ix))
    }

    pub fn nth_entity(&self, ix: usize) -> Option<&NodeMap> {
        self.active_entities.get(ix)
    }
}

pub(crate) struct DuiTabGroupTemplate;
impl DuiTemplate for DuiTabGroupTemplate {
    fn render(
        &self,
        commands: &mut EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<NodeMap, anyhow::Error> {
        let id = commands.id();

        let buttons = props
            .take::<Vec<DuiButton>>("tabs")?
            .ok_or(anyhow!("no tabs in set"))?;
        let start_index = props.take::<Option<usize>>("initial")?.unwrap_or_default();
        let on_changed = props
            .take::<On<DataChanged>>("onchanged")?
            .ok_or(anyhow!("no action for tabgroup"))?;
        let toggle = props.take_bool_like("toggle")?.unwrap_or(false);

        let mut active_entities = Vec::default();

        let children = buttons
            .into_iter()
            .enumerate()
            .map(|(ix, button)| {
                ctx.render_template(
                    &mut commands.commands().spawn_empty(),
                    "button",
                    DuiProps::new().with_prop("button-data", button).with_prop(
                        "onclick",
                        On::<Click>::new(
                            move |mut commands: Commands,
                                  mut q: Query<&mut TabSelection>,
                                  mut active: Query<&mut Active>| {
                                if let Ok(mut sel) = q.get_mut(id) {
                                    if toggle && sel.selected == Some(ix) {
                                        sel.selected = None;
                                    } else {
                                        sel.selected = Some(ix);
                                    }
                                    for (i, child) in sel.active_entities.iter().enumerate() {
                                        active.get_mut(child["button-background"]).unwrap().0 =
                                            Some(i) == sel.selected;
                                    }
                                }

                                if let Some(mut cmd) = commands.get_entity(id) {
                                    cmd.insert(DataChanged);
                                }
                            },
                        ),
                    ),
                )
                .map(|nodes| {
                    commands
                        .commands()
                        .entity(nodes["button-background"])
                        .insert(Active(Some(ix) == start_index));
                    active_entities.push(nodes.clone());
                    nodes["root"]
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        commands.try_push_children(&children);
        commands.insert((
            on_changed,
            TabSelection {
                selected: start_index,
                active_entities,
            },
        ));

        Ok(NodeMap::from_iter(
            children
                .into_iter()
                .enumerate()
                .map(|(i, c)| (format!("tab {i}"), c)),
        ))
    }
}
