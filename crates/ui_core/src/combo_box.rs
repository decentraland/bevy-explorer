use anyhow::anyhow;
use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    render::camera::RenderTarget,
    window::{PrimaryWindow, WindowRef},
};
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry, DuiTemplate};
use common::{
    sets::SceneSets,
    structs::{TextStyle, ZOrder},
    util::{DespawnWith, ModifyComponentExt, TryPushChildrenEx},
};

use crate::{
    dui_utils::PropsExt,
    focus::Focus,
    interact_style::{Active, InteractStyle, InteractStyles},
    scrollable::ScrollTargetEvent,
    text_size::FontSize,
    ui_actions::{close_ui_silent, Click, DataChanged, Defocus, On},
};

#[derive(Component, Debug, Clone)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
    pub style: Option<TextStyle>,
    pub global_zindex: GlobalZIndex,
}

impl ComboBox {
    pub fn new_scene(
        empty_text: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        allow_null: bool,
        disabled: bool,
        initial_selection: Option<isize>,
        style: Option<TextStyle>,
        is_system_scene: bool,
    ) -> Self {
        Self {
            empty_text,
            options: options.into_iter().map(Into::into).collect(),
            selected: initial_selection.unwrap_or(-1),
            allow_null,
            disabled,
            style,
            global_zindex: if is_system_scene {
                ZOrder::SystemSceneUiOverlay.default()
            } else {
                ZOrder::SceneUiOverlay.default()
            },
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
pub struct TargetCameraHelper<'w, 's> {
    target_camera: Query<'w, 's, &'static UiTargetCamera>,
    cameras: Query<'w, 's, &'static Camera>,
    all_windows: Query<'w, 's, &'static Window>,
    primary_window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images: Res<'w, Assets<Image>>,
}

pub struct TargetCameraProps {
    pub target_camera: Option<UiTargetCamera>,
    pub size: UVec2,
    pub scale_factor: f32,
}

impl TargetCameraHelper<'_, '_> {
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
            WindowRef::Primary => self.primary_window.single().ok(),
        });

        let scale_factor = window.map(Window::scale_factor).unwrap_or(1.0);
        let size = if let Some(h_image) = texture_ref {
            self.images.get(&h_image.handle)?.size()
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
                    commands.entity(*child).despawn();
                }
            }
        }
    }

    for (ent, cbox, maybe_children) in &new_boxes {
        debug!("{cbox:?}");

        if let Some(children) = maybe_children {
            for child in children {
                if marked.get(*child).is_ok() {
                    commands.entity(*child).despawn();
                }
            }
        }

        let selected = if cbox.allow_null {
            cbox.selected
        } else {
            cbox.selected.max(0)
        };
        let selection = cbox
            .options
            .get(selected as usize)
            .unwrap_or(&cbox.empty_text);

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
                .modify_component(move |text_font: &mut TextFont| {
                    *text_font = style_copy.0.clone();
                })
                .modify_component(move |text_color: &mut TextColor| {
                    *text_color = style_copy.1;
                });
        } else {
            commands
                .entity(components.named("text"))
                .insert(FontSize(0.03 / 1.3));
        }
        let entity = components.root;
        commands.entity(components.root).try_insert((
            ChildOf(ent),
            ComboMarker,
            Interaction::default(),
            On::<Click>::new(move |mut commands: Commands| {
                if let Ok(mut commands) = commands.get_entity(entity) {
                    commands.insert(Focus);
                }
            }),
            On::<Focus>::new(
                move |mut commands: Commands,
                      combo: Query<(&ComboBox, &ComputedNode, &GlobalTransform)>,
                      target_camera: TargetCameraHelper,
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
                    let v_space_required =
                        (node.unrounded_size().y - 2.0) * cbox.options.len() as f32 + 2.0;
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

                    let text_lightness =
                        Lcha::from(cbox.style.as_ref().map(|s| s.1 .0).unwrap_or(Color::WHITE))
                            .lightness;
                    let background = if text_lightness > 0.5 {
                        Color::BLACK.with_alpha(0.85)
                    } else {
                        Color::WHITE.with_alpha(0.85)
                    };

                    let popup = commands
                        .spawn(cbox.global_zindex)
                        .apply_template(
                            &dui,
                            "combo-popup",
                            DuiProps::new()
                                .with_prop("top", format!("{top}px"))
                                .with_prop(
                                    "left",
                                    format!("{}px", gt.translation().x - node.size().x * 0.5),
                                )
                                .with_prop("width", format!("{}px", node.size().x))
                                .with_prop("height", format!("{height}px"))
                                .with_prop("background", background.to_srgba().to_hex()),
                        )
                        .unwrap();

                    let mut target = None;
                    let contents = cbox
                        .options
                        .iter()
                        .enumerate()
                        .map(|(ix, option)| {
                            let mut cmds = commands.spawn((
                                Node::default(),
                                Interaction::default(),
                                Focus,
                                On::<Defocus>::new(close_ui_silent),
                                On::<Click>::new(move |mut commands: Commands| {
                                    debug!("selected {ix:?}");
                                    if let Ok(mut commands) = commands.get_entity(popup.root) {
                                        commands.despawn();
                                    }
                                    let Ok(mut commands) = commands.get_entity(ent) else {
                                        warn!("no combo");
                                        return;
                                    };

                                    commands
                                        .modify_component(move |combo: &mut ComboBox| {
                                            combo.selected = ix as isize;
                                        })
                                        .insert(DataChanged);
                                }),
                                InteractStyles {
                                    active: Some(InteractStyle {
                                        background: Some(Color::srgba(0.0, 0.0, 0.5, 0.867)),
                                        border: Some(Color::srgba(0.2, 0.2, 0.2, 0.867)),
                                        ..Default::default()
                                    }),
                                    hover: Some(InteractStyle {
                                        background: Some(Color::srgba(0.0, 0.0, 1.0, 0.867)),
                                        border: Some(Color::srgba(0.2, 0.2, 0.2, 1.0)),
                                        ..Default::default()
                                    }),
                                    inactive: Some(InteractStyle {
                                        background: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                                        border: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                },
                            ));

                            cmds.with_children(|c| {
                                let mut cmds = c.spawn((
                                    Text::new(option),
                                    cbox.style.clone().unwrap_or_default(),
                                    Node {
                                        width: Val::Percent(100.0),
                                        min_width: Val::Percent(100.0),
                                        flex_grow: 1.0,
                                        flex_shrink: 0.0,
                                        ..Default::default()
                                    },
                                ));

                                if cbox.style.is_none() {
                                    cmds.insert(FontSize(0.03 / 1.3));
                                }
                            });

                            if cbox.selected == ix as isize {
                                cmds.insert(Active(true));
                                target = Some(cmds.id());
                            }

                            cmds.id()
                        })
                        .collect::<Vec<_>>();

                    commands
                        .entity(popup.named("contents"))
                        .try_push_children(contents.as_slice());

                    if let Some(target) = target {
                        commands.send_event(ScrollTargetEvent {
                            scrollable: popup.named("popup-scroll"),
                            position: crate::scrollable::ScrollTarget::Entity(target),
                        });
                    }

                    if let Some(target_camera) = props.target_camera {
                        commands
                            .entity(popup.root)
                            .insert((target_camera.clone(), DespawnWith(ent)));
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
            global_zindex: GlobalZIndex(
                props
                    .take_as::<i32>(ctx, "global-z-index")?
                    .unwrap_or(ZOrder::DefaultComboPopup as i32),
            ),
        };
        commands.insert(combobox);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
