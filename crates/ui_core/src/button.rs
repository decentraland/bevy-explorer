use anyhow::anyhow;
use bevy::{ecs::system::EntityCommands, prelude::*, ui::FocusPolicy};
use bevy_dui::{DuiContext, DuiProps, DuiTemplate, NodeMap};
use common::util::{ModifyComponentExt, TryPushChildrenEx};

use crate::{
    bound_node::NodeBounds,
    dui_utils::PropsExt,
    interact_style::{Active, InteractStyles},
    ui_actions::{
        close_ui_happy, close_ui_sad, close_ui_silent, Click, ClickRepeat, DataChanged, Enabled,
        On, UiCaller,
    },
};

pub struct DuiButton {
    pub label: Option<String>,
    pub onclick: Option<On<Click>>,
    pub onclickrepeat: Option<On<ClickRepeat>>,
    pub enabled: bool,
    pub styles: Option<InteractStyles>,
    pub children: Option<Entity>,
    pub image: Option<Handle<Image>>,
    pub image_width: Option<Val>,
    pub image_height: Option<Val>,
}

impl Default for DuiButton {
    fn default() -> Self {
        Self {
            label: Default::default(),
            onclick: Default::default(),
            onclickrepeat: Default::default(),
            enabled: true,
            styles: Default::default(),
            children: Default::default(),
            image: None,
            image_width: None,
            image_height: None,
        }
    }
}

impl DuiButton {
    pub fn new_disabled(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            enabled: false,
            ..Default::default()
        }
    }

    pub fn new_enabled<M, S: IntoSystem<(), (), M>>(label: impl Into<String>, onclick: S) -> Self {
        Self::new(label, true, onclick)
    }

    pub fn new_enabled_and_close_happy<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        onclick: S,
    ) -> Self {
        Self::new(label, true, onclick.pipe(close_ui_happy))
    }

    pub fn new_enabled_and_close_sad<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        onclick: S,
    ) -> Self {
        Self::new(label, true, onclick.pipe(close_ui_sad))
    }

    pub fn new_enabled_and_close_silent<M, S: IntoSystem<(), (), M>>(
        label: impl Into<String>,
        onclick: S,
    ) -> Self {
        Self::new(label, true, onclick.pipe(close_ui_silent))
    }

    pub fn close_silent(label: impl Into<String>) -> Self {
        Self::new(label, true, close_ui_silent)
    }

    pub fn close_happy(label: impl Into<String>) -> Self {
        Self::new(label, true, close_ui_happy)
    }

    pub fn close_sad(label: impl Into<String>) -> Self {
        Self::new(label, true, close_ui_sad)
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
            ..Default::default()
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
        if let Some(onclickrepeat) = props.take::<On<ClickRepeat>>("onclickrepeat")? {
            data.onclickrepeat = Some(onclickrepeat);
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
        if let Some(image) = props.take_as::<Handle<Image>>(ctx, "img")? {
            data.image = Some(image);

            data.image_width = props.take_as::<Val>(ctx, "image-width")?;
            data.image_height = props.take_as::<Val>(ctx, "image-height")?;
        };

        let mut components = match (data.label, data.image) {
            (Some(label), _) => ctx.render_template(
                commands,
                "button-base-text",
                DuiProps::new().with_prop("label", format!("<b>{label}</b>")),
            ),
            (None, Some(img)) => {
                let mut props = DuiProps::new().with_prop("img", img);
                if let Some(image_width) = data.image_width {
                    props = props.with_prop("width", image_width.style_string());
                }
                if let Some(image_height) = data.image_height {
                    props = props.with_prop("height", image_height.style_string());
                }
                ctx.render_template(commands, "button-base-image", props)
            }
            (None, None) => ctx.render_template(commands, "button-base-notext", DuiProps::new()),
        }?;

        let mut new_commands = commands.commands();
        let mut button = new_commands.entity(components["button-background"]);

        button.insert((
            Enabled(data.enabled),
            Interaction::default(),
            FocusPolicy::Block,
        ));

        if let Some(styles) = data.styles {
            button.insert(styles);
        }

        if let Some(onclick) = data.onclick {
            debug!("add on click");
            button.insert(onclick);
        }

        if let Some(onclickrepeat) = data.onclickrepeat {
            debug!("add on click repeat");
            button.insert(onclickrepeat);
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
            if let Some(label) = components.get("label") {
                commands
                    .commands()
                    .entity(*label)
                    .modify_component(|text: &mut Text| {
                        for section in text.sections.iter_mut() {
                            section.style.color = Color::srgb(0.5, 0.5, 0.5);
                        }
                    });
            }
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
        let toggle = props.take_as::<bool>(ctx, "toggle")?.unwrap_or(false);
        let edge_scale = props.take_as::<UiRect>(ctx, "edge-scale")?;

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
                                    cmd.try_insert(DataChanged);
                                }
                            },
                        ),
                    ),
                )
                .map(|nodes| {
                    let mut new_commands = commands.commands();
                    let mut bg = new_commands.entity(nodes["button-background"]);

                    bg.insert(Active(Some(ix) == start_index));

                    if let Some(flat_side) = edge_scale {
                        bg.modify_component(move |bounds: &mut NodeBounds| {
                            bounds.edge_scale = flat_side
                        });
                    }

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

pub trait StyleStringEx {
    fn style_string(&self) -> String;
}

impl StyleStringEx for Val {
    fn style_string(&self) -> String {
        match self {
            Val::Auto => "auto".to_owned(),
            Val::Px(px) => format!("{px}px"),
            Val::Percent(pc) => format!("{pc}%"),
            Val::Vw(vw) => format!("{vw}vw"),
            Val::Vh(vh) => format!("{vh}vh"),
            Val::VMin(vmin) => format!("{vmin}vmin"),
            Val::VMax(vmax) => format!("{vmax}vmax"),
        }
    }
}
