use std::path::PathBuf;

use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::structs::Version;
use ui_core::button::DuiButton;

use crate::SystemUiRoot;

pub struct CrashReportPlugin {
    pub file: PathBuf,
}

impl Plugin for CrashReportPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CrashReport(self.file.clone()));
        app.add_systems(OnEnter::<ui_core::State>(ui_core::State::Ready), setup);
    }
}

#[derive(Resource)]
pub struct CrashReport(PathBuf);

fn setup(
    mut commands: Commands,
    root: Res<SystemUiRoot>,
    dui: Res<DuiRegistry>,
    version: Res<Version>,
    report: Res<CrashReport>,
) {
    let version = version.0.clone();
    let subject = format!("bevy explorer crash-report version {version}");
    let subject = urlencoding::encode(&subject);
    let body = format!("please attach (or copy-paste) `{}` (sorry i know this is rubbish).\n\nAnd please add any extra relevant info about what happened.\n\n Thanks!", report.0.to_string_lossy());
    let body = urlencoding::encode(&body);
    let address = "rob.macdonald@bevydev.co.uk";
    let mailto = format!("mailto:{address}?subject={subject}&body={body}");

    let file_a = report.0.parent().unwrap().join(format!(
        "{}.touch",
        report.0.file_name().unwrap().to_string_lossy()
    ));
    let file_b = file_a.clone();

    println!("file: {:?}", file_a);

    let components = commands
        .entity(root.0)
        .spawn_template(
            &dui,
            "text-dialog",
            DuiProps::new()
                .with_prop("title", "Crashed".to_owned())
                .with_prop("body", "It looks like the application crashed, would you like to send a crash report?\nThis will open a mail client, the reporting mechanism will be improved in future".to_owned())
                .with_prop(
                    "buttons",
                    vec![
                        DuiButton::new_enabled_and_close("Don't Send", move || { std::fs::remove_file(file_a.clone()).unwrap(); }),
                        DuiButton::new_enabled_and_close("Send", move || { opener::open(mailto.clone()).unwrap(); std::fs::remove_file(file_b.clone()).unwrap(); }),
                    ],
                ),
        )
        .unwrap();

    commands
        .entity(components.root)
        .insert(ZIndex::Global(1000));
}
