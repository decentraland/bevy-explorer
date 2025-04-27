use std::{path::PathBuf, str::FromStr};

use anyhow::anyhow;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashSet,
};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    rpc::RpcCall,
    structs::{IVec2Arg, SettingsTab},
    util::{ModifyComponentExt, TaskCompat, TaskExt},
};
use ipfs::{ipfs_path::IpfsPath, ChangeRealmEvent, IpfsAssetServer};
use serde::Deserialize;
use ui_core::{
    button::DuiButton,
    combo_box::ComboBox,
    interact_style::Active,
    text_entry::TextEntryValue,
    toggle::Toggled,
    ui_actions::{close_ui_happy, Click, DataChanged, On, UiCaller},
};

use crate::profile::{close_settings, OnCloseEvent, SettingsDialog};

pub struct DiscoverSettingsPlugin;

impl Plugin for DiscoverSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                set_discover_content,
                (update_results, update_page).run_if(|q: Query<&SettingsTab>| {
                    q.get_single()
                        .is_ok_and(|tab| tab == &SettingsTab::Discover)
                }),
            ),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiscoverCategory {
    Art,
    Crypto,
    Social,
    Game,
    Shop,
    Education,
    Music,
    Poi,
    Fashion,
    Sports,
    Casino,
    Business,
    Featured,
}

impl DiscoverCategory {
    fn text(&self) -> String {
        match self {
            DiscoverCategory::Art => "Art",
            DiscoverCategory::Crypto => "Crypto",
            DiscoverCategory::Social => "Social",
            DiscoverCategory::Game => "Game",
            DiscoverCategory::Shop => "Shop",
            DiscoverCategory::Education => "Education",
            DiscoverCategory::Music => "Music",
            DiscoverCategory::Poi => "Point of Interest",
            DiscoverCategory::Fashion => "Fashion",
            DiscoverCategory::Sports => "Sports",
            DiscoverCategory::Casino => "Casino",
            DiscoverCategory::Business => "Business",
            DiscoverCategory::Featured => "Featured",
        }
        .to_owned()
    }

    fn param(&self) -> &'static str {
        match self {
            DiscoverCategory::Art => "art",
            DiscoverCategory::Crypto => "crypto",
            DiscoverCategory::Social => "social",
            DiscoverCategory::Game => "game",
            DiscoverCategory::Shop => "shop",
            DiscoverCategory::Education => "education",
            DiscoverCategory::Music => "music",
            DiscoverCategory::Poi => "poi",
            DiscoverCategory::Fashion => "fashion",
            DiscoverCategory::Sports => "sports",
            DiscoverCategory::Casino => "casino",
            DiscoverCategory::Business => "business",
            DiscoverCategory::Featured => "featured",
        }
    }

    fn iter() -> impl Iterator<Item = DiscoverCategory> {
        [
            DiscoverCategory::Art,
            DiscoverCategory::Crypto,
            DiscoverCategory::Social,
            DiscoverCategory::Game,
            DiscoverCategory::Shop,
            DiscoverCategory::Education,
            DiscoverCategory::Music,
            DiscoverCategory::Poi,
            DiscoverCategory::Fashion,
            DiscoverCategory::Sports,
            DiscoverCategory::Casino,
            DiscoverCategory::Business,
            DiscoverCategory::Featured,
        ]
        .into_iter()
    }

    fn from(text: &str) -> Option<Self> {
        Self::iter().find(|c| c.param() == text)
    }
}

#[derive(Default, PartialEq)]
pub enum SortBy {
    #[default]
    MostLiked,
    MostActive,
    Newest,
    MostVisited,
}

impl SortBy {
    fn text(&self) -> String {
        match self {
            SortBy::MostLiked => "Most Liked",
            SortBy::MostActive => "Most Active",
            SortBy::Newest => "Newest",
            SortBy::MostVisited => "Most Visited",
        }
        .to_owned()
    }

    fn param(&self) -> &'static str {
        match self {
            SortBy::MostLiked => "like_score",
            SortBy::MostActive => "most_active",
            SortBy::Newest => "updated_at",
            SortBy::MostVisited => "user_visits",
        }
    }

    fn iter() -> impl Iterator<Item = SortBy> {
        [
            SortBy::MostLiked,
            SortBy::MostActive,
            SortBy::Newest,
            SortBy::MostVisited,
        ]
        .into_iter()
    }

    fn from(value: &str) -> Self {
        Self::iter().find(|s| s.text() == value).unwrap()
    }

    fn index(&self) -> usize {
        Self::iter().position(|s| &s == self).unwrap()
    }
}

#[derive(Component)]
pub struct DiscoverSettings {
    category_filter: HashSet<DiscoverCategory>,
    search_filter: Option<String>,
    data: Vec<DiscoverPage>,
    has_more: bool,
    task: Option<Task<Result<DiscoverPages, anyhow::Error>>>,
    order_by: SortBy,
    worlds: bool,
    search_timer: f32,
    client: reqwest::Client,
}

impl DiscoverSettings {
    fn new(client: reqwest::Client) -> Self {
        Self {
            category_filter: default(),
            search_filter: default(),
            data: default(),
            has_more: default(),
            task: default(),
            order_by: default(),
            worlds: default(),
            search_timer: default(),
            client,
        }
    }
    fn clear_and_request(&mut self) {
        self.data.clear();
        self.has_more = false;
        self.request();
    }

    fn request(&mut self) {
        let mut url = if self.worlds {
            "https://places.decentraland.org/api/worlds/?limit=50"
        } else {
            "https://places.decentraland.org/api/places/?limit=50"
        }
        .to_string();

        url = format!("{url}&offset={}", self.data.len());

        for cat in &self.category_filter {
            url = format!("{url}&categories={}", cat.param());
        }

        url = format!("{url}&order_by={}", self.order_by.param());

        debug!("request: {url}");

        let client = self.client.clone();
        self.task = Some(IoTaskPool::get().spawn_compat(async move {
            let response = client.get(url).send().await?;

            response
                .json::<DiscoverPages>()
                .await
                .map_err(|e| anyhow!(e))
        }));
    }
}

fn set_discover_content(
    mut commands: Commands,
    dialog: Query<(Entity, Ref<SettingsDialog>)>,
    mut q: Query<(Entity, &SettingsTab, Option<&mut DiscoverSettings>), Changed<SettingsTab>>,
    mut prev_tab: Local<Option<SettingsTab>>,
    dui: Res<DuiRegistry>,
    ipfas: IpfsAssetServer,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab, maybe_discover_settings) in q.iter_mut() {
        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Discover {
            return;
        }

        let mut new_settings = DiscoverSettings::new(ipfas.ipfs().client());
        let is_new = maybe_discover_settings.is_none();
        let discover_settings = match maybe_discover_settings {
            Some(s) => s.into_inner(),
            None => &mut new_settings,
        };

        commands.entity(ent).despawn_descendants();

        let category_buttons = DiscoverCategory::iter()
            .map(|cat| {
                let content = commands
                    .spawn_template(
                        &dui,
                        "discover-category-button",
                        DuiProps::new().with_prop("text", cat.text()).with_prop(
                            "img",
                            format!("images/discover/{}.png", cat.text().to_lowercase()),
                        ),
                    )
                    .unwrap()
                    .root;

                DuiButton {
                    children: Some(content),
                    onclick: Some(On::<Click>::new(
                        move |mut commands: Commands,
                              caller: Res<UiCaller>,
                              mut buttons: Query<&mut Active>,
                              mut settings: Query<&mut DiscoverSettings>| {
                            let Ok(mut settings) = settings.get_single_mut() else {
                                warn!("no settings");
                                return;
                            };

                            let is_active = buttons.get_mut(caller.0).map(|b| b.0).unwrap_or(false);

                            if is_active {
                                commands.entity(caller.0).insert(Active(false));
                                settings.category_filter.remove(&cat);
                            } else {
                                commands.entity(caller.0).insert(Active(true));
                                settings.category_filter.insert(cat);
                            }

                            settings.clear_and_request();
                        },
                    )),
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();

        let props = DuiProps::new()
            .with_prop("category-buttons", category_buttons)
            .with_prop(
                "sort-options",
                SortBy::iter().map(|sb| sb.text()).collect::<Vec<_>>(),
            )
            .with_prop("initial-sort", discover_settings.order_by.index() as isize)
            .with_prop(
                "sort-by-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&ComboBox>,
                     mut settings: Query<&mut DiscoverSettings>| {
                        let Some(value) = q.get(caller.0).ok().and_then(|cb| cb.selected()) else {
                            warn!("no value from sort combo?");
                            return;
                        };
                        let Ok(mut settings) = settings.get_single_mut() else {
                            warn!("no settings");
                            return;
                        };
                        settings.order_by = SortBy::from(value.as_str());
                        settings.clear_and_request();
                    },
                ),
            )
            .with_prop(
                "world-toggle",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     toggle: Query<&Toggled>,
                     mut settings: Query<&mut DiscoverSettings>| {
                        let Ok(toggle) = toggle.get(caller.0) else {
                            warn!("no toggle");
                            return;
                        };

                        let Ok(mut settings) = settings.get_single_mut() else {
                            warn!("no settings");
                            return;
                        };

                        settings.worlds = toggle.0;
                        settings.clear_and_request();
                    },
                ),
            )
            .with_prop(
                "initial-filter",
                discover_settings.search_filter.clone().unwrap_or_default(),
            )
            .with_prop(
                "filter-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&TextEntryValue>,
                     mut settings: Query<&mut DiscoverSettings>| {
                        let Ok(value) = q.get(caller.0).map(|te| te.0.clone()) else {
                            warn!("no value from text entry?");
                            return;
                        };
                        if settings.single().search_filter.as_deref().unwrap_or("") == value {
                            // no change
                            return;
                        }
                        if value.is_empty() {
                            settings.single_mut().search_filter = None;
                        } else {
                            let mut settings = settings.single_mut();
                            settings.search_filter = Some(value);
                            settings.search_timer = 1.0;
                        }
                    },
                ),
            );

        commands
            .entity(ent)
            .apply_template(&dui, "discover", props)
            .unwrap();
        if discover_settings.data.is_empty() {
            discover_settings.clear_and_request();
        }

        if is_new {
            commands.entity(ent).try_insert(new_settings);
        }
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct DiscoverPage {
    pub title: String,
    contact_name: Option<String>,
    description: Option<String>,
    base_position: String,
    image: String,
    world_name: Option<String>,
    user_count: usize,
    favorites: usize,
    user_visits: Option<usize>,
    like_score: Option<f32>,
    likes: usize,
    categories: Vec<String>,
    content_rating: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl DiscoverPage {
    pub fn dummy(coords: IVec2) -> Self {
        Self {
            title: format!("({}, {})", coords.x, coords.y),
            base_position: format!("{},{}", coords.x, coords.y),
            image: "https://realm-provider.decentraland.org/content/contents/bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y".to_owned(),
            ..Default::default()
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct DiscoverPages {
    pub total: usize,
    pub data: Vec<DiscoverPage>,
}

fn update_results(mut q: Query<&mut DiscoverSettings>) {
    for mut settings in q.iter_mut() {
        if let Some(mut task) = settings.bypass_change_detection().task.take() {
            match task.complete() {
                Some(Ok(res)) => {
                    settings.data.extend(res.data);
                    settings.has_more = res.total > settings.data.len();
                }
                Some(Err(e)) => error!("places fetch failed: {e:?}"),
                None => settings.bypass_change_detection().task = Some(task),
            }
        }
    }
}

fn update_page(
    mut commands: Commands,
    settings: Query<(Entity, &DiscoverSettings, &DuiEntities), Changed<DiscoverSettings>>,
    dui: Res<DuiRegistry>,
    ipfas: IpfsAssetServer,
    mut prev_count: Local<usize>,
    mut prev_search: Local<Option<String>>,
    time: Res<Time>,
) {
    let Ok((settings_ent, settings, components)) = settings.get_single() else {
        return;
    };

    if settings.task.is_some() {
        return;
    }

    let Some(mut commands) = components
        .get_named("items")
        .and_then(|e| commands.get_entity(e))
    else {
        warn!("no content node");
        return;
    };

    if settings.search_timer > 0.0 {
        let delta = time.delta_seconds();
        commands.commands().entity(settings_ent).modify_component(
            move |settings: &mut DiscoverSettings| {
                settings.search_timer = 0f32.max(settings.search_timer - delta);
            },
        );
        return;
    }

    commands.despawn_descendants();
    let mut visible_count = 0;

    if settings.search_filter != *prev_search {
        commands.commands().entity(settings_ent).modify_component(
            move |settings: &mut DiscoverSettings| {
                settings.clear_and_request();
            },
        );
        *prev_search = settings.search_filter.clone();
        return;
    }

    for item in settings.data.iter() {
        let visible = settings
            .search_filter
            .as_ref()
            .is_none_or(|filter| item.title.to_lowercase().contains(&filter.to_lowercase()));

        if !visible {
            continue;
        }

        visible_count += 1;

        let item = item.clone();
        let image_path = IpfsPath::new_from_url(&item.image, "image");
        let h_image = ipfas
            .asset_server()
            .load::<Image>(PathBuf::from(&image_path));

        let button = commands
            .spawn_template(
                &dui,
                "discover-page",
                DuiProps::new()
                    .with_prop("img", h_image.clone())
                    .with_prop("label", item.title.clone())
                    .with_prop("author", item.contact_name.clone().unwrap_or_default())
                    .with_prop("views", format!("{}", item.user_visits.unwrap_or_default()))
                    .with_prop(
                        "likes",
                        format!("{:.0}%", item.like_score.unwrap_or(0.0) * 100.0),
                    ),
            )
            .unwrap();
        commands.commands().entity(button.root).insert((
            Interaction::default(),
            On::<Click>::new(
                move |mut commands: Commands,
                      dui: Res<DuiRegistry>,
                      asset_server: Res<AssetServer>| {
                    spawn_discover_popup(&mut commands, &dui, &asset_server, &item);
                },
            ),
        ));
    }

    if settings.has_more {
        if visible_count == *prev_count || visible_count == 0 {
            let components = commands
                .spawn_template(
                    &dui,
                    "button",
                    DuiProps::new().with_prop(
                        "button-data",
                        DuiButton::new_disabled(format!(
                            "Loading {} ...",
                            settings.data.len() + 50
                        )),
                    ),
                )
                .unwrap();

            commands
                .commands()
                .entity(components.root)
                .modify_component(|style: &mut Style| style.min_width = Val::Vw(80.0));

            commands.commands().entity(settings_ent).modify_component(
                |settings: &mut DiscoverSettings| {
                    settings.has_more = false;
                    settings.request();
                },
            );
        } else {
            let components = commands
            .spawn_template(
                &dui,
                "button",
                DuiProps::new().with_prop(
                    "button-data",
                    DuiButton::new_enabled(
                        "Load More",
                        |caller: Res<UiCaller>,
                         mut commands: Commands,
                         mut settings: Query<&mut DiscoverSettings>| {
                            commands.entity(caller.0).despawn_recursive();
                            let Ok(mut settings) = settings.get_single_mut() else {
                                warn!("no settings");
                                return;
                            };

                            settings.has_more = false;
                            settings.request();
                        },
                    ),
                ),
            )
            .unwrap();

            commands
                .commands()
                .entity(components.root)
                .modify_component(|style: &mut Style| style.min_width = Val::Vw(80.0));
        }
    }

    *prev_count = visible_count;
}

pub fn spawn_discover_popup(
    commands: &mut Commands,
    dui: &DuiRegistry,
    asset_server: &AssetServer,
    item: &DiscoverPage,
) {
    let url = match &item.world_name {
        Some(name) => format!(
            "https://worlds-content-server.decentraland.org/world/{}",
            name.clone()
        ),
        None => "https://realm-provider-ea.decentraland.org/main".to_owned(),
    };

    let Ok(to) = IVec2Arg::from_str(&item.base_position) else {
        warn!("invalid location");
        return;
    };
    let system = move |mut settings: Query<&mut SettingsDialog>| {
        let cr_ev = ChangeRealmEvent {
            new_realm: url.clone(),
            content_server_override: None,
        };
        let rpc_ev = RpcCall::TeleportPlayer {
            scene: None,
            to: to.0,
            response: Default::default(),
        };

        if let Ok(mut settings) = settings.get_single_mut() {
            settings.on_close = Some(OnCloseEvent::ChangeRealm(cr_ev, rpc_ev));
        } else {
            warn!("no settings");
        }
    };

    let jump_in = On::<Click>::new(system.pipe(close_settings).pipe(close_ui_happy));

    let image_path = IpfsPath::new_from_url(&item.image, "image");
    let h_image = asset_server.load::<Image>(PathBuf::from(&image_path));

    let props = DuiProps::new()
        .with_prop("close", On::<Click>::new(DuiButton::close_dialog))
        .with_prop("img", h_image.clone())
        .with_prop("title", item.title.clone())
        .with_prop("author", item.contact_name.clone().unwrap_or_default())
        .with_prop(
            "likes",
            format!(
                "{:.0}% ({})",
                item.like_score.unwrap_or(0.0) * 100.0,
                item.likes
            ),
        )
        .with_prop("description", item.description.clone().unwrap_or_default())
        .with_prop("location", item.base_position.clone())
        .with_prop(
            "categories",
            item.categories
                .iter()
                .flat_map(|c| DiscoverCategory::from(c))
                .map(|cat| DuiButton::new_disabled(cat.text()))
                .collect::<Vec<_>>(),
        )
        .with_prop("rating", item.content_rating.clone())
        .with_prop("active", format!("{}", item.user_count))
        .with_prop("favorites", format!("{}", item.favorites))
        .with_prop(
            "visits",
            format!("{}", item.user_visits.unwrap_or_default()),
        )
        .with_prop(
            "updated",
            format!("{}", item.updated_at.format("%d/%m/%Y %H:%M")),
        )
        .with_prop("jump-in", jump_in);

    commands
        .spawn_template(dui, "discover-popup", props)
        .unwrap();
}
