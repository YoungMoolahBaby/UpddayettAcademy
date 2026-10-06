//! Lesson 3 in 3D: the laundromat trade computer, rendered with Bevy.

mod arrows;
mod counter;
mod scene;
mod shots;
mod sim;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(sim::Laundromat::new())
            .init_gizmo_group::<arrows::TradeArrows>()
            .init_gizmo_group::<arrows::LockedArrows>()
            .init_gizmo_group::<arrows::Neon>()
            .add_systems(Startup, (scene::setup, arrows::configure, counter::setup.after(scene::setup)))
            .add_systems(
                Update,
                (
                    sim::step_sim,
                    (
                        scene::shake_washer,
                        scene::bend_strips,
                        scene::update_leds,
                        scene::flicker_lights,
                        scene::salty_props,
                        scene::place_board_cam,
                        arrows::draw,
                        arrows::neon,
                        counter::fly,
                        counter::draw_sign,
                    ),
                )
                    .chain(),
            )
            .add_systems(EguiPrimaryContextPass, ui::panels);
        if shots::enabled() {
            app.add_systems(Update, shots::drive.after(sim::step_sim));
        }
    }
}
