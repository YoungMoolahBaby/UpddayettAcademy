//! Lesson 3 in 3D: the laundromat trade computer, rendered with Bevy.

mod ads;
mod arrows;
mod cards;
mod counter;
mod drum;
mod printer;
#[cfg(feature = "solari")]
mod rt;
mod scene;
mod shots;
mod tv;
mod sim;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let lm = sim::Laundromat::new();
        app.insert_resource(cards::Cards::new(lm.night))
            .insert_resource(lm)
            .init_resource::<tv::Tv>()
            .init_gizmo_group::<arrows::TradeArrows>()
            .init_gizmo_group::<arrows::LockedArrows>()
            .init_gizmo_group::<arrows::Neon>()
            .add_systems(Startup, (scene::setup, arrows::configure, counter::setup.after(scene::setup), drum::setup.after(scene::setup), printer::setup, tv::setup))
            .add_systems(
                Update,
                (
                    sim::step_sim,
                    printer::update,
                    (
                        scene::shake_washer,
                        scene::run_row,
                        drum::update,
                        scene::bend_strips,
                        scene::update_leds,
                        scene::flicker_lights,
                        scene::salty_props,
                        scene::place_board_cam,
                        arrows::draw,
                        arrows::neon,
                        counter::fly,
                        counter::draw_sign,
                        cards::hide_inset,
                        tv::update,
                    ),
                )
                    .chain(),
            )
            .add_systems(EguiPrimaryContextPass, (cards::draw, ui::panels.run_if(cards::clear)).chain());
        #[cfg(feature = "solari")]
        app.add_plugins(rt::RtPlugin);
        if cards::shots_enabled() {
            app.add_systems(Update, cards::shots);
        } else if shots::enabled() {
            app.add_systems(Update, shots::drive.after(sim::step_sim));
        }
    }
}
