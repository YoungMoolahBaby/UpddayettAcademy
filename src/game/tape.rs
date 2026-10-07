//! Bring your own tape (Step 8): a beat-up boombox on the floor by
//! Upddayett's printer plays the player's own music. We ship none.
//!
//! - Drop audio files (or a folder) on the window, or put them in `music/`.
//!   A dropped file is copied into `music/`, so it's there next launch.
//! - MP3, Ogg, FLAC and WAV. Each file is checked by the decoder on a worker
//!   thread first, so a bad file gets a note instead of a crash.
//! - It stays out of the way: silent until given a tape, no autoplay at
//!   launch, and it can be switched off. Off, it's just a prop: no controls
//!   and no hotkeys (M pauses, N skips).
//! - Click the label under the boombox for the controls. The deck remembers
//!   its volume, shuffle and power in `music/deck.txt`.
//!
//! The prop is cf-design CSG; the audio is Bevy's (rodio).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{AudioSink, AudioSinkPlayback, Source, Volume};
use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiContexts, egui};
use cortenforge::cf_design::Solid;
use nalgebra::{Point3, UnitQuaternion, Vector3};

use super::scene::MainCam;

/// The player's music, next to the game (git-ignored).
const DIR: &str = "music";
const SETTINGS: &str = "music/deck.txt";
const FORMATS: [&str; 4] = ["mp3", "ogg", "flac", "wav"];
/// Where the boombox sits: on the floor left of the printer's stool, turned
/// a little toward the camera.
const BOOMBOX: Vec3 = Vec3::new(-0.78, 0.0, 2.5);
const YAW: f32 = 0.3;
/// The cassette window's reels (boombox frame, y up, +z out the front).
const REEL_Y: f32 = 0.19;
const REEL_X: f32 = 0.036;
const REEL_Z: f32 = 0.062;
/// Tape pack radius on a reel: empty hub to full.
const PACK: (f32, f32) = (0.012, 0.028);
/// Reel turn rate while playing (rad/s).
const SPIN: f32 = 3.0;
/// A tape of unknown length winds over this long, then starts over.
const UNKNOWN: f32 = 240.0;
pub const TAPE_CREAM: egui::Color32 = egui::Color32::from_rgb(240, 228, 196);
const INK: egui::Color32 = egui::Color32::from_rgb(40, 34, 60);

/// A file the worker read and the decoder accepted (or why not).
type Loaded = Result<(Arc<[u8]>, Option<Duration>), String>;

#[derive(Resource)]
pub struct Deck {
    /// Switched on. Off, the boombox is just a prop.
    pub on: bool,
    pub tapes: Vec<PathBuf>,
    /// The tape in the deck (an index into `tapes`).
    pub at: usize,
    pub shuffle: bool,
    /// 0..1 on the knob (the gain is its square: the knob feels even).
    pub volume: f32,
    pub paused: bool,
    /// The controls are open (click the label).
    pub open: bool,
    /// A file is being dragged over the window.
    pub hover: bool,
    /// The playing (or paused) tape's entity, and its length if known.
    current: Option<Entity>,
    length: Option<Duration>,
    loading: Option<Arc<Mutex<Option<Loaded>>>>,
    /// A short note under the label (a bad file, tapes added) and its time left.
    pub note: Option<(String, f32)>,
    /// Seconds since a tape went in (the clunk).
    clunk: f32,
    clunk_sound: Handle<AudioSource>,
    rng: u64,
    /// Shot mode: don't copy files or write settings.
    test: bool,
}

impl Deck {
    pub fn title(&self) -> Option<String> {
        self.tapes.get(self.at).map(|p| title(p))
    }

    pub fn loaded(&self) -> bool {
        self.current.is_some() || self.loading.is_some()
    }

    /// Put tape `k` in: stop what's playing and read the file on a worker.
    pub fn play(&mut self, k: usize, commands: &mut Commands) {
        self.stop(commands);
        let Some(path) = self.tapes.get(k).cloned() else {
            return;
        };
        self.at = k;
        self.paused = false;
        let slot = Arc::new(Mutex::new(None));
        let out = slot.clone();
        std::thread::Builder::new()
            .name("tape".into())
            .spawn(move || {
                let r = read(&path);
                if let Ok(mut s) = out.lock() {
                    *s = Some(r);
                }
            })
            .expect("tape thread");
        self.loading = Some(slot);
    }

    pub fn stop(&mut self, commands: &mut Commands) {
        if let Some(e) = self.current.take() {
            commands.entity(e).despawn();
        }
        self.loading = None;
        self.length = None;
    }

    /// The next tape (auto-reverse: after the last comes the first).
    pub fn next(&mut self, commands: &mut Commands) {
        let n = self.tapes.len();
        if n == 0 {
            return;
        }
        let k = if self.shuffle && n > 1 {
            // xorshift, any tape but this one
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            (self.at + 1 + (self.rng % (n as u64 - 1)) as usize) % n
        } else {
            (self.at + 1) % n
        };
        self.play(k, commands);
    }

    /// M: pause, resume, or start the tape that's in.
    pub fn toggle(&mut self, commands: &mut Commands) {
        if self.loaded() {
            self.paused = !self.paused;
        } else if !self.tapes.is_empty() {
            self.play(self.at, commands);
        }
    }

    pub fn switch(&mut self, on: bool, commands: &mut Commands) {
        self.on = on;
        if !on {
            self.stop(commands);
            self.open = false;
        }
        self.save();
    }

    /// Add dropped files: copy them into `music/` (on a worker) and play the
    /// first if the deck is idle. Returns how many were new.
    fn add(&mut self, dropped: &Path, commands: &mut Commands) -> usize {
        let files = audio_files(dropped);
        if files.is_empty() {
            let what = dropped.file_name().map_or_else(|| dropped.display().to_string(), |n| n.to_string_lossy().into_owned());
            self.note = Some((format!("not a tape: {what} (MP3, Ogg, FLAC or WAV)"), 5.0));
            return 0;
        }
        let mut first = None;
        let mut new = 0;
        for f in &files {
            // Already in the deck (same file name): play that one.
            let k = match self.tapes.iter().position(|t| t.file_name() == f.file_name()) {
                Some(k) => k,
                None => {
                    self.tapes.push(f.clone());
                    new += 1;
                    self.tapes.len() - 1
                }
            };
            first.get_or_insert(k);
        }
        if !self.test {
            let files = files.clone();
            std::thread::spawn(move || copy_in(&files));
        }
        if !self.loaded()
            && let Some(k) = first
        {
            self.play(k, commands);
        }
        new
    }

    fn save(&self) {
        if self.test {
            return;
        }
        let s = format!("on={}\nvolume={:.2}\nshuffle={}\n", self.on as u8, self.volume, self.shuffle as u8);
        if std::fs::create_dir_all(DIR).is_ok() {
            std::fs::write(SETTINGS, s).ok();
        }
    }

    fn gain(&self) -> Volume {
        Volume::Linear(self.volume * self.volume)
    }
}

fn title(p: &Path) -> String {
    let s = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().replace('_', " "));
    if s.chars().count() > 34 { format!("{}...", s.chars().take(32).collect::<String>()) } else { s }
}

fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| FORMATS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The audio files in a dropped path (a file, or a folder and its folders), sorted.
fn audio_files(p: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if p.is_dir() {
        let mut stack = vec![p.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let q = e.path();
                if q.is_dir() {
                    stack.push(q);
                } else if is_audio(&q) {
                    out.push(q);
                }
            }
        }
    } else if is_audio(p) {
        out.push(p.to_path_buf());
    }
    out.sort();
    out
}

/// Copy dropped files into `music/`, unless they're there already.
fn copy_in(files: &[PathBuf]) {
    let Ok(dir) = std::fs::create_dir_all(DIR).and_then(|_| std::fs::canonicalize(DIR)) else {
        return;
    };
    for f in files {
        let Some(name) = f.file_name() else { continue };
        let to = dir.join(name);
        let inside = std::fs::canonicalize(f).is_ok_and(|c| c.starts_with(&dir));
        if !inside && !to.exists() {
            std::fs::copy(f, &to).ok();
        }
    }
}

/// Read a tape and let the decoder look at it: Bevy's decoder unwraps, so a
/// file it can't play would take the game down (bevy_audio audio_source.rs:97-101).
fn read(path: &Path) -> Loaded {
    let bytes: Arc<[u8]> = std::fs::read(path).map_err(|e| e.to_string())?.into();
    let dec = rodio::Decoder::builder()
        .with_byte_len(bytes.len() as u64)
        .with_data(Cursor::new(bytes.clone()))
        .build()
        .map_err(|e| format!("can't play {}: {}", title(path), e.to_string().trim_end_matches('.').to_lowercase()))?;
    Ok((bytes, dec.total_duration()))
}

/// The settings file: `on`, `volume`, `shuffle`.
fn load_settings() -> (bool, f32, bool) {
    let (mut on, mut vol, mut shuffle) = (true, 0.5f32, false);
    for line in std::fs::read_to_string(SETTINGS).unwrap_or_default().lines() {
        match line.split_once('=') {
            Some(("on", v)) => on = v.trim() != "0",
            Some(("volume", v)) => vol = v.trim().parse().unwrap_or(vol),
            Some(("shuffle", v)) => shuffle = v.trim() == "1",
            _ => {}
        }
    }
    (on, vol.clamp(0.0, 1.0), shuffle)
}

/// The clunk of a tape going in, made here (a thud and a click) so we ship
/// no sound files: 16-bit mono WAV.
fn clunk_wav() -> Vec<u8> {
    let rate = 44_100u32;
    let n = (rate as f32 * 0.14) as usize;
    let mut seed = 0x2545_f491u32;
    let samples: Vec<i16> = (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let thud = (std::f32::consts::TAU * 90.0 * t).sin() * (-t * 40.0).exp();
            let click = noise * (-t * 300.0).exp() + noise * 0.5 * (-(t - 0.07).abs() * 400.0).exp();
            ((0.6 * thud + 0.35 * click).clamp(-1.0, 1.0) * 20_000.0) as i16
        })
        .collect();
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// The playing tape's sound.
#[derive(Component)]
pub struct TapeSound;
/// The boombox (it jolts when a tape goes in).
#[derive(Component)]
pub struct Boombox;
/// Reel 0 (supply, left) or 1 (take-up, right); its tape pack is a child.
#[derive(Component)]
pub struct Reel(usize);
#[derive(Component)]
pub struct Pack(usize);
/// The cassette in the window (shown while a tape is in).
#[derive(Component)]
pub struct Cassette;

/// The deck, from `music/` (nothing plays), and the boombox: cf-design CSG
/// in metres, y up, the front facing +z.
pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut sounds: ResMut<Assets<AudioSource>>) {
    let test = shots_enabled();
    let (on, volume, shuffle) = if test { (true, 0.5, false) } else { load_settings() };
    let tapes = if test { vec![] } else { audio_files(Path::new(DIR)) };
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
    commands.insert_resource(Deck {
        on,
        tapes,
        at: 0,
        shuffle,
        volume,
        paused: false,
        open: false,
        hover: false,
        current: None,
        length: None,
        loading: None,
        note: None,
        clunk: f32::MAX,
        clunk_sound: sounds.add(AudioSource { bytes: clunk_wav().into() }),
        rng: seed,
        test,
    });

    let v = |x: f64, y: f64, z: f64| Vector3::new(x, y, z);
    let mesh = |s: &Solid, res: f64| {
        let m = s.mesh(res);
        if test {
            println!("tape: boombox part {} tris at {res}", m.geometry.faces.len());
        }
        super::drum::to_mesh(&m)
    };
    let paint = |mats: &mut Assets<StandardMaterial>, c: Color, metal: f32, rough: f32| {
        mats.add(StandardMaterial { base_color: c, metallic: metal, perceptual_roughness: rough, ..default() })
    };
    let (w, h, d) = (0.33, 0.17, 0.08);
    // The case: a rounded box with two speaker wells and the cassette bay
    // cut out of the front.
    let speakers = [-0.2, 0.2];
    let mut case = Solid::cuboid(v(w - 0.02, h - 0.02, d - 0.02)).round(0.02).translate(v(0.0, h, 0.0));
    for x in speakers {
        case = case.subtract(Solid::cylinder(0.105, 0.03).translate(v(x, 0.16, d)));
    }
    case = case.subtract(Solid::cuboid(v(0.085, 0.052, 0.03)).translate(v(0.0, REEL_Y as f64, d)));
    // The handle and the antenna.
    let handle = Solid::pipe(vec![Point3::new(-0.25, 0.3, 0.0), Point3::new(-0.21, 0.43, 0.0), Point3::new(0.21, 0.43, 0.0), Point3::new(0.25, 0.3, 0.0)], 0.016);
    let antenna = Solid::pipe(vec![Point3::new(0.27, 0.33, -0.05), Point3::new(0.42, 0.78, -0.05)], 0.006).union(Solid::sphere(0.012).translate(v(0.42, 0.78, -0.05)));
    // Piano-key buttons along the top.
    let mut keys = Solid::cuboid(v(0.016, 0.012, 0.022)).translate(v(-0.1, 2.0 * h, 0.035));
    for k in 1..5 {
        keys = keys.union(Solid::cuboid(v(0.016, 0.012, 0.022)).translate(v(-0.1 + 0.04 * k as f64, 2.0 * h, 0.035)));
    }
    // Speaker grilles: a disc punched with a grid of holes, over a cone.
    let grille = Solid::cylinder(0.1, 0.004).subtract(Solid::cylinder(0.0065, 0.01).repeat_bounded(v(0.022, 0.022, 1.0), [9, 9, 1]));
    let cone = Solid::cone(0.095, 0.03).rotate(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI)).translate(v(0.0, 0.0, -0.03)).union(Solid::sphere(0.025).translate(v(0.0, 0.0, -0.035)));
    // The label strip over the cassette bay.
    let label = Solid::cuboid(v(0.085, 0.011, 0.004));
    // The cassette, and a reel hub with six teeth.
    let cassette = Solid::cuboid(v(0.078, 0.046, 0.006));
    let mut hub = Solid::cylinder(0.011, 0.004).subtract(Solid::cylinder(0.006, 0.01));
    for k in 0..6 {
        let a = k as f64 * std::f64::consts::TAU / 6.0;
        hub = hub.union(Solid::cuboid(v(0.0025, 0.0015, 0.004)).rotate(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), a)).translate(v(0.0065 * a.cos(), 0.0065 * a.sin(), 0.0)));
    }

    let plastic = paint(&mut mats, Color::srgb(0.32, 0.32, 0.35), 0.2, 0.5);
    let chrome = paint(&mut mats, Color::srgb(0.8, 0.8, 0.84), 1.0, 0.2);
    let black = paint(&mut mats, Color::srgb(0.03, 0.03, 0.035), 0.0, 0.8);
    let cream = paint(&mut mats, Color::srgb(0.94, 0.89, 0.77), 0.0, 0.9);
    let smoke = paint(&mut mats, Color::srgb(0.12, 0.1, 0.1), 0.0, 0.3);
    let white = paint(&mut mats, Color::srgb(0.92, 0.92, 0.92), 0.0, 0.5);
    let brown = paint(&mut mats, Color::srgb(0.25, 0.15, 0.08), 0.0, 0.6);
    let orange = paint(&mut mats, Color::srgb(1.0, 0.45, 0.1), 0.0, 0.5);

    let (hub_mesh, pack_mesh) = (meshes.add(mesh(&hub, 0.0012)), meshes.add(Cylinder::new(1.0, 0.007)));
    let at = Transform::from_translation(BOOMBOX).with_rotation(Quat::from_rotation_y(YAW));
    commands.spawn((Boombox, at, Visibility::default())).with_children(|p| {
        p.spawn((Mesh3d(meshes.add(mesh(&case, 0.006))), MeshMaterial3d(plastic.clone())));
        p.spawn((Mesh3d(meshes.add(mesh(&handle, 0.006))), MeshMaterial3d(chrome.clone())));
        p.spawn((Mesh3d(meshes.add(mesh(&antenna, 0.003))), MeshMaterial3d(chrome.clone())));
        p.spawn((Mesh3d(meshes.add(mesh(&keys, 0.004))), MeshMaterial3d(black.clone())));
        let (grille, cone) = (meshes.add(mesh(&grille, 0.0025)), meshes.add(mesh(&cone, 0.006)));
        for x in speakers {
            p.spawn((Mesh3d(cone.clone()), MeshMaterial3d(black.clone()), Transform::from_xyz(x as f32, 0.16, d as f32 - 0.004)));
            p.spawn((Mesh3d(grille.clone()), MeshMaterial3d(chrome.clone()), Transform::from_xyz(x as f32, 0.16, d as f32 - 0.004)));
        }
        p.spawn((Mesh3d(meshes.add(mesh(&label, 0.002))), MeshMaterial3d(cream), Transform::from_xyz(0.0, 0.262, d as f32 + 0.003)));
        // The power light, by the antenna.
        p.spawn((Mesh3d(meshes.add(Sphere::new(0.008))), MeshMaterial3d(orange), Transform::from_xyz(0.13, 0.262, d as f32)));
        p.spawn((Cassette, Mesh3d(meshes.add(mesh(&cassette, 0.002))), MeshMaterial3d(smoke), Transform::from_xyz(0.0, REEL_Y, REEL_Z - 0.008), Visibility::Hidden));
        for (k, x) in [-REEL_X, REEL_X].into_iter().enumerate() {
            p.spawn((Reel(k), Mesh3d(hub_mesh.clone()), MeshMaterial3d(white.clone()), Transform::from_xyz(x, REEL_Y, REEL_Z)))
                .with_children(|r| {
                    r.spawn((Pack(k), Mesh3d(pack_mesh.clone()), MeshMaterial3d(brown.clone()), Transform::from_xyz(0.0, 0.0, -0.002).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), Visibility::Hidden));
                });
        }
    });
}

/// Drops, loading, the end of a tape, and the reels.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    time: Res<Time>,
    mut deck: ResMut<Deck>,
    mut drops: MessageReader<FileDragAndDrop>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut sinks: Query<&mut AudioSink, With<TapeSound>>,
    mut boombox: Query<&mut Transform, (With<Boombox>, Without<Reel>, Without<Pack>)>,
    mut reels: Query<(&Reel, &mut Transform), Without<Pack>>,
    mut packs: Query<(&Pack, &mut Transform, &mut Visibility), Without<Cassette>>,
    mut cassette: Query<&mut Visibility, (With<Cassette>, Without<Pack>)>,
) {
    let deck = &mut *deck;
    let dt = time.delta_secs();
    for drop in drops.read() {
        match drop {
            FileDragAndDrop::HoveredFile { .. } => deck.hover = deck.on,
            FileDragAndDrop::HoveredFileCanceled { .. } => deck.hover = false,
            FileDragAndDrop::DroppedFile { path_buf, .. } => {
                deck.hover = false;
                if deck.on {
                    let new = deck.add(path_buf, &mut commands);
                    if new > 1 {
                        deck.note = Some((format!("{new} tapes in the deck"), 4.0));
                    }
                }
            }
        }
    }
    if let Some((_, left)) = &mut deck.note {
        *left -= dt;
        if *left <= 0.0 {
            deck.note = None;
        }
    }

    // A tape read on the worker goes in (clunk), or gets a note and is skipped.
    let done = deck.loading.as_ref().and_then(|s| s.lock().ok().and_then(|mut s| s.take()));
    if let Some(r) = done {
        deck.loading = None;
        match r {
            Ok((bytes, length)) => {
                let src = sources.add(AudioSource { bytes });
                let settings = PlaybackSettings { volume: deck.gain(), paused: deck.paused, ..PlaybackSettings::ONCE };
                deck.current = Some(commands.spawn((TapeSound, AudioPlayer::new(src), settings)).id());
                deck.length = length;
                deck.clunk = 0.0;
                commands.spawn((AudioPlayer::new(deck.clunk_sound.clone()), PlaybackSettings { volume: deck.gain(), ..PlaybackSettings::DESPAWN }));
            }
            Err(e) => {
                warn!("tape: {e}");
                deck.note = Some((e, 6.0));
                deck.tapes.remove(deck.at);
                if !deck.tapes.is_empty() {
                    let k = deck.at % deck.tapes.len();
                    deck.play(k, &mut commands);
                }
            }
        }
    }

    // Keep the sink on the deck's volume and pause; at the end, the next tape.
    let mut ended = false;
    let mut pos = 0.0;
    if let Some(e) = deck.current
        && let Ok(mut sink) = sinks.get_mut(e)
    {
        if (sink.volume().to_linear() - deck.gain().to_linear()).abs() > 1e-4 {
            sink.set_volume(deck.gain());
        }
        if deck.paused != sink.is_paused() {
            if deck.paused { sink.pause() } else { sink.play() }
        }
        pos = sink.position().as_secs_f32();
        ended = sink.empty() && !deck.paused;
    }
    if ended {
        deck.next(&mut commands);
    }

    // The clunk: a little jolt.
    deck.clunk = (deck.clunk + dt).min(10.0);
    if let Ok(mut tf) = boombox.single_mut() {
        let c = deck.clunk;
        tf.translation.y = BOOMBOX.y + if c < 0.35 { 0.012 * (c * 40.0).sin().abs() * (1.0 - c / 0.35) } else { 0.0 };
    }
    // Reels turn while it plays; tape winds from the left reel to the right.
    let playing = deck.current.is_some() && !deck.paused;
    let frac = match deck.length {
        Some(l) if l.as_secs_f32() > 0.0 => (pos / l.as_secs_f32()).clamp(0.0, 1.0),
        _ => (pos / UNKNOWN).fract(),
    };
    let r = |f: f32| (PACK.0 * PACK.0 + (PACK.1 * PACK.1 - PACK.0 * PACK.0) * f).sqrt();
    let radius = [r(1.0 - frac), r(frac)];
    for (reel, mut tf) in &mut reels {
        if playing {
            // The same tape speed at both reels: a small reel turns faster.
            tf.rotate_local_z(-SPIN * PACK.1 / radius[reel.0] * 0.5 * dt);
        }
    }
    let loaded = deck.current.is_some();
    for (pack, mut tf, mut vis) in &mut packs {
        tf.scale = Vec3::new(radius[pack.0], 1.0, radius[pack.0]);
        *vis = if loaded { Visibility::Inherited } else { Visibility::Hidden };
    }
    for mut vis in &mut cassette {
        *vis = if loaded { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// The label under the boombox (the tape's name, or BYO TAPE) and, when
/// clicked, the controls. M and N work while the deck is on.
pub fn ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut deck: ResMut<Deck>,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCam>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let deck = &mut *deck;
    if deck.on && !ctx.egui_wants_keyboard_input() {
        if keys.just_pressed(KeyCode::KeyM) {
            deck.toggle(&mut commands);
        }
        if keys.just_pressed(KeyCode::KeyN) {
            deck.next(&mut commands);
        }
    }
    let (cam, cam_tf) = *camera;
    let foot = BOOMBOX + Quat::from_rotation_y(YAW) * Vec3::new(0.0, 0.0, 0.09);
    let Ok(p) = cam.world_to_viewport(cam_tf, foot) else {
        return Ok(());
    };
    // Clear of the board-cam inset (bottom left): it draws over egui.
    let (x, _, w, _) = super::scene::board_cam_rect(&window);
    let clear = x + w + 8.0;
    egui::Area::new("tape_deck".into()).fixed_pos(egui::pos2((p.x - 70.0).max(clear), p.y + 8.0)).pivot(egui::Align2::LEFT_TOP).show(ctx, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        if !deck.on {
            // Off: just a prop, with a dim switch.
            let b = ui.add(egui::Button::new(egui::RichText::new("tape deck: off").size(11.0).color(egui::Color32::from_white_alpha(90))).fill(egui::Color32::from_black_alpha(90)));
            if b.on_hover_text("Switch the boombox on (it still won't play until you give it a tape).").clicked() {
                deck.switch(true, &mut commands);
            }
            return;
        }
        let text = if deck.hover {
            "drop it in the deck".to_string()
        } else {
            match deck.title() {
                Some(t) if deck.loaded() => format!("{} {t}", if deck.paused { "||" } else { ">" }),
                Some(_) => format!("{} TAPE{} - press play", deck.tapes.len(), if deck.tapes.len() == 1 { "" } else { "S" }),
                None => "BYO TAPE".into(),
            }
        };
        let label = egui::Button::new(egui::RichText::new(text).size(13.0).strong().color(INK)).fill(TAPE_CREAM).stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 100, 70)));
        if ui.add(label).on_hover_text("Click for the tape deck").clicked() {
            deck.open = !deck.open;
        }
        if let Some((note, _)) = &deck.note {
            ui.label(egui::RichText::new(note).size(11.5).color(egui::Color32::from_rgb(255, 200, 120)));
        }
    });
    if !deck.on || !deck.open {
        return Ok(());
    }
    // The controls open upward, over the boombox.
    let Ok(top) = cam.world_to_viewport(cam_tf, BOOMBOX + Vec3::new(0.0, 0.5, 0.0)) else {
        return Ok(());
    };
    egui::Area::new("tape_controls".into()).fixed_pos(egui::pos2(top.x.max(clear + 145.0), top.y)).pivot(egui::Align2::CENTER_BOTTOM).show(ctx, |ui| {
        egui::Frame::new().fill(egui::Color32::from_black_alpha(215)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_max_width(270.0);
            if deck.tapes.is_empty() {
                ui.label(egui::RichText::new("Bring your own tape").strong().color(TAPE_CREAM));
                ui.small("Drop MP3, Ogg, FLAC or WAV files (or a folder) on the window, or put them in the music folder next to the game. Nothing plays until you do.");
            } else {
                ui.horizontal(|ui| {
                    let playing = deck.loaded() && !deck.paused;
                    if ui.button(if playing { "pause" } else { "play" }).on_hover_text("M").clicked() {
                        deck.toggle(&mut commands);
                    }
                    if ui.button("next").on_hover_text("N").clicked() {
                        deck.next(&mut commands);
                    }
                    if ui.checkbox(&mut deck.shuffle, "shuffle").changed() {
                        deck.save();
                    }
                });
                ui.horizontal(|ui| {
                    ui.small("volume");
                    let r = ui.add(egui::Slider::new(&mut deck.volume, 0.0..=1.0).show_value(false));
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        deck.save();
                    }
                });
                ui.small(format!("{} tape{} in music/. Drop more on the window. M pause, N next.", deck.tapes.len(), if deck.tapes.len() == 1 { "" } else { "s" }));
            }
            if ui.small_button("switch the deck off").on_hover_text("Playing music from another app? Off, the boombox is just a prop (no hotkeys).").clicked() {
                deck.switch(false, &mut commands);
            }
        });
    });
    Ok(())
}

pub fn shots_enabled() -> bool {
    std::env::var_os("UPD_TAPE").is_some()
}

/// `UPD_TAPE=empty|<file or folder>`: shoot the deck (`shots/tape_*.png`) and
/// print where the tape is, then exit. A path goes in as if dropped (nothing
/// is copied), then N twice (a bad file gets skipped with a note), then off.
pub fn shots(
    mut commands: Commands,
    mut deck: ResMut<Deck>,
    sinks: Query<&AudioSink, With<TapeSound>>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    *frame += 1;
    let f = *frame;
    let arg = std::env::var("UPD_TAPE").unwrap_or_default();
    let shot = |commands: &mut Commands, name: &str| {
        std::fs::create_dir_all("shots").ok();
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/{name}.png")));
    };
    let pos = |deck: &Deck| deck.current.and_then(|e| sinks.get(e).ok()).map(|s| (s.position().as_secs_f32(), s.is_paused(), s.empty()));
    match f {
        30 if arg == "empty" => deck.open = true,
        30 => {
            deck.add(Path::new(&arg), &mut commands);
            deck.open = true;
        }
        90 if arg == "empty" => shot(&mut commands, "tape_0_empty"),
        240 if arg != "empty" => {
            println!("tape: {:?} at {:?} (pos s, paused, empty), length {:?}, {} tapes", deck.title(), pos(&deck), deck.length, deck.tapes.len());
            shot(&mut commands, "tape_1_playing");
        }
        250 | 320 if arg != "empty" => deck.next(&mut commands),
        310 | 380 if arg != "empty" => println!("tape: {:?} at {:?} after N, {} tapes, note {:?}", deck.title(), pos(&deck), deck.tapes.len(), deck.note),
        385 if arg != "empty" => shot(&mut commands, "tape_2_next"),
        400 if arg != "empty" => deck.switch(false, &mut commands),
        410 if arg != "empty" => shot(&mut commands, "tape_3_off"),
        130 if arg == "empty" => {
            exit.write(AppExit::Success);
        }
        440 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clunk_decodes_and_junk_does_not() {
        let dir = std::env::temp_dir().join("upd_tape_test");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("clunk.WAV"), clunk_wav()).unwrap();
        std::fs::write(dir.join("sub").join("junk.mp3"), b"not audio").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();
        // A folder: audio files at any depth, any case, nothing else.
        let files = audio_files(&dir);
        assert_eq!(files.len(), 2, "{files:?}");
        let (_, len) = read(&dir.join("clunk.WAV")).expect("our clunk decodes");
        assert!(len.is_some_and(|d| (d.as_secs_f32() - 0.14).abs() < 0.01), "{len:?}");
        let err = read(&dir.join("sub").join("junk.mp3")).unwrap_err();
        assert!(err.starts_with("can't play junk"), "{err}");
        assert_eq!(title(Path::new("music/My_Song_Name.flac")), "My Song Name");
        std::fs::remove_dir_all(&dir).ok();
    }
}
