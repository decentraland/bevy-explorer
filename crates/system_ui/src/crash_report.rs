use std::{io::Read, path::PathBuf};

use analytics::{data_definition::SegmentEventExplorerError, segment_system::SegmentMetricsEvents};
use bevy::prelude::*;

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

fn setup(report: Res<CrashReport>, mut metrics: ResMut<SegmentMetricsEvents>) {
    let mut f = match std::fs::File::open(&report.0) {
        Ok(f) => f,
        Err(e) => {
            warn!("failed to open log for crash report: {e}");
            return;
        }
    };
    let mut buf = Vec::default();
    if let Err(e) = f.read_to_end(&mut buf) {
        warn!("failed to read log for crash report: {e}");
        return;
    }

    let start = buf.len().saturating_sub(31000);
    let Ok(error_message) = std::str::from_utf8(&buf[start..]) else {
        warn!("failed to convert crash log to utf8");
        return;
    };

    metrics.add_event(analytics::data_definition::SegmentEvent::ExplorerError(
        SegmentEventExplorerError {
            error_type: "Crash".to_owned(),
            error_message: error_message.to_owned(),
            error_stack: String::default(),
        },
    ));

    let touch = report.0.parent().unwrap().join(format!(
        "{}.touch",
        report.0.file_name().unwrap().to_string_lossy()
    ));
    std::fs::remove_file(touch.clone()).unwrap();

    info!("crash report sent");
}
