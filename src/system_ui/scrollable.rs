use bevy::{input::mouse::MouseWheel, prelude::*, utils::HashMap, window::PrimaryWindow};

pub struct ScrollablePlugin;

impl Plugin for ScrollablePlugin {
    fn build(&self, app: &mut App) {
        app.add_system(update_scrollables);
    }
}

pub trait SpawnScrollable {
    fn spawn_scrollable(
        &mut self,
        bundle: impl Bundle,
        scrollable: Scrollable,
        spawn_children: impl FnOnce(&mut ChildBuilder),
    );
}

impl SpawnScrollable for ChildBuilder<'_, '_, '_> {
    fn spawn_scrollable(
        &mut self,
        bundle: impl Bundle,
        scrollable: Scrollable,
        spawn_children: impl FnOnce(&mut ChildBuilder),
    ) {
        let panel_size = match scrollable.direction {
            ScrollDirection::Vertical(_) => Size::height(Val::Px(100000.0)),
            ScrollDirection::Horizontal(_) => Size::width(Val::Px(100000.0)),
            ScrollDirection::Both(_, _) => Size::all(Val::Px(100000.0)),
        };

        let mut content = Entity::PLACEHOLDER;

        self.spawn((bundle, scrollable))
            .with_children(|commands| {
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            size: panel_size,
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
                        commands.spawn(NodeBundle {
                            style: Style {
                                flex_grow: 1.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                    });
            })
            .insert(ScrollContent(content));
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
        Changed<GlobalTransform>,
        Changed<Node>,
        &Interaction,
    )>,
    mut bars: Query<
        (Entity, &ScrollBar, &mut Style, &Interaction),
        (Without<Scrollable>, Without<Slider>),
    >,
    mut sliders: Query<
        (Entity, &mut Slider, &mut Style, &Interaction),
        (Without<Scrollable>, Without<ScrollBar>),
    >,
    mut clicked_slider: Local<Option<(Entity, Vec2)>>,
    mut wheel: EventReader<MouseWheel>,
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

    let Ok(window) = window.get_single() else { return; };
    let cursor_position = window.cursor_position().unwrap_or_default() * Vec2::new(1.0, -1.0)
        + Vec2::new(0.0, window.height());

    let previously_clicked = std::mem::take(&mut *clicked_slider);

    let mut vertical_scrollers = HashMap::default();
    let mut horizontal_scrollers = HashMap::default();

    // gather scrollable components that need scrollbars
    for (
        entity,
        mut scrollable,
        scroll_content,
        node,
        transform,
        transform_changed,
        node_changed,
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
        if scrollable.drag && interaction == &Interaction::Clicked {
            if let Some((prev_entity, prev_pos)) = previously_clicked.as_ref() {
                if prev_entity == &entity {
                    let delta = cursor_position - *prev_pos;
                    new_slider_deltas = Some(delta / slide_amount);
                }
            }

            *clicked_slider = Some((entity, cursor_position));
        }
        if scrollable.wheel && interaction != &Interaction::None {
            for ev in wheel.iter() {
                let unit = match ev.unit {
                    bevy::input::mouse::MouseScrollUnit::Line => 20.0,
                    bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
                };
                *new_slider_deltas.get_or_insert(Default::default()) +=
                    Vec2::new(ev.x, ev.y) * unit / slide_amount;
            }
        }

        // the reported content-size is rounded, and occasionally repositioning when it changes causes a loop of +/- 1 pixel
        // so we allow 1 pixel tolerance (new content smaller) before redrawing
        let change = scrollable.content_size - child_size;
        let redraw = transform_changed
            || node_changed
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
                        bar_position: ui_position * 0.0 + Vec2::new(5.0, parent_size.y - 10.0),
                        length: parent_size.x - 20.0,
                        start,
                        redraw,
                        update_slider: new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.x)),
                    },
                );
            }
        } else {
            // or if we don't need a scrollbar, make sure the content position is at zero
            style.position.left = Val::Px(0.0);
        }

        if ratio.y < 1.0 {
            if let Some(start) = scrollable.direction.vertical() {
                vertical_scrollers.insert(
                    entity,
                    ScrollInfo {
                        content: scroll_content.0,
                        slide_amount,
                        ratio: ratio.y,
                        bar_position: ui_position * 0.0 + Vec2::new(parent_size.x - 10.0, 5.0),
                        length: parent_size.y - 20.0,
                        start,
                        redraw,
                        update_slider: new_slider_deltas.map(|d| UpdateSliderPosition::Rel(-d.y)),
                    },
                );
            }
        } else {
            style.position.top = Val::Px(0.0);
        }

        scrollable.content_size = child_size;
    }

    // bars
    for (entity, bar, mut style, interaction) in bars.iter_mut() {
        let source = if bar.vertical {
            &mut vertical_scrollers
        } else {
            &mut horizontal_scrollers
        };

        let Some(info) = source.get_mut(&bar.parent) else {
            commands.entity(entity).despawn();
            continue;
        };

        if info.redraw {
            // parent either moved, was resized or the content size changed. in any case, reposition/resize the bars
            style.position = UiRect {
                left: Val::Px(info.bar_position.x),
                top: Val::Px(info.bar_position.y),
                ..Default::default()
            };
            if bar.vertical {
                style.size = Size::new(Val::Px(5.0), Val::Px(info.length));
            } else {
                style.size = Size::new(Val::Px(info.length), Val::Px(5.0));
            }
        } else if interaction == &Interaction::Clicked {
            // jump the slider to the clicked position
            let (Val::Px(left), Val::Px(top)) = (style.position.left, style.position.top) else { continue; };
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
    for (entity, mut slider, mut style, interaction) in sliders.iter_mut() {
        let source = if slider.vertical {
            &mut vertical_scrollers
        } else {
            &mut horizontal_scrollers
        };
        let Some(info) = source.get(&slider.parent) else {
            commands.entity(entity).despawn();
            continue;
        };

        // if anything changes we will redraw the slider and re-paginate the content
        let mut update_position = false;

        if info.redraw {
            // parent moved/resized or content moved/resized
            update_position = true;
        } else if interaction == &Interaction::Clicked {
            // use slider click in priority over bar or container
            if let Some((prev_entity, prev_pos)) = previously_clicked.as_ref() {
                if prev_entity == &entity {
                    let delta = cursor_position - *prev_pos;
                    let delta = if slider.vertical { delta.y } else { delta.x };

                    if delta != 0.0 {
                        let slider_len = info.length * info.ratio;
                        let relative_delta = delta / (info.length - slider_len);

                        slider.position = (slider.position + relative_delta).clamp(0.0, 1.0);
                        update_position = true;
                    }
                }
            }

            *clicked_slider = Some((entity, cursor_position));
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
                style.size = Size::new(Val::Px(5.0), Val::Px(slider_len));
                let slider_start =
                    info.bar_position.y + (info.length - slider_len) * slider.position;
                style.position = UiRect {
                    left: Val::Px(info.bar_position.x),
                    top: Val::Px(slider_start),
                    ..Default::default()
                };
            } else {
                style.size = Size::new(Val::Px(slider_len), Val::Px(5.0));
                let slider_start =
                    info.bar_position.x + (info.length - slider_len) * slider.position;
                style.position = UiRect {
                    left: Val::Px(slider_start),
                    top: Val::Px(info.bar_position.y),
                    ..Default::default()
                };
            }

            // re-paginate content
            let mut style = nodes.get_component_mut::<Style>(info.content).unwrap();
            let offset = info.slide_amount * -slider.position;
            if slider.vertical {
                style.position.top = Val::Px(offset.y);
            } else {
                style.position.left = Val::Px(offset.x);
            }
        }

        source.remove(&slider.parent);
    }

    // create any required bars/sliders that we didn't find existing above
    let mut init_bar = |entity: Entity, info: ScrollInfo, vertical: bool| {
        let bar_size = if vertical {
            Size::new(Val::Px(5.0), Val::Px(info.length))
        } else {
            Size::new(Val::Px(info.length), Val::Px(5.0))
        };

        commands.entity(entity).with_children(|commands| {
            commands.spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        position: UiRect {
                            left: Val::Px(info.bar_position.x),
                            top: Val::Px(info.bar_position.y),
                            ..Default::default()
                        },
                        size: bar_size,
                        ..Default::default()
                    },
                    background_color: Color::GRAY.into(),
                    z_index: ZIndex::Local(1),
                    ..Default::default()
                },
                ScrollBar {
                    parent: entity,
                    vertical,
                },
                Interaction::default(),
            ));
        });

        let position = match info.start {
            StartPosition::Start => 0.0,
            StartPosition::Center => 0.5,
            StartPosition::End => 1.0,
        };
        let slider_len = info.length * info.ratio;

        let (ui_position, slider_size) = if vertical {
            let slider_start = info.bar_position.y + (info.length - slider_len) * position;
            (
                UiRect {
                    left: Val::Px(info.bar_position.x),
                    top: Val::Px(slider_start),
                    ..Default::default()
                },
                Size::new(Val::Px(5.0), Val::Px(slider_len)),
            )
        } else {
            let slider_start = info.bar_position.x + (info.length - slider_len) * position;
            (
                UiRect {
                    left: Val::Px(slider_start),
                    top: Val::Px(info.bar_position.y),
                    ..Default::default()
                },
                Size::new(Val::Px(slider_len), Val::Px(5.0)),
            )
        };

        commands.entity(entity).with_children(|commands| {
            commands.spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        position: ui_position,
                        size: slider_size,
                        ..Default::default()
                    },
                    background_color: Color::WHITE.into(),
                    z_index: ZIndex::Local(2),
                    ..Default::default()
                },
                Slider {
                    parent: entity,
                    vertical,
                    position,
                },
                Interaction::default(),
            ));
        });

        let mut style = nodes.get_component_mut::<Style>(info.content).unwrap();
        let offset = info.slide_amount * -position;
        if vertical {
            style.position.top = Val::Px(offset.y);
        } else {
            style.position.left = Val::Px(offset.x);
        }
    };

    for (entity, info) in vertical_scrollers.drain() {
        init_bar(entity, info, true);
    }

    for (entity, info) in horizontal_scrollers.drain() {
        init_bar(entity, info, false);
    }
}
