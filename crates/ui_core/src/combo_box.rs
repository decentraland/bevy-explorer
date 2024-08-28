use anyhow::anyhow;
use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    render::camera::RenderTarget,
    window::{PrimaryWindow, WindowRef},
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry, DuiTemplate};
use common::{
    sets::SceneSets,
    util::{DespawnWith, ModifyComponentExt, TryPushChildrenEx},
};

use crate::{
    dui_utils::PropsExt,
    focus::{Focus, Focusable},
    text_size::FontSize,
    ui_actions::{close_ui, Click, DataChanged, On},
};

#[derive(Component, Debug, Clone)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
    pub style: Option<TextStyle>,
}

impl ComboBox {
    pub fn new(
        empty_text: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        allow_null: bool,
        disabled: bool,
        initial_selection: Option<isize>,
        style: Option<TextStyle>,
    ) -> Self {
        Self {
            empty_text,
            options: options.into_iter().map(Into::into).collect(),
            selected: initial_selection.unwrap_or(-1),
            allow_null,
            disabled,
            style,
        }
    }

    pub fn selected(&self) -> Option<&String> {
        if self.selected == -1 {
            None
        } else {
            self.options.get(self.selected as usize)
        }
    }
}
pub struct ComboBoxPlugin;

impl Plugin for ComboBoxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_comboboxen.in_set(SceneSets::PostLoop));
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("combo-box", DuiComboBoxTemplate);
}

#[derive(SystemParam)]
pub struct TargetCameraProperties<'w, 's> {
    target_camera: Query<'w, 's, &'static TargetCamera>,
    cameras: Query<'w, 's, &'static Camera>,
    all_windows: Query<'w, 's, &'static Window>,
    primary_window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images: Res<'w, Assets<Image>>,
}

pub struct TargetCameraProps {
    pub target_camera: Option<TargetCamera>,
    pub size: UVec2,
    pub scale_factor: f32,
}

impl<'w, 's> TargetCameraProperties<'w, 's> {
    fn get_props(&self, e: Entity) -> Option<TargetCameraProps> {
        let target_camera = self.target_camera.get(e).ok().cloned();
        let (window_ref, texture_ref) = match &target_camera {
            Some(target) => {
                let camera = self.cameras.get(target.0).ok()?;

                match &camera.target {
                    RenderTarget::Window(window_ref) => (Some(*window_ref), None),
                    RenderTarget::Image(h_image) => (None, Some(h_image)),
                    _ => return None,
                }
            }
            None => (Some(WindowRef::Primary), None),
        };

        let window = window_ref.and_then(|window_ref| match window_ref {
            WindowRef::Entity(w) => self.all_windows.get(w).ok(),
            WindowRef::Primary => self.primary_window.get_single().ok(),
        });

        let scale_factor = window.map(Window::scale_factor).unwrap_or(1.0);
        let size = if let Some(h_image) = texture_ref {
            self.images.get(h_image)?.size()
        } else {
            window?.size().as_uvec2()
        };

        Some(TargetCameraProps {
            target_camera,
            size,
            scale_factor,
        })
    }
}

#[derive(Component)]
struct ComboMarker;

fn update_comboboxen(
    mut commands: Commands,
    new_boxes: Query<(Entity, &ComboBox, Option<&Children>), Changed<ComboBox>>,
    children: Query<&Children>,
    marked: Query<&ComboMarker>,
    mut removed: RemovedComponents<ComboBox>,
    dui: Res<DuiRegistry>,
) {
    for ent in removed.read() {
        if let Ok(children) = children.get(ent) {
            for child in children {
                if marked.get(*child).is_ok() {
                    commands.entity(*child).despawn_recursive();
                }
            }
        }
    }

    for (ent, cbox, maybe_children) in &new_boxes {
        debug!("{cbox:?}");

        if let Some(children) = maybe_children {
            for child in children {
                if marked.get(*child).is_ok() {
                    commands.entity(*child).despawn_recursive();
                }
            }
        }

        let selected = if cbox.allow_null {
            cbox.selected
        } else {
            cbox.selected.max(0)
        };
        let selection = if selected < 0 {
            cbox.empty_text.as_str()
        } else {
            cbox.options[selected as usize].as_str()
        };

        let components = commands
            .spawn_template(
                &dui,
                "combo-root",
                DuiProps::default().with_prop("selection", selection.to_owned()),
            )
            .unwrap();
        if let Some(style) = cbox.style.as_ref() {
            let style_copy = style.clone();
            commands
                .entity(components.named("text"))
                .modify_component(move |text: &mut Text| {
                    for section in text.sections.iter_mut() {
                        section.style = style_copy.clone();
                    }
                });
        } else {
            commands
                .entity(components.named("text"))
                .insert(FontSize(0.03 / 1.3));
        }
        commands.entity(components.root).set_parent(ent).insert((
            ComboMarker,
            Interaction::default(),
            On::<Click>::new(
                move |mut commands: Commands,
                      combo: Query<(&ComboBox, &Node, &GlobalTransform)>,
                      target_camera: TargetCameraProperties,
                      dui: Res<DuiRegistry>| {
                    let Ok((cbox, node, gt)) = combo.get(ent) else {
                        warn!("no node");
                        return;
                    };

                    let Some(props) = target_camera.get_props(ent) else {
                        warn!("no props");
                        return;
                    };

                    let ui_size = props.size.as_vec2();
                    let v_space_required = node.size().y * cbox.options.len() as f32;
                    let node_bottom = node.size().y * 0.5 + gt.translation().y;
                    let node_top = gt.translation().y - node.size().y * 0.5;
                    let v_space_below = ui_size.y - node_bottom;
                    let v_space_above = node_top;
                    let (top, height) = if v_space_below >= v_space_required {
                        (node_bottom, v_space_required)
                    } else if v_space_above >= v_space_required {
                        (node_top - v_space_required, v_space_required)
                    } else if v_space_below > v_space_above {
                        (node_bottom, v_space_below)
                    } else {
                        (0.0, v_space_above)
                    };

                    // dbg!(ui_size);
                    // dbg!(v_space_required);
                    // dbg!(node_bottom);
                    // dbg!(node_top);
                    // dbg!(v_space_below);
                    // dbg!(v_space_above);
                    // dbg!(top);
                    // dbg!(height);
                    // dbg!(gt.translation());
                    // dbg!(node.size());

                    let popup = commands
                        .spawn_template(
                            &dui,
                            "combo-popup",
                            DuiProps::new()
                                .with_prop("top", format!("{top}px"))
                                .with_prop(
                                    "left",
                                    format!("{}px", gt.translation().x - node.size().x * 0.5),
                                )
                                .with_prop("width", format!("{}px", node.size().x))
                                .with_prop("height", format!("{height}px")),
                        )
                        .unwrap();

                    let contents = cbox
                        .options
                        .iter()
                        .enumerate()
                        .map(|(ix, option)| {
                            commands
                                .spawn((
                                    NodeBundle {
                                        style: Style {
                                            width: Val::Percent(100.0),
                                            min_width: Val::Percent(100.0),
                                            flex_grow: 1.0,
                                            flex_shrink: 0.0,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    },
                                    Interaction::default(),
                                    On::<Click>::new(move |mut commands: Commands| {
                                        debug!("selected {ix:?}");
                                        let Some(mut commands) = commands.get_entity(ent) else {
                                            warn!("no combo");
                                            return;
                                        };

                                        commands
                                            .modify_component(move |combo: &mut ComboBox| {
                                                combo.selected = ix as isize;
                                            })
                                            .insert(DataChanged);
                                    }),
                                ))
                                .with_children(|c| {
                                    let mut cmds = c.spawn((TextBundle {
                                        text: Text::from_section(
                                            option,
                                            cbox.style.clone().unwrap_or_default(),
                                        ),
                                        style: Style {
                                            width: Val::Percent(100.0),
                                            min_width: Val::Percent(100.0),
                                            flex_grow: 1.0,
                                            flex_shrink: 0.0,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    },));

                                    if cbox.style.is_none() {
                                        cmds.insert(FontSize(0.03 / 1.3));
                                    }
                                })
                                .id()
                        })
                        .collect::<Vec<_>>();

                    commands
                        .entity(popup.named("contents"))
                        .try_push_children(contents.as_slice());

                    let blocker = commands
                        .spawn((
                            NodeBundle {
                                style: Style {
                                    position_type: PositionType::Absolute,
                                    left: Val::Px(0.0),
                                    right: Val::Px(0.0),
                                    top: Val::Px(0.0),
                                    bottom: Val::Px(0.0),
                                    ..Default::default()
                                },
                                focus_policy: bevy::ui::FocusPolicy::Block,
                                z_index: ZIndex::Global(99),
                                ..Default::default()
                            },
                            Focusable,
                            Interaction::default(),
                            DespawnWith(popup.root),
                            On::<Focus>::new(
                                (move |mut commands: Commands| {
                                    commands.entity(popup.root).despawn_recursive();
                                })
                                .pipe(close_ui),
                            ),
                        ))
                        .id();

                    if let Some(target_camera) = props.target_camera {
                        commands.entity(popup.root).insert(target_camera.clone());
                        commands.entity(blocker).insert(target_camera);
                    }
                },
            ),
        ));
    }
}
pub struct DuiComboBoxTemplate;

impl DuiTemplate for DuiComboBoxTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let combobox = ComboBox {
            empty_text: props.take::<String>("empty-text")?.unwrap_or_default(),
            options: props
                .take::<Vec<String>>("options")?
                .ok_or(anyhow!("no options for combobox"))?,
            selected: props.take_as::<isize>(ctx, "selected")?.unwrap_or(-1),
            allow_null: props.take_as::<bool>(ctx, "allow-null")?.unwrap_or(false),
            disabled: props.take_as::<bool>(ctx, "disabled")?.unwrap_or(false),
            style: None,
        };
        commands.insert(combobox);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
