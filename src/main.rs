//! Upddayett's School of Biddness. Lesson 3: Money Laundering (Legally).

mod game;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

fn main() {
    // `UPD_WINDOW=960x600` opens a smaller window (for checking the compact layout).
    let (w, h) = std::env::var("UPD_WINDOW")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        .unwrap_or((1600, 900));
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Upddayett's School of Biddness - Lesson 3: Money Laundering (Legally)".into(),
                resolution: (w, h).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(game::GamePlugin)
        .run();
}
