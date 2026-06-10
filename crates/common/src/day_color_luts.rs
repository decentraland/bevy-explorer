//! The measured Unity-client skybox, parsed from the full analysis report.
//!
//! Source: research/skybox_analysis_report.json (from dcl-regenesislabs/
//! color-lighting-test-scene) — the Unity explorer's skybox was screenshotted
//! every hour in every cardinal direction plus straight up, and analyzed into
//! 10-stop vertical color gradients per image. This module embeds and parses
//! the complete report (24 hours x 5 orientations x 10 stops); nothing is
//! discarded or re-derived.

use bevy::math::Vec3;
use serde::Deserialize;
use std::sync::LazyLock;

const REPORT: &str = include_str!("../../../research/skybox_analysis_report.json");

/// vertical gradient stops per analyzed screenshot
pub const STOPS: usize = 10;
/// cardinal orientations in azimuth order (degrees: N=0, E=90, S=180, W=270)
pub const CARDINALS: [char; 4] = ['N', 'E', 'S', 'W'];

#[derive(Deserialize)]
struct Report {
    analyses: Vec<Analysis>,
}

#[derive(Deserialize)]
struct Analysis {
    orientation: String,
    hour: usize,
    vertical_gradient: Vec<GradientStop>,
}

#[derive(Deserialize)]
struct GradientStop {
    godot: Rgb,
}

#[derive(Deserialize)]
struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

pub struct MeasuredSky {
    /// [hour][cardinal N,E,S,W][stop 0=top of view .. 9=bottom]
    pub cardinal: [[[Vec3; STOPS]; 4]; 24],
    /// [hour][stop] — the straight-up view
    pub up: [[Vec3; STOPS]; 24],
    /// [hour] — average of all sky readings (up view + upper half of
    /// cardinal views; the lower half of cardinal views is water)
    pub ambient: [Vec3; 24],
    /// [hour] — the cardinal horizon reading (last stop above the waterline)
    pub fog: [Vec3; 24],
}

/// index of the last cardinal-view stop that is sky rather than water
/// (positions 0.0, 0.11, 0.22, 0.33, 0.44 of 10 — the rest is ocean)
pub const HORIZON_STOP: usize = 4;

pub static MEASURED_SKY: LazyLock<MeasuredSky> = LazyLock::new(|| {
    let report: Report =
        serde_json::from_str(REPORT).expect("invalid skybox_analysis_report.json");

    let mut sky = MeasuredSky {
        cardinal: [[[Vec3::ZERO; STOPS]; 4]; 24],
        up: [[Vec3::ZERO; STOPS]; 24],
        ambient: [Vec3::ZERO; 24],
        fog: [Vec3::ZERO; 24],
    };

    for a in &report.analyses {
        let grad: Vec<Vec3> = a
            .vertical_gradient
            .iter()
            .map(|s| Vec3::new(s.godot.r, s.godot.g, s.godot.b))
            .collect();
        let grad: [Vec3; STOPS] = grad.try_into().expect("expected 10 gradient stops");
        match a.orientation.as_str() {
            "U" => sky.up[a.hour] = grad,
            c => {
                let ci = CARDINALS
                    .iter()
                    .position(|&k| k.to_string() == c)
                    .expect("unknown orientation");
                sky.cardinal[a.hour][ci] = grad;
            }
        }
    }

    for h in 0..24 {
        let mut sum = Vec3::ZERO;
        let mut n = 0.0;
        for stop in &sky.up[h] {
            sum += *stop;
            n += 1.0;
        }
        for c in 0..4 {
            for stop in &sky.cardinal[h][c][..=HORIZON_STOP] {
                sum += *stop;
                n += 1.0;
            }
        }
        sky.ambient[h] = sum / n;

        let mut fog = Vec3::ZERO;
        for c in 0..4 {
            fog += sky.cardinal[h][c][HORIZON_STOP];
        }
        sky.fog[h] = fog / 4.0;
    }

    sky
});

fn lerp_hours(lut: &[Vec3; 24], day: f32) -> Vec3 {
    let h = day.rem_euclid(1.0) * 24.0;
    let i0 = (h.floor() as usize) % 24;
    let i1 = (i0 + 1) % 24;
    lut[i0].lerp(lut[i1], h.fract())
}

/// measured ambient color at normalized time of day (0.0 = midnight, 0.5 = noon)
pub fn sample_ambient(day: f32) -> Vec3 {
    lerp_hours(&MEASURED_SKY.ambient, day)
}

/// measured fog/horizon color at normalized time of day
pub fn sample_fog(day: f32) -> Vec3 {
    lerp_hours(&MEASURED_SKY.fog, day)
}
