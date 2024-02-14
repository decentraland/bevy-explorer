use anyhow::anyhow;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::util::TaskExt;
use ipfs::{ChangeRealmEvent, CurrentRealm};
use isahc::ReadResponseExt;
use serde::Deserialize;
use ui_core::{
    button::DuiButton,
    ui_actions::{Click, On},
};

#[derive(Event, Default)]
pub struct ChangeRealmDialog;

#[derive(Component)]
pub struct ServerList {
    task: Task<Result<std::vec::Vec<ServerDesc>, anyhow::Error>>,
    root_id: Entity,
}

#[derive(Component)]
pub struct UpdateRealmText;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerDesc {
    server_name: String,
    url: String,
    users_count: i32,
}

pub struct ChangeRealmPlugin;

impl Plugin for ChangeRealmPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChangeRealmDialog>()
            .add_systems(Update, (change_realm_dialog, update_server_list));
    }
}

fn change_realm_dialog(
    mut commands: Commands,
    mut events: EventReader<ChangeRealmDialog>,
    dui: ResMut<DuiRegistry>,
    realm: Res<CurrentRealm>,
    // _ipfas: IpfsAssetServer,
    mut q: Query<&mut Text, With<UpdateRealmText>>,
) {
    if realm.is_changed() {
        for mut text in q.iter_mut() {
            text.sections[0].value = format!(
                "Realm: {}",
                realm
                    .config
                    .realm_name
                    .clone()
                    .unwrap_or_else(|| String::from("<none>"))
            );
        }
    }

    if events.read().last().is_none() {
        return;
    }

    // let endpoint = ipfas
    //     .ipfs()
    //     .lambda_endpoint()
    //     .unwrap_or_else(|| String::from("https://realm-provider.decentraland.org/lambdas"));
    // let target_url = format!("{endpoint}/explore/realms");

    // hard coded since the other doesn't list main
    let target_url = "https://realm-provider.decentraland.org/realms";

    let task: Task<Result<Vec<ServerDesc>, anyhow::Error>> = IoTaskPool::get().spawn(async move {
        let mut response = isahc::get(target_url).map_err(|e| anyhow!(e))?;
        response.json::<Vec<ServerDesc>>().map_err(|e| anyhow!(e))
    });

    let mut root = commands.spawn_empty();
    let root_id = root.id();

    let components = dui
        .apply_template(
            &mut root,
            "change-realm",
            DuiProps::new()
                .with_prop(
                    "realm",
                    realm
                        .config
                        .realm_name
                        .clone()
                        .unwrap_or(String::from("<none>")),
                )
                .with_prop("buttons", vec![DuiButton::close("cancel")]),
        )
        .unwrap();
    commands
        .entity(components.named("server-list"))
        .insert(ServerList { task, root_id });
}

fn update_server_list(
    mut commands: Commands,
    mut q: Query<(Entity, &mut ServerList)>,
    dui: Res<DuiRegistry>,
    current_realm: Res<CurrentRealm>,
) {
    for (ent, mut server_list) in q.iter_mut() {
        if let Some(res) = server_list.task.complete() {
            commands.entity(ent).remove::<ServerList>();

            match res {
                Ok(mut servers) => {
                    let root_id = server_list.root_id;
                    commands.entity(ent).despawn_descendants();
                    servers.sort_by_key(|server| -server.users_count);
                    for server in servers {
                        commands.entity(ent).spawn_template(
                            &dui,
                            "server-item",
                            DuiProps::new()
                                .with_prop("enabled", Some(&server.server_name) != current_realm.config.realm_name.as_ref())
                                .with_prop("name", server.server_name)
                                .with_prop("users", format!("{}", server.users_count))
                                .with_prop(
                                    "onclick",
                                    On::<Click>::new(move |mut commands: Commands, mut e: EventWriter<ChangeRealmEvent>| {
                                        e.send(ChangeRealmEvent {
                                            new_realm: server.url.clone(),
                                        });
                                        commands.entity(root_id).despawn_recursive();
                                    }),
                                )
                        ).unwrap();
                    }
                }
                Err(e) => warn!("lambda query failed: {e}"),
            }
        }
    }
}
