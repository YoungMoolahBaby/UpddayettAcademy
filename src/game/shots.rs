//! `UPD_SHOT=1`: run a fast spin cycle, save screenshots to `shots/`, exit.
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
    mut shot: Local<u32>,
    mut done_at: Local<Option<u32>>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    let f = *frame;
    if f == 30 {
        lm.speed = 120.0;
        lm.start_cycle();
    }
    let name = match (f, lm.mode) {
        (90, _) => Some("1_spinning_hot"),
        (300, Mode::Cycle { .. }) => Some("2_cooling"),
        (_, Mode::Done) if *shot == 2 && f >= done_at.get_or_insert(f).saturating_add(60) => Some("3_locked"),
        _ => None,
    };
    if let Some(name) = name {
        std::fs::create_dir_all("shots").ok();
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/{name}.png")));
        *shot += 1;
    }
    if *shot == 3 && name.is_none() && lm.mode == Mode::Done {
        // Give the last screenshot a few frames to land on disk.
        if f.is_multiple_of(90) {
            exit.write(AppExit::Success);
        }
    }
}
