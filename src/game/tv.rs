//! The laundromat TV: a wall set under the neon sign whose screen is Bevy UI
//! rendered to a texture. It flips between The Shrug Network (tonight's real
//! numbers, shrugged off) and PromiseTV (fictional candidates promising
//! everyone something Market St has one of). The copy is `trade::tv`.
//! `UPD_TV=shrug|promise` holds one channel (for screenshots).

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use cortenforge_play::trade::tv::{self, Promise};

use super::sim::Laundromat;

/// The screen texture, in pixels, and the screen in the room, in meters.
const TEX: UVec2 = UVec2::new(640, 360);
const SCREEN: Vec2 = Vec2::new(2.2, 2.2 * 9.0 / 16.0);
/// Hung on arms off the back wall, between the neon sign and the back row
/// of washers, tipped down toward the room.
const AT: Vec3 = Vec3::new(0.0, 2.4, -4.4);
const TILT: f32 = -0.12;
/// Seconds per channel visit, per story, and of static on a flip.
const VISIT: f32 = 14.0;
const STORY: f32 = 7.0;
const STATIC: f32 = 0.3;
/// Chyron speed, pixels per second.
const CRAWL: f32 = 90.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Channel {
    Shrug,
    Promise,
}

#[derive(Resource)]
pub struct Tv {
    pub channel: Channel,
    /// The PromiseTV ad on the air (None on the Shrug Network).
    pub promise: Option<Promise>,
    hold: Option<Channel>,
}

impl Default for Tv {
    fn default() -> Self {
        let hold = match std::env::var("UPD_TV").ok().as_deref() {
            Some("shrug") => Some(Channel::Shrug),
            Some("promise") => Some(Channel::Promise),
            _ => None,
        };
        Tv { channel: hold.unwrap_or(Channel::Shrug), promise: None, hold }
    }
}

/// The text pieces on the screen.
#[derive(Component, Clone, Copy, PartialEq)]
pub enum Part {
    Logo,
    Bug,
    Big,
    Sub,
    Crawl,
}

/// The chyron text that crawls.
#[derive(Component)]
pub struct Crawling;

/// The colored pieces on the screen.
#[derive(Component, Clone, Copy, PartialEq)]
pub enum Panel {
    Body,
    Header,
    Bar,
    Static,
}

/// A font from Windows' font folder, else Bevy's built-in one.
fn system_font(fonts: &mut Assets<Font>, file: &str) -> Handle<Font> {
    std::fs::read(format!("C:/Windows/Fonts/{file}")).map(|b| fonts.add(Font::from_bytes(b))).unwrap_or_default()
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
) {
    let mut image = Image::new_fill(
        Extent3d { width: TEX.x, height: TEX.y, ..default() },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let cam = commands
        .spawn((Camera2d, Camera { order: -1, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }, RenderTarget::Image(image.clone().into())))
        .id();

    let chunky = system_font(&mut fonts, "ariblk.ttf");
    let plain = system_font(&mut fonts, "arialbd.ttf");
    let font = |f: &Handle<Font>, size: f32| TextFont { font: f.clone().into(), font_size: FontSize::Px(size), ..default() };
    commands
        .spawn((
            Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, ..default() },
            BackgroundColor(Color::BLACK),
            Panel::Body,
            UiTargetCamera(cam),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    height: px(64),
                    padding: UiRect::horizontal(px(16)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                Panel::Header,
            ))
            .with_children(|h| {
                h.spawn((Text::new(""), font(&chunky, 34.0), TextColor::WHITE, Part::Logo));
                h.spawn((Text::new(""), font(&plain, 22.0), TextColor::WHITE, Part::Bug));
            });
            p.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(px(20)),
                row_gap: px(10),
                ..default()
            })
            .with_children(|b| {
                b.spawn((Text::new(""), font(&chunky, 42.0), TextColor::WHITE, Part::Big));
                b.spawn((Text::new(""), font(&plain, 30.0), TextColor::WHITE, Part::Sub));
            });
            p.spawn((Node { height: px(50), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::WHITE), Panel::Bar)).with_children(|c| {
                c.spawn((
                    Node { position_type: PositionType::Absolute, left: px(TEX.x as f32), top: px(8), ..default() },
                    Text::new(""),
                    TextLayout::no_wrap(),
                    font(&plain, 28.0),
                    TextColor::BLACK,
                    Part::Crawl,
                    Crawling,
                ));
            });
            // Static between channels.
            p.spawn((
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
                BackgroundColor(Color::NONE),
                Panel::Static,
            ));
        });

    // The set: a black bezel, the screen, and two arms back to the wall.
    let plastic = mats.add(StandardMaterial { base_color: Color::srgb(0.03, 0.03, 0.035), perceptual_roughness: 0.4, ..default() });
    // The picture shows as is: unlit, and drawn forward so it reads the same
    // under ray tracing (Solari shades deferred surfaces itself, and showed
    // an emissive picture as a faint wash).
    let screen = mats.add(StandardMaterial {
        base_color_texture: Some(image),
        unlit: true,
        opaque_render_method: bevy::material::OpaqueRendererMethod::Forward,
        ..default()
    });
    let frame = Transform::from_translation(AT).with_rotation(Quat::from_rotation_x(TILT));
    commands.spawn((frame, Visibility::default())).with_children(|p| {
        p.spawn((Mesh3d(meshes.add(Cuboid::new(SCREEN.x + 0.14, SCREEN.y + 0.14, 0.1))), MeshMaterial3d(plastic.clone()), Transform::default()));
        p.spawn((Mesh3d(meshes.add(Rectangle::from_size(SCREEN))), MeshMaterial3d(screen), Transform::from_xyz(0.0, 0.0, 0.051)));
        let reach = -5.95 - AT.z;
        for x in [-0.7, 0.7] {
            p.spawn((Mesh3d(meshes.add(Cuboid::new(0.06, 0.06, reach.abs()))), MeshMaterial3d(plastic.clone()), Transform::from_xyz(x, 0.2, reach / 2.0)));
        }
    });
}

/// What the screen shows at time `t`: the channel, and which story on it.
fn schedule(t: f32, hold: Option<Channel>) -> (Channel, usize, f32) {
    let visit = (t / VISIT) as usize;
    let into = t % VISIT;
    let channel = hold.unwrap_or(if visit.is_multiple_of(2) { Channel::Shrug } else { Channel::Promise });
    // Two stories per Shrug visit, one ad per PromiseTV visit.
    let story = match channel {
        Channel::Shrug => visit * 2 + (into / STORY) as usize,
        Channel::Promise => visit,
    };
    (channel, story, into)
}

pub fn update(
    time: Res<Time>,
    lm: Res<Laundromat>,
    mut tv: ResMut<Tv>,
    mut texts: Query<(&mut Text, &mut TextColor, &Part)>,
    mut panels: Query<(&mut BackgroundColor, &Panel)>,
    mut crawl: Query<(&mut Node, &ComputedNode), With<Crawling>>,
) {
    let t = time.elapsed_secs();
    let (channel, story, into) = schedule(t, tv.hold);
    tv.channel = channel;
    let w = &lm.tc.world;
    let gifts = lm.tc.cycles.iter().filter(|c| c.is_gift()).count();
    let brag = lm.sabotage().and_then(|s| s.brag());

    let gold = Color::srgb(1.0, 0.8, 0.2);
    let (logo, bug, big, sub, ticker, colors) = match channel {
        Channel::Shrug => {
            tv.promise = None;
            let news = tv::shrug(w, lm.tc.cycles.len() - gifts, brag);
            let h = &news[story % news.len()];
            let ticker = news.iter().map(|h| h.text()).collect::<Vec<_>>().join("   ///   ");
            let colors = [Color::srgb(0.06, 0.08, 0.14), Color::srgb(0.22, 0.26, 0.36), Color::srgb(0.9, 0.9, 0.88)];
            ("THE SHRUG NETWORK".to_string(), "LIVE-ISH".to_string(), h.fact.to_uppercase(), format!("Anyway, {}", h.anyway), ticker, colors)
        }
        Channel::Promise => {
            let ads = tv::promises(w);
            let ad = ads[story % ads.len()].clone();
            let ticker = ads
                .iter()
                .filter(|a| a.candidate != ad.candidate)
                .map(|a| format!("{}: {}", a.candidate, a.pitch))
                .chain([format!("Paid for by {} for Whatever", ad.candidate)])
                .collect::<Vec<_>>()
                .join("   ///   ");
            let colors = [Color::srgb(0.14, 0.05, 0.2), Color::srgb(0.35, 0.12, 0.45), gold];
            let big = format!("VOTE {}", ad.candidate.to_uppercase());
            let sub = ad.pitch.to_string();
            tv.promise = Some(ad);
            ("PROMISE TV".to_string(), "PAID AD".to_string(), big, sub, ticker, colors)
        }
    };

    for (mut text, mut color, part) in &mut texts {
        let (want, tint) = match part {
            Part::Logo => (&logo, if channel == Channel::Promise { gold } else { Color::WHITE }),
            Part::Bug => (&bug, Color::srgb(1.0, 0.3, 0.3)),
            Part::Big => (&big, if channel == Channel::Promise { gold } else { Color::WHITE }),
            Part::Sub => (&sub, Color::srgb(0.85, 0.85, 0.85)),
            Part::Crawl => (&ticker, Color::srgb(0.05, 0.05, 0.08)),
        };
        if text.0 != *want {
            text.0 = want.clone();
        }
        color.0 = tint;
    }
    // Snow for a moment after each flip; the hold channel doesn't flip.
    let snow = tv.hold.is_none() && into < STATIC;
    for (mut bg, panel) in &mut panels {
        bg.0 = match panel {
            Panel::Body => colors[0],
            Panel::Header => colors[1],
            Panel::Bar => colors[2],
            Panel::Static if snow => Color::srgb(0.5, 0.5, 0.5).with_luminance(0.3 + 0.4 * (t * 97.0).sin().abs()),
            Panel::Static => Color::NONE,
        };
    }
    // The chyron crawls in from the right and starts over once it's gone.
    for (mut node, computed) in &mut crawl {
        let width = computed.size().x.max(1.0);
        let x = TEX.x as f32 - (into * CRAWL) % (TEX.x as f32 + width);
        node.left = px(x);
    }
}
