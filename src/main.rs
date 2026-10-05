//! Upddayett's School of Biddness. Lesson 3: Money Laundering (Legally).

mod game;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Upddayett's School of Biddness - Lesson 3: Money Laundering (Legally)".into(),
                resolution: (1600, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(game::GamePlugin)
        .run();
}
