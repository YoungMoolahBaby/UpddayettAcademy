//! `UPD_SHOT=1`: run a fast spin cycle, save screenshots to `shots/`, exit.
//! `UPD_PROGRAM=0..3` picks the wash program (default Normal).
//! For checking the look without sitting in front of the window.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use super::sim::{Laundromat, Mode};

pub fn enabled() -> bool {
    std::env::var_os("UPD_SHOT").is_some()
}

pub fn drive(
    mut commands: Commands,
    mut lm: ResMut<Laundromat>,
    mut frame: Local<u32>,
    mut stage: Local<u32>,
    mut since: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    let f = *frame;
    if f == 30 {
        lm.watch = 4.0;
        if let Some(p) = std::env::var("UPD_PROGRAM").ok().and_then(|s| s.parse::<usize>().ok()) {
            lm.program = p.min(super::sim::PROGRAMS.len() - 1);
        }
        lm.start_cycle();
        *since = f;
    }
    if f < 30 {
        return;
    }
    // Stages follow the cycle's progress, so any program length works.
    let p = lm.progress();
    let done = lm.mode == Mode::Done;
    let name = match *stage {
        0 if p >= 0.15 || done => Some("1_spinning_hot"),
        1 if p >= 0.6 || done => Some("2_cooling"),
        2 if done && f >= *since + 60 => Some("3_locked"),
        _ => None,
    };
    if !done || *stage < 2 {
        *since = f;
    }
    if let Some(name) = name {
        std::fs::create_dir_all("shots").ok();
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/{name}.png")));
        *stage += 1;
        *since = f;
    } else if *stage == 3 && f >= *since + 30 {
        // The last screenshot has had a few frames to land on disk.
        exit.write(AppExit::Success);
    }
    if f > 30 + 60 * 180 {
        error!("screenshot mode timed out after 3 minutes");
        exit.write(AppExit::error());
    }
}
