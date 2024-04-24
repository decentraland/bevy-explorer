use bevy::{input::mouse::MouseWheel, prelude::*, utils::HashMap, window::PrimaryWindow};
use bevy_dui::{DuiContext, DuiProps, DuiRegistry, DuiTemplate};
use common::util::TryPushChildrenEx;

use crate::interact_style::{InteractStyle, InteractStyles};

use super::ui_builder::SpawnSpacer;

pub struct ScrollablePlugin;

impl Plugin for ScrollablePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_scrollables);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("scrollable-base", ScrollableTemplate);
    dui.register_template("vscroll", VerticalScrollTemplate);
    dui.register_template("hscroll", HorizontalScrollTemplate);
    dui.register_template("scroll", TwoWayScrollTemplate);
}

pub trait SpawnScrollable {
    fn spawn_scrollable(
        &mut self,
        bundle: impl Bundle,
        scrollable: Scrollable,
        spawn_children: impl FnOnce(&mut ChildBuilder),
    );
}

impl SpawnScrollable for ChildBuilder<'_> {
    fn spawn_scrollable(
        &mut self,
        bundle: impl Bundle,
        scrollable: Scrollable,
        spawn_children: impl FnOnce(&mut ChildBuilder),
    ) {
        let panel_size = match scrollable.direction {
            ScrollDirection::Vertical(_) => (Val::Auto, Val::Px(100000.0)),
            ScrollDirection::Horizontal(_) => (Val::Px(100000.0), Val::Auto),
            ScrollDirection::Both(_, _) => (Val::Px(100000.0), Val::Px(100000.0)),
        };

        let mut content = Entity::PLACEHOLDER;

        self.spawn((bundle, scrollable))
            .with_children(|commands| {
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            width: panel_size.0,
                            height: panel_size.1,
                            // TODO this should be set based on direction
                            flex_direction: FlexDirection::Column,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        // TODO need one more layer for bidirectional scrolling
                        content = commands
                            .spawn(NodeBundle {
                                style: Style {
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|commands| spawn_children(commands))
                            .id();
                        commands.spacer();
                    });
            })
            .try_insert(ScrollContent(content));
    }
}

#[derive(Component)]
struct TempMarker;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ScrollDirection {
    Vertical(StartPosition),
    Horizontal(StartPosition),
    Both(StartPosition, StartPosition),
}

impl Default for ScrollDirection {
    fn default() -> Self {
        Self::Both(Default::default(), Default::default())
    }
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum StartPosition {
    Start,
    #[default]
    Center,
    End,
}

impl ScrollDirection {
    fn vertical(&self) -> Option<StartPosition> {
        match self {
            ScrollDirection::Vertical(v) | ScrollDirection::Both(_, v) => Some(*v),
            _ => None,
        }
    }
    fn horizontal(&self) -> Option<StartPosition> {
        match self {
            ScrollDirection::Horizontal(h) | ScrollDirection::Both(h, _) => Some(*h),
            _ => None,
        }
    }
}

#[derive(Component, Default)]
pub struct Scrollable {
    pub direction: ScrollDirection,
    pub drag: bool,
    pub wheel: bool,
    content_size: Vec2,
}

impl Scrollable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_drag(self, drag: bool) -> Self {
        Self { drag, ..self }
    }

    pub fn with_direction(self, direction: ScrollDirection) -> Self {
        Self { direction, ..self }
    }

    pub fn with_wheel(self, wheel: bool) -> Self {
        Self { wheel, ..self }
    }
}

#[derive(Component)]
struct ScrollContent(Entity);

#[derive(Component)]
struct ScrollBar {
    parent: Entity,
    vertical: bool,
}

#[derive(Component)]
struct Slider {
    parent: Entity,
    vertical: bool,
    position: f32,
}

pub const BAR_WIDTH: f32 = 15.0;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_scrollables(
    mut commands: Commands,
    window: Query<&Window, With<PrimaryWindow>>,
    mut nodes: Query<
        (&Node, &mut Style, Option<&Children>),
        (Without<Scrollable>, Without<ScrollBar>, Without<Slider>),
    >,
    mut scrollables: Query<(
        Entity,
        &mut Scrollable,
        &ScrollContent,
        &Node,
        &GlobalTransform,
        Ref<GlobalTransform>,
        Ref<Node>,
        &Interaction,
    )>,
    mut bars: Query<
        (
            Entity,
            &ScrollBar,
            &mut Style,
            &Interaction,
            &Node,
            &GlobalTransform,
        ),
        (Without<Scrollable>, Without<Slider>),
    >,
    mut sliders: Query<
        (Entity, &mut Slider, &mut Style),
        (Without<Scrollable>, Without<ScrollBar>),
    >,
    mut clicked_slider: Local<Option<(Entity, Vec2)>>,
    mut wheel: EventReader<MouseWheel>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
) {
    #[derive(Copy, Clone, Debug)]
    enum UpdateSliderPosition {
        Abs(f32),
        Rel(f32),
    }

    struct ScrollInfo {
        content: Entity,
        ratio: f32,
        slide_amount: Vec2,
        bar_position: Vec2,
        length: f32,
        start: StartPosition,
        redraw: bool,
        update_slider: Option<UpdateSliderPosition>,
    }

    let Ok(window) = window.get_single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    if mouse_button_input.just_released(MouseButton::Left) {
        *clicked_slider = None;
    }

    let mut vertical_scrollers = HashMap::default();
    let mut horizontal_scrollers = HashMap::default();

    let wheel_events = wheel.read().collect::<Vec<_>>();

    // gather scrollable components that need scrollbars
    for (
        entity,
        mut scrollable,
        scroll_content,
        node,
        transform,
        ref_transform,
        ref_node,
        interaction,
    ) in scrollables.iter_mut()
    {
        let Ok((child_node, mut style, _)) = nodes.get_mut(scroll_content.0) else {
            warn!("scrollable hierarchy is broken");
            continue;
        };

        let child_size = child_node.size();
        let parent_size = node.size();
        let ratio = parent_size / child_size;
        let ui_position = transform.translation().truncate() - parent_size * 0.5;
        let slide_amount = child_size - parent_size;

        // calculate deltas based on drag or mouse wheel in the parent container
        let mut new_slider_deltas = None;
        if scrollable.drag {
            if let Some((prev_entity, prev_pos)) = clicked_slider.as_ref() {
                if prev_entity == &entity {
                    let delta = cursor_position - *prev_pos;
                    new_slider_deltas = Some(delta / slide_amount);
                }
            }

            if interaction == &Interaction::Pressed {
                *clicked_slider = Some((entity, cursor_position));
            }
        }
        if scrollable.wheel {
            // we check only if the cursor is within our frame - this means scrollables can still be scrolled when
            // blocking dialogs cover them. TODO: make this better, requires either
            // - check all children for interaction (yuck)
            // - add some context to FocusPolicy (e.g. FocusPolicy::Block(HashSet<Buttons>))
            // - add another system to manage "container" focus based on child focus
            if cursor_position.clamp(ui_position, ui_position + parent_size) == cursor_position {
                for ev in wheel_events.iter() {
                    let unit = match ev.unit {
                        bevy::input::mouse::MouseScrollUnit::Line => 20.0,
                        bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
                    };
                    *new_slider_deltas.get_or_insert(Default::default()) +=
                        Vec2::new(ev.x, ev.y) * unit / slide_amount;
                }
            }
        }

        // the reported content-size is rounded, and occasionally repositioning when it changes causes a loop of +/- 1 pixel
        // so we allow 1 pixel tolerance (new content smaller) before redrawing
        let change = scrollable.content_size - child_size;
        let redraw = ref_transform.is_changed()
            || ref_node.is_changed()
            || change.max_element() > 0.0
            || change.min_element() < -1.0;

        if ratio.x < 1.0 {
            // generate info for the required scrollbars
            if let Some(start) = scrollable.direction.horizontal() {
                horizontal_scrollers.insert(
                    entity,
                    ScrollInfo {
                        content: scroll_content.0,
                        slide_amount,
                        ratio: ratio.x,
                        bar_position: ui_position * 0.0
                            + Vec2::new(BAR_WIDTH, parent_size.y - BAR_WIDTH - 5.0),
                        length: parent_size.x - 20.0,
                        start,
                        redraw,
                        update_slider: new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.x)),
                    },
                );
            }
        } else {
            // or if we don't need a scrollbar, make sure the content position is at zero
            style.left = Val::Px(0.0);
        }

        if ratio.y < 1.0 {
            if let Some(start) = scrollable.direction.vertical() {
                vertical_scrollers.insert(
                    entity,
                    ScrollInfo {
                        content: scroll_content.0,
                        slide_amount,
                        ratio: ratio.y,
                        bar_position: ui_position * 0.0
                            + Vec2::new(parent_size.x - BAR_WIDTH - 5.0, BAR_WIDTH),
                        length: parent_size.y - 20.0,
                        start,
                        redraw,
                        update_slider: new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.y)),
                    },
                );
            }
        } else {
            style.top = Val::Px(0.0);
        }

        scrollable.content_size = child_size;
    }

    // bars
    for (entity, bar, mut style, interaction, node, transform) in bars.iter_mut() {
        let source = if bar.vertical {
            &mut vertical_scrollers
        } else {
            &mut horizontal_scrollers
        };

        let Some(info) = source.get_mut(&bar.parent) else {
            commands.entity(entity).despawn_recursive();
            continue;
        };

        if info.redraw {
            // parent either moved, was resized or the content size changed. in any case, reposition/resize the bars
            style.left = Val::Px(info.bar_position.x);
            style.top = Val::Px(info.bar_position.y);
            if bar.vertical {
                style.width = Val::Px(BAR_WIDTH);
                style.height = Val::Px(info.length);
            } else {
                style.width = Val::Px(info.length);
                style.height = Val::Px(BAR_WIDTH);
            }
        } else if interaction == &Interaction::Pressed
            || clicked_slider.map_or(false, |(ent, _)| ent == bar.parent)
        {
            // jump the slider to the clicked position
            let Vec2 { x: left, y: top } = transform.translation().xy() - node.size() * 0.5;
            let relative_position = cursor_position - Vec2::new(left, top);
            let slider_len = info.length * info.ratio;
            let position = if bar.vertical {
                (relative_position.y - slider_len * 0.5) / (info.length - slider_len)
            } else {
                (relative_position.x - slider_len * 0.5) / (info.length - slider_len)
            };
            info.update_slider = Some(UpdateSliderPosition::Abs(position));
        }
    }

    // sliders
    for (entity, mut slider, mut style) in sliders.iter_mut() {
        let source = if slider.vertical {
            &mut vertical_scrollers
        } else {
            &mut horizontal_scrollers
        };
        let Some(info) = source.get(&slider.parent) else {
            commands.entity(entity).despawn_recursive();
            continue;
        };

        // if anything changes we will redraw the slider and re-paginate the content
        let mut update_position = false;

        if info.redraw {
            // parent moved/resized or content moved/resized
            update_position = true;
        } else if let Some(position) = info.update_slider {
            // the container or the bar have triggered a slider update
            slider.position = match position {
                UpdateSliderPosition::Abs(p) => p.clamp(0.0, 1.0),
                UpdateSliderPosition::Rel(r) => (slider.position + r).clamp(0.0, 1.0),
            };
            update_position = true;
        }

        if update_position {
            // redraw slider
            let slider_len = info.length * info.ratio;
            if slider.vertical {
                style.width = Val::Px(BAR_WIDTH);
                style.height = Val::Px(slider_len);
                let slider_start =
                    info.bar_position.y + (info.length - slider_len) * slider.position;
                style.left = Val::Px(info.bar_position.x);
                style.top = Val::Px(slider_start);
            } else {
                style.width = Val::Px(slider_len);
                style.height = Val::Px(BAR_WIDTH);
                let slider_start =
                    info.bar_position.x + (info.length - slider_len) * slider.position;
                style.left = Val::Px(slider_start);
                style.top = Val::Px(info.bar_position.y);
            }

            // re-paginate content
            let mut style = nodes.get_mut(info.content).unwrap().1;
            let offset = info.slide_amount * -slider.position;
            if slider.vertical {
                style.top = Val::Px(offset.y);
            } else {
                style.left = Val::Px(offset.x);
            }
        }

        source.remove(&slider.parent);
    }

    // create any required bars/sliders that we didn't find existing above
    let mut init_bar = |entity: Entity, info: ScrollInfo, vertical: bool| {
        let bar_size = if vertical {
            (Val::Px(BAR_WIDTH), Val::Px(info.length))
        } else {
            (Val::Px(info.length), Val::Px(BAR_WIDTH))
        };

        let children = [commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left: Val::Px(info.bar_position.x),
                        top: Val::Px(info.bar_position.y),
                        width: bar_size.0,
                        height: bar_size.1,
                        ..Default::default()
                    },
                    background_color: Color::rgba(0.5, 0.5, 0.5, 0.2).into(),
                    z_index: ZIndex::Local(1),
                    ..Default::default()
                },
                ScrollBar {
                    parent: entity,
                    vertical,
                },
                Interaction::default(),
                InteractStyles {
                    hover: Some(InteractStyle { background: Some(Color::GRAY), ..Default::default() }),
                    press: Some(InteractStyle { background: Some(Color::GRAY), ..Default::default() }),
                    inactive: Some(InteractStyle { background: Some(Color::rgba(0.5, 0.5, 0.5, 0.2)), ..Default::default() }),
                    ..Default::default()
                }
            ))
            .id()];

        commands.entity(entity).try_push_children(&children);

        let position = match info.start {
            StartPosition::Start => 0.0,
            StartPosition::Center => 0.5,
            StartPosition::End => 1.0,
        };
        let slider_len = info.length * info.ratio;

        let (left, top, width, height) = if vertical {
            let slider_start = info.bar_position.y + (info.length - slider_len) * position;
            (
                Val::Px(info.bar_position.x),
                Val::Px(slider_start),
                Val::Px(BAR_WIDTH),
                Val::Px(slider_len),
            )
        } else {
            let slider_start = info.bar_position.x + (info.length - slider_len) * position;
            (
                Val::Px(slider_start),
                Val::Px(info.bar_position.y),
                Val::Px(slider_len),
                Val::Px(BAR_WIDTH),
            )
        };

        let children = [commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left,
                        top,
                        width,
                        height,
                        ..Default::default()
                    },
                    background_color: Color::rgba(1.0, 1.0, 1.0, 0.2).into(),
                    z_index: ZIndex::Local(2),
                    ..Default::default()
                },
                Slider {
                    parent: entity,
                    vertical,
                    position,
                },
                Interaction::default(),
                InteractStyles {
                    hover: Some(InteractStyle { background: Some(Color::WHITE), ..Default::default() }),
                    press: Some(InteractStyle { background: Some(Color::WHITE), ..Default::default() }),
                    inactive: Some(InteractStyle { background: Some(Color::rgba(1.0, 1.0, 1.0, 0.2)), ..Default::default() }),
                    ..Default::default()
                }
            ))
            .id()];

        commands.entity(entity).try_push_children(&children);

        let mut style = nodes.get_mut(info.content).unwrap().1;
        let offset = info.slide_amount * -position;
        if vertical {
            style.top = Val::Px(offset.y);
        } else {
            style.left = Val::Px(offset.x);
        }
    };

    for (entity, info) in vertical_scrollers.drain() {
        init_bar(entity, info, true);
    }

    for (entity, info) in horizontal_scrollers.drain() {
        init_bar(entity, info, false);
    }
}

pub struct ScrollableTemplate;
impl DuiTemplate for ScrollableTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let scrollable = props
            .take::<Scrollable>("scroll-settings")?
            .unwrap_or_default();

        let panel_size = match scrollable.direction {
            ScrollDirection::Vertical(_) => (Val::Percent(100.0), Val::Px(100000.0)),
            ScrollDirection::Horizontal(_) => (Val::Px(100000.0), Val::Percent(100.0)),
            ScrollDirection::Both(_, _) => (Val::Px(100000.0), Val::Px(100000.0)),
        };

        let mut content = Entity::PLACEHOLDER;
        let mut results = Ok(Default::default());

        commands
            .insert(NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    min_width: Val::Percent(0.0),
                    min_height: Val::Percent(0.0),
                    max_width: Val::Percent(100.0),
                    max_height: Val::Percent(100.0),
                    overflow: Overflow::clip(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .with_children(|c| {
                c.spawn(NodeBundle {
                    style: Style {
                        width: panel_size.0,
                        height: panel_size.1,
                        // TODO this should be set based on direction
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(|commands| {
                    // TODO need one more layer for bidirectional scrolling
                    let mut content_cmds = commands.spawn(NodeBundle {
                        style: Style {
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                    results = ctx.apply_children(&mut content_cmds);
                    content = content_cmds.id();
                    commands.spacer();
                });
            });

        commands.try_insert((Interaction::default(), scrollable, ScrollContent(content)));
        results
    }
}

fn get_position(s: Option<&String>) -> Result<StartPosition, anyhow::Error> {
    Ok(match s.map(String::as_str) {
        Some("start") | None => StartPosition::Start,
        Some("end") => StartPosition::End,
        Some("center") => StartPosition::Center,
        _ => anyhow::bail!("unrecognised start-position"),
    })
}

fn get_bool(s: Option<&String>) -> Result<bool, anyhow::Error> {
    Ok(match s.map(String::as_str) {
        Some("true") | None => true,
        Some("false") => false,
        _ => anyhow::bail!("unrecognised bool"),
    })
}

pub struct VerticalScrollTemplate;
impl DuiTemplate for VerticalScrollTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let pos = get_position(props.borrow::<String>("start-position", ctx)?)?;
        let drag = get_bool(props.borrow::<String>("drag", ctx)?)?;
        let wheel = get_bool(props.borrow::<String>("wheel", ctx)?)?;

        props.insert_prop(
            "scroll-settings",
            Scrollable::new()
                .with_direction(ScrollDirection::Vertical(pos))
                .with_drag(drag)
                .with_wheel(wheel),
        );

        ctx.render_template(commands, "scrollable-base", props)
    }
}

pub struct HorizontalScrollTemplate;
impl DuiTemplate for HorizontalScrollTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let pos = get_position(props.borrow::<String>("start-position", ctx)?)?;
        let drag = get_bool(props.borrow::<String>("drag", ctx)?)?;
        let wheel = get_bool(props.borrow::<String>("wheel", ctx)?)?;

        props.insert_prop(
            "scroll-settings",
            Scrollable::new()
                .with_direction(ScrollDirection::Horizontal(pos))
                .with_drag(drag)
                .with_wheel(wheel),
        );

        ctx.render_template(commands, "scrollable-base", props)
    }
}

pub struct TwoWayScrollTemplate;
impl DuiTemplate for TwoWayScrollTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let pos_x = get_position(props.borrow::<String>("start-position-x", ctx)?)?;
        let pos_y = get_position(props.borrow::<String>("start-position-y", ctx)?)?;
        let drag = get_bool(props.borrow::<String>("drag", ctx)?)?;
        let wheel = get_bool(props.borrow::<String>("wheel", ctx)?)?;

        props.insert_prop(
            "scroll-settings",
            Scrollable::new()
                .with_direction(ScrollDirection::Both(pos_x, pos_y))
                .with_drag(drag)
                .with_wheel(wheel),
        );

        ctx.render_template(commands, "scrollable-base", props)
    }
}
