//! `UPD_SHOT=1`: run a fast spin cycle, save screenshots to `shots/`, exit.
//! `UPD_PROGRAM=0..3` picks the wash program (default Normal);
//! `UPD_WANT=who:what` (e.g. `upd:hub`) sets a want first.
//! `UPD_NEXT=1` goes to the next night first (checks the night switch).
//! `UPD_GIVE=what` (e.g. `phone`) has Upddayett give that away (before the want).
//! `UPD_BATTERY=1`, `UPD_IDLE=1`, `UPD_SHIELD=<strip>|found`: defenses (see below).
//! `UPD_SCOPE=<strip>`: put that trade's strip on the scope.
//! `UPD_RESPIN=1`: after the cycle, shield what the spin check caught (battery if the
//! power went out) and spin again (`shots/respin_*.png`).
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
    mut respun: Local<bool>,
    mut exit: MessageWriter<AppExit>,
    drum: Res<super::drum::DrumView>,
) {
    // The drum's model takes ~12 s to build; start once it tumbles.
    if !drum.ready() {
        return;
    }
    *frame += 1;
    let f = *frame;
    if f == 30 {
        lm.watch = 4.0;
        if std::env::var_os("UPD_NEXT").is_some() {
            lm.next_night();
        }
        if let Some(p) = std::env::var("UPD_PROGRAM").ok().and_then(|s| s.parse::<usize>().ok()) {
            lm.program = p.min(super::sim::PROGRAMS.len() - 1);
        }
        if let Ok(what) = std::env::var("UPD_GIVE") {
            match lm.give_menu.iter().map(|&(i, _)| i).find(|&i| lm.tc.world.items[i].name.to_lowercase().contains(&what.to_lowercase())) {
                Some(item) => lm.set_give(Some(item)),
                None => error!("UPD_GIVE: Upddayett has nothing like {what:?} to give"),
            }
        }
        // Defenses: UPD_BATTERY=1, UPD_IDLE=1 (idle check), UPD_SHIELD=<strip>
        // or UPD_SHIELD=found (over the strip the idle check flags).
        if std::env::var_os("UPD_BATTERY").is_some() {
            lm.set_battery(true);
        }
        let shield = std::env::var("UPD_SHIELD").ok();
        if std::env::var_os("UPD_IDLE").is_some() || shield.as_deref() == Some("found") {
            lm.idle_check();
        }
        match shield.as_deref() {
            Some("found") => {
                let at = lm.idle.as_ref().map(|s| s.iter().enumerate().fold((0, 0.0f64), |b, (i, x)| if x.abs() > b.1 { (i, x.abs()) } else { b }).0);
                lm.set_shield(at);
            }
            Some(s) => match s.parse() {
                Ok(at) => lm.set_shield(Some(at)),
                Err(_) => error!("UPD_SHIELD: a strip number or \"found\", not {s:?}"),
            },
            None => {}
        }
        if let Some((who, what)) = std::env::var("UPD_WANT").ok().as_deref().and_then(|w| w.split_once(':')) {
            let npc = lm.tc.world.find_npc(who);
            let item = lm.tc.world.find_item(what);
            match (npc, item) {
                (Some(n), Some(i)) => lm.set_want(Some((n, i))),
                _ => error!("UPD_WANT: no customer like {who:?} or item like {what:?}"),
            }
        }
        if let Some(k) = std::env::var("UPD_SCOPE").ok().and_then(|s| s.parse::<usize>().ok()) {
            lm.scope = (k < lm.n()).then_some(k);
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
        // Long enough for the crates to land with their new holders.
        2 if done && f >= *since + 120 => Some("3_locked"),
        _ => None,
    };
    if !done || *stage < 2 {
        *since = f;
    }
    if let Some(name) = name {
        std::fs::create_dir_all("shots").ok();
        let prefix = if *respun { "respin_" } else { "" };
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/{prefix}{name}.png")));
        *stage += 1;
        *since = f;
    } else if *stage == 3 && f >= *since + 30 && !*respun && std::env::var_os("UPD_RESPIN").is_some() {
        // Fight back: shield whatever the spin check caught, bring the
        // battery if the power went out, and spin again.
        *respun = true;
        if let Some((k, _)) = lm.spin_alarm() {
            lm.set_shield(Some(k));
        }
        if lm.power_out() {
            lm.set_battery(true);
        }
        lm.start_cycle();
        *stage = 0;
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
