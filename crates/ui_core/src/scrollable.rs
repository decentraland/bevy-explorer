use bevy::{
    prelude::*, transform::TransformSystem, ui::ManualCursorPosition, utils::HashMap,
    window::PrimaryWindow,
};
use bevy_dui::{DuiContext, DuiProps, DuiRegistry, DuiTemplate};
use common::{
    inputs::{Action, CommonInputAction, SCROLL_SET},
    util::{ModifyComponentExt, ModifyDefaultComponentExt, TryPushChildrenEx},
};
use input_manager::{InputManager, InputPriority, InputType};

use crate::{
    bound_node::{BoundedNode, BoundedNodeBundle, NodeBounds},
    interact_style::{InteractStyle, InteractStyles},
    ui_actions::DataChanged,
};

use super::ui_builder::SpawnSpacer;

pub struct ScrollablePlugin;

impl Plugin for ScrollablePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                update_scrollables.after(TransformSystem::TransformPropagate),
            )
            .add_event::<ScrollTargetEvent>();
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
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

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum StartPosition {
    Start,
    #[default]
    Center,
    End,
    Explicit(f32),
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

#[derive(Component, Default, Debug)]
pub struct ScrollPosition {
    pub h: f32,
    pub v: f32,
}

#[derive(Component)]
pub struct Scrollable {
    pub direction: ScrollDirection,
    pub drag: bool,
    pub wheel: bool,
    content_size: Vec2,
    pub horizontal_bar: bool,
    pub vertical_bar: bool,
}

impl Default for Scrollable {
    fn default() -> Self {
        Self {
            direction: Default::default(),
            drag: Default::default(),
            wheel: Default::default(),
            content_size: Default::default(),
            horizontal_bar: true,
            vertical_bar: true,
        }
    }
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

    pub fn with_bars_visible(self, horizontal_bar: bool, vertical_bar: bool) -> Self {
        Self {
            horizontal_bar,
            vertical_bar,
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollTarget {
    Literal(Vec2),
    Entity(Entity),
}

#[derive(Event, Debug)]
pub struct ScrollTargetEvent {
    pub scrollable: Entity,
    pub position: ScrollTarget,
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_scrollables(
    mut commands: Commands,
    window: Query<&Window, With<PrimaryWindow>>,
    mut nodes: Query<
        (&Node, &mut Style, Option<&Children>),
        (Without<Scrollable>, Without<ScrollBar>, Without<Slider>),
    >,
    positions: Query<(&Node, &Transform, &Parent, &GlobalTransform)>,
    mut scrollables: Query<(
        Entity,
        &mut Scrollable,
        &ScrollContent,
        &Node,
        &GlobalTransform,
        Ref<GlobalTransform>,
        Ref<Node>,
        &Interaction,
        Option<&TargetCamera>,
    )>,
    mut bars: Query<
        (
            Entity,
            &ScrollBar,
            &mut Style,
            &Interaction,
            &Node,
            &GlobalTransform,
            Option<&TargetCamera>,
        ),
        (Without<Scrollable>, Without<Slider>),
    >,
    mut sliders: Query<
        (Entity, &mut Slider, &mut Style),
        (Without<Scrollable>, Without<ScrollBar>),
    >,
    mut clicked_slider: Local<Option<Entity>>,
    mut clicked_scrollable: Local<Option<(Entity, Vec2)>>,
    mut events: EventReader<ScrollTargetEvent>,
    cursors: Query<(Entity, &ManualCursorPosition)>,
    mut input_manager: InputManager,
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
        visible: bool,
    }

    let mut events = events
        .read()
        .map(|ev| (ev.scrollable, ev.position))
        .collect::<HashMap<_, _>>();

    for action in SCROLL_SET.0 {
        input_manager
            .priorities()
            .release(InputType::Action(action), InputPriority::Scroll);
    }
    input_manager.priorities().release(
        InputType::Action(Action::Scene(CommonInputAction::IaPointer)),
        InputPriority::Scroll,
    );

    let Ok(window) = window.get_single() else {
        return;
    };

    let bar_width = (window.width().min(window.height()) * 0.02).ceil();

    let Some(window_cursor_position) = window.cursor_position() else {
        return;
    };
    let manual_cursor_positions: HashMap<_, _> = cursors.iter().collect();
    let cursor_position = |camera: Option<&TargetCamera>| -> Option<Vec2> {
        if let Some(camera) = camera {
            if let Some(position) = manual_cursor_positions.get(&camera.0) {
                return position.0;
            }
        }

        Some(window_cursor_position)
    };

    if input_manager.just_up(CommonInputAction::IaPointer) {
        *clicked_slider = None;
        *clicked_scrollable = None;
    }

    let mut vertical_scrollers = HashMap::default();
    let mut horizontal_scrollers = HashMap::default();

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
        maybe_target_camera,
    ) in scrollables.iter_mut()
    {
        let Ok((child_node, mut style, _)) = nodes.get_mut(scroll_content.0) else {
            warn!("scrollable hierarchy is broken");
            continue;
        };

        let cursor_position = cursor_position(maybe_target_camera);

        let child_size = child_node.size();
        let parent_size = node.size();
        let ratio = parent_size / child_size;
        let ui_position = transform.translation().truncate() - parent_size * 0.5;
        let slide_amount = child_size - parent_size;

        // calculate based on event
        let mut new_slider_abses: Option<Vec2> = None;
        if let Some(position) = events.remove(&entity) {
            match position {
                ScrollTarget::Literal(pos) => {
                    new_slider_abses = Some(pos);
                }
                ScrollTarget::Entity(e) => {
                    let mut translation = Vec2::ZERO;

                    let mut i = e;
                    loop {
                        let Ok((_, transform, parent, _)) = positions.get(i) else {
                            warn!("scroll target not found");
                            translation = Vec2::ZERO;
                            break;
                        };

                        translation += transform.translation.xy();
                        if parent.get() == scroll_content.0 {
                            break;
                        }
                        i = parent.get();
                    }

                    let overflow = (child_size - parent_size).max(Vec2::ONE);
                    let abs_mid =
                        (translation + overflow * 0.5).clamp(Vec2::ZERO, overflow) / overflow;
                    new_slider_abses = Some(abs_mid);
                    debug!("{translation} -> parent size: {parent_size}, child_size: {child_size}, abs {abs_mid:?}");
                }
            }
        }

        let mut new_slider_deltas = None;
        if let Some(cursor_position) = cursor_position {
            // calculate deltas based on drag or mouse wheel in the parent container
            if scrollable.drag && clicked_slider.is_none() {
                if let Some((prev_entity, prev_pos)) = clicked_scrollable.as_ref() {
                    if prev_entity == &entity {
                        let delta = cursor_position - *prev_pos;
                        new_slider_deltas = Some(delta / slide_amount);
                    }
                }

                if clicked_scrollable.is_none_or(|(prev_entity, _)| prev_entity == entity)
                    && interaction != &Interaction::None
                    && input_manager.is_down(CommonInputAction::IaPointer, InputPriority::Scroll)
                {
                    *clicked_scrollable = Some((entity, cursor_position));
                }
            }
            if scrollable.wheel {
                // we check only if the cursor is within our frame - this means scrollables can still be scrolled when
                // blocking dialogs cover them. TODO: make this better, requires either
                // - check all children for interaction (yuck)
                // - add some context to FocusPolicy (e.g. FocusPolicy::Block(HashSet<Buttons>))
                // - add another system to manage "container" focus based on child focus
                if clicked_scrollable.is_none_or(|(prev_entity, _)| prev_entity == entity)
                    && cursor_position.clamp(ui_position, ui_position + parent_size)
                        == cursor_position
                {
                    for action in SCROLL_SET.0 {
                        input_manager
                            .priorities()
                            .reserve(InputType::Action(action), InputPriority::Scroll);
                    }
                    let scroll_delta =
                        input_manager.get_analog(SCROLL_SET, InputPriority::Scroll);
                    *new_slider_deltas.get_or_insert(Default::default()) +=
                        scroll_delta / slide_amount;
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
                            + Vec2::new(bar_width, parent_size.y - bar_width - 5.0),
                        length: parent_size.x - bar_width * 3.0,
                        start,
                        redraw,
                        update_slider: new_slider_abses
                            .map(|a| UpdateSliderPosition::Abs(a.x))
                            .or(new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.x))),
                        visible: scrollable.horizontal_bar,
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
                            + Vec2::new(parent_size.x - bar_width - 5.0, bar_width),
                        length: parent_size.y - bar_width * 3.0,
                        start,
                        redraw,
                        update_slider: new_slider_abses
                            .map(|a| UpdateSliderPosition::Abs(a.y))
                            .or(new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.y))),
                        visible: scrollable.vertical_bar,
                    },
                );
            }
        } else {
            style.top = Val::Px(0.0);
        }

        scrollable.content_size = child_size;
    }

    // bars
    for (entity, bar, mut style, interaction, node, transform, maybe_target_camera) in
        bars.iter_mut()
    {
        let source = if bar.vertical {
            &mut vertical_scrollers
        } else {
            &mut horizontal_scrollers
        };

        let Some(info) = source.get_mut(&bar.parent) else {
            commands.entity(entity).despawn_recursive();
            continue;
        };

        style.display = if info.visible {
            Display::Flex
        } else {
            Display::None
        };

        if info.redraw {
            // parent either moved, was resized or the content size changed. in any case, reposition/resize the bars
            style.left = Val::Px(info.bar_position.x);
            style.top = Val::Px(info.bar_position.y);
            if bar.vertical {
                style.width = Val::Px(bar_width);
                style.height = Val::Px(info.length);
            } else {
                style.width = Val::Px(info.length);
                style.height = Val::Px(bar_width);
            }
        }

        let Some(cursor_position) = cursor_position(maybe_target_camera) else {
            continue;
        };

        if interaction != &Interaction::None {
            input_manager.priorities().reserve(
                InputType::Action(Action::Scene(CommonInputAction::IaPointer)),
                InputPriority::Scroll,
            );
        }

        if (interaction != &Interaction::None
            && input_manager.just_down(CommonInputAction::IaPointer, InputPriority::Scroll))
            || clicked_slider.is_some_and(|ent| ent == entity)
        {
            // jump the slider to the clicked position
            let Vec2 { x: left, y: top } = transform.translation().xy() - node.size() * 0.5;
            let relative_position = cursor_position - Vec2::new(left, top);
            let slider_len = (info.length * info.ratio).max(bar_width);
            let position = if bar.vertical {
                (relative_position.y - slider_len * 0.5) / (info.length - slider_len)
            } else {
                (relative_position.x - slider_len * 0.5) / (info.length - slider_len)
            };
            info.update_slider = Some(UpdateSliderPosition::Abs(position));
            *clicked_slider = Some(entity);
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

        style.display = if info.visible {
            Display::Flex
        } else {
            Display::None
        };

        // if anything changes we will redraw the slider and re-paginate the content
        let mut update_position = false;

        if let Some(position) = info.update_slider {
            // the container or the bar have triggered a slider update
            slider.position = match position {
                UpdateSliderPosition::Abs(p) => p.clamp(0.0, 1.0),
                UpdateSliderPosition::Rel(r) => (slider.position + r).clamp(0.0, 1.0),
            };
            update_position = true;
        } else if info.redraw {
            // parent moved/resized or content moved/resized
            update_position = true;
        }

        if update_position {
            // redraw slider
            let slider_len = (info.length * info.ratio).max(bar_width);
            if slider.vertical {
                style.width = Val::Px(bar_width);
                style.height = Val::Px(slider_len);
                let slider_start =
                    info.bar_position.y + (info.length - slider_len) * slider.position;
                style.left = Val::Px(info.bar_position.x);
                style.top = Val::Px(slider_start);
            } else {
                style.width = Val::Px(slider_len);
                style.height = Val::Px(bar_width);
                let slider_start =
                    info.bar_position.x + (info.length - slider_len) * slider.position;
                style.left = Val::Px(slider_start);
                style.top = Val::Px(info.bar_position.y);
            }

            // re-paginate content
            let mut style = nodes.get_mut(info.content).unwrap().1;
            let offset = info.slide_amount * -slider.position;
            if slider.vertical {
                style.top = Val::Px(offset.y.floor());
                let position = slider.position;
                commands
                    .entity(slider.parent)
                    .try_insert(DataChanged)
                    .modify_component(move |pos: &mut ScrollPosition| {
                        pos.v = position;
                    });
            } else {
                style.left = Val::Px(offset.x.floor());
                let position = slider.position;
                commands
                    .entity(slider.parent)
                    .try_insert(DataChanged)
                    .modify_component(move |pos: &mut ScrollPosition| {
                        pos.h = position;
                    });
            }
        }

        source.remove(&slider.parent);
    }

    // create any required bars/sliders that we didn't find existing above
    let mut init_bar = |entity: Entity, info: ScrollInfo, vertical: bool| {
        let bar_size = if vertical {
            (Val::Px(bar_width), Val::Px(info.length))
        } else {
            (Val::Px(info.length), Val::Px(bar_width))
        };

        let children = [commands
            .spawn((
                NodeBounds {
                    corner_size: Val::Px(bar_width * 0.5),
                    border_size: Val::Px(bar_width * 0.125),
                    border_color: Color::NONE,
                    ..Default::default()
                },
                BoundedNodeBundle {
                    bounded: BoundedNode {
                        image: None,
                        color: Color::srgba(0.5, 0.5, 0.5, 0.2).into(),
                    },
                    style: Style {
                        display: if info.visible {
                            Display::Flex
                        } else {
                            Display::None
                        },
                        position_type: PositionType::Absolute,
                        left: Val::Px(info.bar_position.x),
                        top: Val::Px(info.bar_position.y),
                        width: bar_size.0,
                        height: bar_size.1,
                        ..Default::default()
                    },
                    z_index: ZIndex::Local(1),
                    ..Default::default()
                },
                ScrollBar {
                    parent: entity,
                    vertical,
                },
                Interaction::default(),
                InteractStyles {
                    hover: Some(InteractStyle {
                        background: Some(Color::srgba(0.3, 0.3, 0.3, 0.6)),
                        ..Default::default()
                    }),
                    press: Some(InteractStyle {
                        background: Some(Color::srgba(0.3, 0.3, 0.3, 0.8)),
                        ..Default::default()
                    }),
                    inactive: Some(InteractStyle {
                        background: Some(Color::srgba(0.3, 0.3, 0.3, 0.3)),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ))
            .id()];

        commands.entity(entity).try_push_children(&children);

        let mut position = match info.start {
            StartPosition::Start => 0.0,
            StartPosition::Center => 0.5,
            StartPosition::End => 1.0,
            StartPosition::Explicit(v) => v.clamp(0.0, 1.0),
        };

        if let Some(UpdateSliderPosition::Abs(val)) = info.update_slider {
            position = val;
        }

        let slider_len = (info.length * info.ratio).max(bar_width);

        let (left, top, width, height) = if vertical {
            let slider_start = info.bar_position.y + (info.length - slider_len) * position;
            (
                Val::Px(info.bar_position.x),
                Val::Px(slider_start),
                Val::Px(bar_width),
                Val::Px(slider_len),
            )
        } else {
            let slider_start = info.bar_position.x + (info.length - slider_len) * position;
            (
                Val::Px(slider_start),
                Val::Px(info.bar_position.y),
                Val::Px(slider_len),
                Val::Px(bar_width),
            )
        };

        let children = [commands
            .spawn((
                NodeBounds {
                    corner_size: Val::Px(bar_width * 0.5),
                    border_size: Val::Px(bar_width * 0.25),
                    border_color: Color::NONE,
                    ..Default::default()
                },
                BoundedNodeBundle {
                    bounded: BoundedNode {
                        image: None,
                        color: Color::srgba(1.0, 1.0, 1.0, 0.2).into(),
                    },
                    style: Style {
                        display: if info.visible {
                            Display::Flex
                        } else {
                            Display::None
                        },
                        position_type: PositionType::Absolute,
                        left,
                        top,
                        width,
                        height,
                        ..Default::default()
                    },
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
                    hover: Some(InteractStyle {
                        background: Some(Color::WHITE),
                        ..Default::default()
                    }),
                    press: Some(InteractStyle {
                        background: Some(Color::WHITE),
                        ..Default::default()
                    }),
                    inactive: Some(InteractStyle {
                        background: Some(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ))
            .id()];

        commands.entity(entity).try_push_children(&children);

        let mut style = nodes.get_mut(info.content).unwrap().1;
        let offset = info.slide_amount * -position;
        if vertical {
            style.top = Val::Px(offset.y.floor());
            commands
                .entity(entity)
                .default_and_modify_component(move |pos: &mut ScrollPosition| {
                    pos.v = position;
                })
                .try_insert(DataChanged);
        } else {
            style.left = Val::Px(offset.x.floor());
            commands
                .entity(entity)
                .default_and_modify_component(move |pos: &mut ScrollPosition| {
                    pos.h = position;
                })
                .try_insert(DataChanged);
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

        let mut results = Ok(Default::default());
        let content = props.take::<Entity>("content")?.unwrap_or_else(|| {
            let mut root_cmds = commands.commands();
            let mut content_cmds = root_cmds.spawn(NodeBundle {
                style: Style {
                    ..Default::default()
                },
                ..Default::default()
            });

            results = ctx.apply_children(&mut content_cmds);
            content_cmds.id()
        });

        let panel_size = match scrollable.direction {
            ScrollDirection::Vertical(_) => (Val::Percent(100.0), Val::Px(100000.0)),
            ScrollDirection::Horizontal(_) => (Val::Px(100000.0), Val::Percent(100.0)),
            ScrollDirection::Both(_, _) => (Val::Px(100000.0), Val::Px(100000.0)),
        };

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
                .push_children(&[content]);
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
