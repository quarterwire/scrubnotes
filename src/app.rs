use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::glow::{self, HasContext};
use egui::{
    Align, Align2, Color32, FontId, Key, Layout, Margin, Modifiers, Rect, RichText, Sense, Shape, Stroke,
    ViewportCommand, pos2, vec2,
};

use crate::mpv::{Event, Mpv, Prop, Value};
use crate::notes::{self, Category, ExportFormat, ExportOptions, Note, Review};
use crate::timecode::{self, TcStyle};

pub const APP_TITLE: &str = concat!("ScrubNotes ", env!("CARGO_PKG_VERSION"));

const ACCENT: Color32 = Color32::from_rgb(0x2d, 0x8c, 0xeb);
const PANEL: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
const PANEL_DARK: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
const TEXT_DIM: Color32 = Color32::from_gray(150);
const SHUTTLE_SPEEDS: [f64; 4] = [1.0, 2.0, 4.0, 8.0];
const AUTOSAVE_DELAY: Duration = Duration::from_millis(1200);
const SCRUB_SETTLE: Duration = Duration::from_millis(110);
const UNDO_LIMIT: usize = 200;
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "webm", "avi", "flv", "ts", "m2ts", "mts", "wmv", "m4v", "mpg", "mpeg", "3gp", "ogv",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    TogglePlay,
    Shuttle(i32),
    Stop,
    Slow(i32),
    Step(i32, bool),
    StepMany(i32),
    PrevNote,
    NextNote,
    GoStart,
    GoEnd,
    MarkIn,
    MarkOut,
    GoIn,
    GoOut,
    ClearIn,
    ClearOut,
    ClearInOut,
    AddMarker,
    AddRangeNote,
    ShowExport,
    ExportFrame,
    DeleteNote,
    Undo,
    Redo,
    Open,
    OpenUrl,
    Save,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ToggleNotes,
    ToggleFullscreen,
    Escape,
    Help,
    SetCategory(usize),
    ToggleMute,
}

/// Premiere Pro default keyboard layout, with markers standing in for edit points.
fn action_for(key: Key, m: Modifiers, repeat: bool, k_held: bool) -> Option<Action> {
    use Action::*;
    let ctrl = m.ctrl || m.command;
    Some(match (key, ctrl, m.shift, m.alt) {
        (Key::Space, false, false, false) => TogglePlay,
        (Key::L, false, false, false) if k_held => Step(1, repeat),
        (Key::J, false, false, false) if k_held => Step(-1, repeat),
        (Key::L, false, false, false) => Shuttle(1),
        (Key::J, false, false, false) => Shuttle(-1),
        (Key::K, false, false, false) if repeat => return None,
        (Key::K, false, false, false) => Stop,
        (Key::L, false, true, false) => Slow(1),
        (Key::J, false, true, false) => Slow(-1),
        (Key::ArrowRight, false, false, false) => Step(1, repeat),
        (Key::ArrowLeft, false, false, false) => Step(-1, repeat),
        (Key::ArrowRight, false, true, false) => StepMany(5),
        (Key::ArrowLeft, false, true, false) => StepMany(-5),
        (Key::ArrowUp, false, false, false) | (Key::M, true, true, false) => PrevNote,
        (Key::ArrowDown, false, false, false) | (Key::M, false, true, false) => NextNote,
        (Key::Home, false, false, false) => GoStart,
        (Key::End, false, false, false) => GoEnd,
        (Key::I, false, false, false) => MarkIn,
        (Key::O, false, false, false) => MarkOut,
        (Key::I, false, true, false) => GoIn,
        (Key::O, false, true, false) => GoOut,
        (Key::I, true, true, false) => ClearIn,
        (Key::O, true, true, false) => ClearOut,
        (Key::X, true, true, false) => ClearInOut,
        (Key::M, false, false, false) => AddMarker,
        (Key::M, false, false, true) => AddRangeNote,
        (Key::M, true, false, false) => ShowExport,
        (Key::E, true, true, false) => ExportFrame,
        (Key::Delete | Key::Backspace, false, false, false) => DeleteNote,
        (Key::Z, true, false, false) => Undo,
        (Key::Z, true, true, false) | (Key::Y, true, false, false) => Redo,
        (Key::O, true, false, false) => Open,
        (Key::U, true, false, false) => OpenUrl,
        (Key::S, true, false, false) => Save,
        (Key::Equals | Key::Plus, false, _, false) => ZoomIn,
        (Key::Minus, false, false, false) => ZoomOut,
        (Key::Backslash, false, false, false) => ZoomFit,
        (Key::Backtick, false, false, false) => ToggleNotes,
        (Key::F11, false, false, false) => ToggleFullscreen,
        (Key::Escape, false, false, false) => Escape,
        (Key::F1, false, false, false) => Help,
        (Key::Num0, false, false, false) => ToggleMute,
        (Key::Num1, true, false, false) => SetCategory(0),
        (Key::Num2, true, false, false) => SetCategory(1),
        (Key::Num3, true, false, false) => SetCategory(2),
        (Key::Num4, true, false, false) => SetCategory(3),
        (Key::Num5, true, false, false) => SetCategory(4),
        (Key::Num6, true, false, false) => SetCategory(5),
        _ => return None,
    })
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Space", "Play / stop"),
    ("J / K / L", "Shuttle reverse / stop / forward (press again for 2x, 4x, 8x)"),
    ("Shift+J / Shift+L", "Slow-motion reverse / forward (0.25x steps)"),
    ("Hold K + J / L", "Step one frame back / forward"),
    ("Left / Right", "Step one frame back / forward"),
    ("Shift+Left / Right", "Step five frames"),
    ("Up / Down", "Previous / next note"),
    ("Ctrl+Shift+M / Shift+M", "Previous / next note"),
    ("Home / End", "Go to start / end"),
    ("I / O", "Mark In / Mark Out"),
    ("Shift+I / Shift+O", "Go to In / Out"),
    ("Ctrl+Shift+I / O / X", "Clear In / Out / both"),
    ("M", "Add note at playhead (type, then Enter)"),
    ("Alt+M", "Add range note from In to Out"),
    ("Ctrl+1 … Ctrl+6", "Set selected note's category"),
    ("Delete", "Delete selected note"),
    ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo note changes"),
    ("Ctrl+M", "Export notes"),
    ("Ctrl+Shift+E", "Export frame (PNG next to the video)"),
    ("= / - / \\", "Zoom timeline in / out / fit"),
    ("Mouse wheel on timeline", "Zoom around cursor (Shift+wheel pans)"),
    ("`", "Show / hide notes panel"),
    ("F11 / double-click video", "Fullscreen"),
    ("0", "Mute"),
    ("Ctrl+O / Ctrl+U", "Open video file / URL"),
    ("Ctrl+S", "Save notes (also auto-saved)"),
];

enum Source {
    File(PathBuf),
    Url(String),
}

impl Source {
    fn notes_path(&self) -> PathBuf {
        match self {
            Source::File(p) => notes::sidecar_path(p),
            Source::Url(u) => notes::url_notes_path(u),
        }
    }

    fn display_name(&self) -> String {
        match self {
            Source::File(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into()),
            Source::Url(u) => u.clone(),
        }
    }

    fn stem(&self) -> String {
        match self {
            Source::File(p) => p.file_stem().map_or_else(|| "frame".into(), |n| n.to_string_lossy().into()),
            Source::Url(_) => "stream".into(),
        }
    }

    fn mpv_target(&self) -> String {
        match self {
            Source::File(p) => p.to_string_lossy().into(),
            Source::Url(u) => u.clone(),
        }
    }

    fn frames_dir(&self) -> PathBuf {
        match self {
            Source::File(p) => p.parent().map(PathBuf::from).unwrap_or_default(),
            Source::Url(_) => notes::data_dir().join("frames"),
        }
    }
}

struct Playback {
    pos: f64,
    duration: f64,
    paused: bool,
    speed: f64,
    fps: f64,
    estimated_fps: f64,
    volume: f64,
    mute: bool,
    hwdec: String,
    codec: String,
    width: i64,
    height: i64,
    backward: bool,
    seeking: bool,
    loaded: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            pos: 0.0,
            duration: 0.0,
            paused: true,
            speed: 1.0,
            fps: 0.0,
            estimated_fps: 0.0,
            volume: 100.0,
            mute: false,
            hwdec: String::new(),
            codec: String::new(),
            width: 0,
            height: 0,
            backward: false,
            seeking: false,
            loaded: false,
        }
    }
}

struct Scrub {
    was_paused: bool,
    target: f64,
    last_move: Instant,
    exact_sent: bool,
}

#[derive(Default)]
struct Timeline {
    zoom: f64,
    start: f64,
    last_pos: f64,
    marker_press: bool,
}

/// Offscreen render target mpv draws into, displayed by egui as a texture.
struct Surface {
    texture: glow::Texture,
    fbo: glow::Framebuffer,
    size: (i32, i32),
    id: egui::TextureId,
}

#[derive(Clone, Copy)]
struct View {
    start: f64,
    len: f64,
    left: f32,
    width: f32,
}

impl View {
    fn x(&self, t: f64) -> f32 {
        self.left + ((t - self.start) / self.len) as f32 * self.width
    }

    fn t(&self, x: f32) -> f64 {
        self.start + ((x - self.left) / self.width) as f64 * self.len
    }
}

enum NoteEdit {
    Category(u64, Category),
    Start(u64, f64),
    End(u64, Option<f64>),
}

pub struct App {
    mpv: Option<Mpv>,
    mpv_error: Option<String>,
    surface: Option<Surface>,
    pb: Playback,
    source: Option<Source>,
    pending_open: Option<PathBuf>,

    review: Review,
    selected: Option<u64>,
    scroll_to_selected: bool,
    focus_editor: bool,
    dirty: bool,
    last_edit: Instant,
    undo: Vec<Vec<Note>>,
    redo: Vec<Vec<Note>>,
    last_category: Category,

    mark_in: Option<f64>,
    mark_out: Option<f64>,
    shuttle: i32,
    slow: f64,
    tl: Timeline,
    scrub: Option<Scrub>,

    exact_scrub: bool,
    pause_on_note: bool,
    show_notes: bool,
    filter: String,
    category_filter: Option<Category>,
    show_export: bool,
    export: ExportOptions,
    export_in_out_only: bool,
    show_help: bool,
    show_open_url: bool,
    url_input: String,
    status: Option<(String, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let (mpv, mpv_error) = match Mpv::new(cc.egui_ctx.clone()) {
            Ok(mut mpv) => match mpv.init_render() {
                Ok(()) => (Some(mpv), None),
                Err(e) => (None, Some(e)),
            },
            Err(e) => (None, Some(e)),
        };
        Self {
            mpv,
            mpv_error,
            surface: None,
            pb: Playback::default(),
            source: None,
            pending_open: initial,
            review: Review::default(),
            selected: None,
            scroll_to_selected: false,
            focus_editor: false,
            dirty: false,
            last_edit: Instant::now(),
            undo: Vec::new(),
            redo: Vec::new(),
            last_category: Category::Note,
            mark_in: None,
            mark_out: None,
            shuttle: 0,
            slow: 0.0,
            tl: Timeline { zoom: 1.0, ..Default::default() },
            scrub: None,
            exact_scrub: false,
            pause_on_note: true,
            show_notes: true,
            filter: String::new(),
            category_filter: None,
            show_export: false,
            export: ExportOptions::default(),
            export_in_out_only: false,
            show_help: false,
            show_open_url: false,
            url_input: String::new(),
            status: None,
        }
    }

    // ---------------------------------------------------------------- playback

    fn fps(&self) -> f64 {
        if self.pb.fps > 1.0 {
            self.pb.fps
        } else if self.pb.estimated_fps > 1.0 {
            self.pb.estimated_fps
        } else {
            30.0
        }
    }

    fn display_pos(&self) -> f64 {
        match &self.scrub {
            Some(s) if s.target.is_finite() => s.target,
            _ => self.pb.pos,
        }
    }

    fn cmd(&self, args: &[&str]) {
        if let Some(mpv) = &self.mpv {
            mpv.command(args);
        }
    }

    fn set(&self, property: &str, value: &str) {
        if let Some(mpv) = &self.mpv {
            mpv.set(property, value);
        }
    }

    fn seek(&mut self, t: f64, exact: bool) {
        if !self.pb.loaded {
            return;
        }
        let t = t.clamp(0.0, self.pb.duration.max(0.0));
        let mode = if exact { "absolute+exact" } else { "absolute+keyframes" };
        self.cmd(&["seek", &format!("{t:.6}"), mode]);
        self.pb.pos = t;
    }

    fn set_backward(&mut self, backward: bool) {
        if self.pb.backward != backward {
            self.set("play-dir", if backward { "backward" } else { "forward" });
            self.pb.backward = backward;
        }
    }

    fn play(&mut self, backward: bool, speed: f64) {
        self.set_backward(backward);
        self.set("speed", &format!("{speed}"));
        self.set("pause", "no");
        self.pb.speed = speed;
        self.pb.paused = false;
    }

    fn stop(&mut self) {
        self.set("pause", "yes");
        self.pb.paused = true;
        self.shuttle = 0;
        self.slow = 0.0;
        if self.pb.speed != 1.0 {
            self.set("speed", "1");
            self.pb.speed = 1.0;
        }
        self.set_backward(false);
    }

    fn toggle_play(&mut self) {
        if !self.pb.loaded {
            return;
        }
        if self.pb.paused {
            if self.pb.duration > 0.0 && self.pb.pos >= self.pb.duration - 1.5 / self.fps() {
                self.seek(0.0, true);
            }
            self.shuttle = 1;
            self.slow = 0.0;
            self.play(false, 1.0);
        } else {
            self.stop();
        }
    }

    fn shuttle(&mut self, dir: i32) {
        if !self.pb.loaded {
            return;
        }
        let level = if !self.pb.paused && self.shuttle.signum() == dir { (self.shuttle.abs() + 1).min(4) } else { 1 };
        self.shuttle = level * dir;
        self.slow = 0.0;
        self.play(dir < 0, SHUTTLE_SPEEDS[level as usize - 1]);
    }

    fn slow(&mut self, dir: i32) {
        if !self.pb.loaded {
            return;
        }
        let same = !self.pb.paused && self.slow != 0.0 && self.slow.signum() == dir as f64;
        let speed = if same && self.slow.abs() < 0.75 { self.slow.abs() + 0.25 } else { 0.25 };
        self.slow = speed * dir as f64;
        self.shuttle = 0;
        self.play(dir < 0, speed);
    }

    fn step(&mut self, frames: i32, repeat: bool) {
        if !self.pb.loaded || (repeat && self.pb.seeking) {
            return;
        }
        if self.pb.paused {
            self.set_backward(false);
        } else {
            self.stop();
        }
        self.cmd(&[if frames > 0 { "frame-step" } else { "frame-back-step" }]);
    }

    fn step_many(&mut self, frames: i32) {
        if !self.pb.loaded {
            return;
        }
        if !self.pb.paused {
            self.stop();
        }
        let delta = frames as f64 / self.fps();
        self.cmd(&["seek", &format!("{delta:.6}"), "relative+exact"]);
    }

    fn goto_note(&mut self, forward: bool) {
        let eps = 0.5 / self.fps();
        let pos = self.pb.pos;
        let target = if forward {
            self.review.notes.iter().filter(|n| n.start > pos + eps).min_by(|a, b| a.start.total_cmp(&b.start))
        } else {
            self.review.notes.iter().filter(|n| n.start < pos - eps).max_by(|a, b| a.start.total_cmp(&b.start))
        };
        if let Some(note) = target {
            let (id, start) = (note.id, note.start);
            self.selected = Some(id);
            self.scroll_to_selected = true;
            self.seek(start, true);
        }
    }

    fn zoom_by(&mut self, factor: f64, anchor: Option<f64>) {
        let dur = self.pb.duration;
        if dur <= 0.0 {
            return;
        }
        let old_len = dur / self.tl.zoom;
        let anchor = anchor.unwrap_or_else(|| self.display_pos());
        let mut frac = (anchor - self.tl.start) / old_len;
        if !(0.0..=1.0).contains(&frac) {
            frac = 0.5;
        }
        self.tl.zoom = (self.tl.zoom * factor).clamp(1.0, self.max_zoom());
        let new_len = dur / self.tl.zoom;
        self.tl.start = (anchor - frac * new_len).clamp(0.0, (dur - new_len).max(0.0));
    }

    /// Deepest zoom shows about 30 frames across the timeline.
    fn max_zoom(&self) -> f64 {
        (self.pb.duration / (30.0 / self.fps())).max(1.0)
    }

    fn poll_mpv(&mut self, ctx: &egui::Context) {
        let Some(mpv) = &self.mpv else { return };
        for event in mpv.poll_events() {
            match event {
                Event::Property(prop, value) => self.apply_property(prop, value),
                Event::FileLoaded => {
                    self.pb.loaded = true;
                    self.tl = Timeline { zoom: 1.0, ..Default::default() };
                }
                Event::EndFile { error } => {
                    self.pb.loaded = false;
                    if let Some(err) = error {
                        let hint = match &self.source {
                            Some(Source::Url(_)) => " (streaming URLs need yt-dlp on PATH)",
                            _ => "",
                        };
                        self.set_status(format!("Could not play: {err}{hint}"));
                    }
                }
                Event::CommandFailed(err) => eprintln!("mpv command failed: {err}"),
                Event::Shutdown => {
                    self.mpv_error = Some("The player core shut down.".into());
                }
            }
        }
        if matches!(self.mpv_error.as_deref(), Some("The player core shut down.")) {
            if let Some(mut mpv) = self.mpv.take() {
                mpv.free_render();
            }
            self.surface = None;
            ctx.request_repaint();
        }
    }

    fn apply_property(&mut self, prop: Prop, value: Value) {
        match (prop, value) {
            (Prop::TimePos, Value::Double(t)) => self.pb.pos = t,
            (Prop::Duration, Value::Double(d)) => self.pb.duration = d,
            (Prop::Duration, Value::None) => self.pb.duration = 0.0,
            (Prop::Pause, Value::Flag(paused)) => {
                self.pb.paused = paused;
                if paused {
                    self.shuttle = 0;
                    self.slow = 0.0;
                }
            }
            (Prop::Speed, Value::Double(s)) => self.pb.speed = s,
            (Prop::ContainerFps, Value::Double(f)) => self.pb.fps = f,
            (Prop::ContainerFps, Value::None) => self.pb.fps = 0.0,
            (Prop::EstimatedFps, Value::Double(f)) => self.pb.estimated_fps = f,
            (Prop::Volume, Value::Double(v)) => self.pb.volume = v,
            (Prop::Mute, Value::Flag(m)) => self.pb.mute = m,
            (Prop::Hwdec, Value::Str(s)) => self.pb.hwdec = s,
            (Prop::Hwdec, Value::None) => self.pb.hwdec.clear(),
            (Prop::Codec, Value::Str(s)) => self.pb.codec = s,
            (Prop::Codec, Value::None) => self.pb.codec.clear(),
            (Prop::Width, Value::Int(w)) => self.pb.width = w,
            (Prop::Height, Value::Int(h)) => self.pb.height = h,
            (Prop::PlayDir, Value::Str(s)) => self.pb.backward = s == "backward",
            (Prop::Seeking, Value::Flag(s)) => self.pb.seeking = s,
            _ => {}
        }
    }

    // ---------------------------------------------------------------- files

    fn open(&mut self, ctx: &egui::Context, source: Source) {
        if self.mpv.is_none() {
            self.set_status("Can't open: libmpv is not loaded".into());
            return;
        }
        self.save_notes(false);
        let path = source.notes_path();
        self.review = match Review::load(&path) {
            Ok(review) => review,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Review::default(),
            Err(e) => {
                let backup = path.with_extension("json.bak");
                let _ = std::fs::rename(&path, &backup);
                self.set_status(format!("Notes file was unreadable ({e}); moved to {}", backup.display()));
                Review::default()
            }
        };
        if let Source::Url(url) = &source
            && self.review.source_url.is_empty()
        {
            self.review.source_url = url.clone();
        }
        self.review.video = source.display_name();
        self.selected = None;
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.mark_in = None;
        self.mark_out = None;
        self.shuttle = 0;
        self.slow = 0.0;
        self.scrub = None;
        self.pb = Playback { volume: self.pb.volume, mute: self.pb.mute, ..Default::default() };
        self.set("pause", "yes");
        self.cmd(&["loadfile", &source.mpv_target()]);
        ctx.send_viewport_cmd(ViewportCommand::Title(format!("{} — {APP_TITLE}", source.display_name())));
        if !self.review.notes.is_empty() {
            self.set_status(format!("Loaded {} notes from {}", self.review.notes.len(), path.display()));
        }
        self.source = Some(source);
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title("Open video")
            .add_filter("Video", VIDEO_EXTENSIONS)
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = picked {
            self.open(ctx, Source::File(path));
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_edit = Instant::now();
    }

    fn save_notes(&mut self, announce: bool) {
        let Some(source) = &self.source else { return };
        if !self.dirty && !announce {
            return;
        }
        if self.pb.fps > 1.0 {
            self.review.fps = self.pb.fps;
        }
        let path = source.notes_path();
        match self.review.save(&path) {
            Ok(()) => {
                self.dirty = false;
                if announce {
                    self.set_status(format!("Notes saved to {}", path.display()));
                }
            }
            Err(e) => self.set_status(format!("Could not save notes to {}: {e}", path.display())),
        }
    }

    fn export_frame(&mut self) {
        let Some(source) = &self.source else { return };
        if !self.pb.loaded {
            return;
        }
        let dir = source.frames_dir();
        let _ = std::fs::create_dir_all(&dir);
        let tc = timecode::frames(self.pb.pos, self.fps()).replace(':', "-");
        let file = dir.join(format!("{}_{tc}.png", source.stem()));
        self.cmd(&["screenshot-to-file", &file.to_string_lossy(), "video"]);
        self.set_status(format!("Frame exported to {}", file.display()));
    }

    fn set_status(&mut self, msg: String) {
        self.status = Some((msg, Instant::now()));
    }

    // ---------------------------------------------------------------- notes

    fn snapshot(&mut self) {
        if self.undo.last() == Some(&self.review.notes) {
            return;
        }
        self.undo.push(self.review.notes.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn add_note(&mut self, start: f64, end: Option<f64>) {
        if self.source.is_none() || !self.pb.loaded {
            self.set_status("Open a video first (Ctrl+O)".into());
            return;
        }
        self.snapshot();
        let id = self.review.next_id();
        self.review.notes.push(Note { id, start, end, category: self.last_category, text: String::new() });
        self.review.sort();
        self.selected = Some(id);
        self.focus_editor = true;
        self.scroll_to_selected = true;
        self.show_notes = true;
        self.mark_dirty();
        if self.pause_on_note && !self.pb.paused {
            self.stop();
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected.take() else { return };
        self.snapshot();
        self.review.notes.retain(|n| n.id != id);
        self.mark_dirty();
        self.set_status("Note deleted (Ctrl+Z to undo)".into());
    }

    fn undo(&mut self, redo: bool) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(notes) = from.pop() {
            to.push(std::mem::replace(&mut self.review.notes, notes));
            if self.selected.is_some_and(|id| !self.review.notes.iter().any(|n| n.id == id)) {
                self.selected = None;
            }
            self.mark_dirty();
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        use Action::*;
        let pos = self.pb.pos;
        match action {
            TogglePlay => self.toggle_play(),
            Shuttle(dir) => self.shuttle(dir),
            Stop => self.stop(),
            Slow(dir) => self.slow(dir),
            Step(n, repeat) => self.step(n, repeat),
            StepMany(n) => self.step_many(n),
            PrevNote => self.goto_note(false),
            NextNote => self.goto_note(true),
            GoStart => self.seek(0.0, true),
            GoEnd => {
                if self.pb.loaded {
                    self.cmd(&["seek", "100", "absolute-percent+exact"]);
                }
            }
            MarkIn => {
                self.mark_in = Some(pos);
                if self.mark_out.is_some_and(|o| o <= pos) {
                    self.mark_out = None;
                }
            }
            MarkOut => {
                self.mark_out = Some(pos);
                if self.mark_in.is_some_and(|i| i >= pos) {
                    self.mark_in = None;
                }
            }
            GoIn => {
                if let Some(t) = self.mark_in {
                    self.seek(t, true);
                }
            }
            GoOut => {
                if let Some(t) = self.mark_out {
                    self.seek(t, true);
                }
            }
            ClearIn => self.mark_in = None,
            ClearOut => self.mark_out = None,
            ClearInOut => {
                self.mark_in = None;
                self.mark_out = None;
            }
            AddMarker => self.add_note(pos, None),
            AddRangeNote => match (self.mark_in, self.mark_out) {
                (Some(a), Some(b)) => self.add_note(a, Some(b)),
                (Some(a), None) if pos > a => self.add_note(a, Some(pos)),
                _ => self.set_status("Set In (I) and Out (O) first, then Alt+M".into()),
            },
            ShowExport => self.show_export = true,
            ExportFrame => self.export_frame(),
            DeleteNote => self.delete_selected(),
            Undo => self.undo(false),
            Redo => self.undo(true),
            Open => self.open_dialog(ctx),
            OpenUrl => self.show_open_url = true,
            Save => self.save_notes(true),
            ZoomIn => self.zoom_by(2.0, None),
            ZoomOut => self.zoom_by(0.5, None),
            ZoomFit => {
                self.tl.zoom = 1.0;
                self.tl.start = 0.0;
            }
            ToggleNotes => self.show_notes = !self.show_notes,
            ToggleFullscreen => {
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
            Escape => {
                if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                } else {
                    self.selected = None;
                }
            }
            Help => self.show_help = !self.show_help,
            SetCategory(i) => {
                let category = Category::ALL[i];
                self.last_category = category;
                if let Some(id) = self.selected {
                    self.snapshot();
                    if let Some(note) = self.review.notes.iter_mut().find(|n| n.id == id) {
                        note.category = category;
                    }
                    self.mark_dirty();
                }
            }
            ToggleMute => self.set("mute", if self.pb.mute { "no" } else { "yes" }),
        }
    }

    /// Pulls shortcut key presses out of the input queue before any widget sees them,
    /// so Space/arrows never activate a focused button.
    fn take_shortcuts(&self, ctx: &egui::Context) -> Vec<Action> {
        if ctx.text_edit_focused() {
            return Vec::new();
        }
        let mut actions = Vec::new();
        ctx.input_mut(|i| {
            let k_held = i.key_down(Key::K);
            i.events.retain(|e| {
                if let egui::Event::Key { key, pressed: true, modifiers, repeat, .. } = e
                    && let Some(action) = action_for(*key, *modifiers, *repeat, k_held)
                {
                    actions.push(action);
                    return false;
                }
                true
            });
            if !actions.is_empty() {
                i.events.retain(|e| !matches!(e, egui::Event::Text(_)));
            }
        });
        actions
    }

    // ---------------------------------------------------------------- video

    fn draw_video(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let rect = ui.max_rect();
        let response = ui.interact(rect, egui::Id::unique("video_surface"), Sense::click());
        let painter = ui.painter_at(rect);

        if let Some(err) = &self.mpv_error {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                err,
                FontId::proportional(14.0),
                Color32::from_rgb(0xff, 0x80, 0x80),
            );
            return;
        }

        if let (Some(mpv), Some(gl)) = (&self.mpv, frame.gl().cloned()) {
            let ppp = ui.ctx().pixels_per_point();
            let size = ((rect.width() * ppp).round() as i32, (rect.height() * ppp).round() as i32);
            if size.0 > 0 && size.1 > 0 {
                let wants = mpv.wants_render();
                let resized = match &mut self.surface {
                    Some(s) if s.size == size => false,
                    Some(s) => {
                        unsafe { resize_surface(&gl, s.texture, s.fbo, size) };
                        s.size = size;
                        true
                    }
                    None => match unsafe { create_surface(&gl, size) } {
                        Ok((texture, fbo)) => {
                            let id = frame.register_native_glow_texture(texture);
                            self.surface = Some(Surface { texture, fbo, size, id });
                            true
                        }
                        Err(e) => {
                            self.mpv_error = Some(format!("OpenGL error: {e}"));
                            return;
                        }
                    },
                };
                if let Some(s) = &self.surface {
                    if wants || resized {
                        unsafe {
                            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(s.fbo));
                            gl.disable(glow::SCISSOR_TEST);
                            gl.disable(glow::BLEND);
                        }
                        mpv.render(s.fbo.0.get(), size.0, size.1);
                        unsafe { restore_gl_state(&gl) };
                    }
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                    painter.image(s.id, rect, uv, Color32::WHITE);
                }
            }
        }

        if !self.pb.loaded {
            painter.rect_filled(rect, 0.0, Color32::BLACK);
            let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
            let msg = if hovering {
                "Drop to open"
            } else if self.source.is_some() {
                "Loading…"
            } else {
                "Drop a video here, or press Ctrl+O\n\nF1 shows keyboard shortcuts"
            };
            painter.text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(18.0), TEXT_DIM);
        }

        if response.double_clicked() {
            self.apply(ui.ctx(), Action::ToggleFullscreen);
        } else if response.clicked() {
            self.toggle_play();
        }
    }

    // ---------------------------------------------------------------- transport + timeline

    fn ui_transport(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let fps = self.fps();
        let button = |ui: &mut egui::Ui, label: &str, tip: &str| {
            ui.add(egui::Button::new(RichText::new(label).size(15.0)).min_size(vec2(34.0, 26.0)))
                .on_hover_text(tip)
                .clicked()
        };
        ui.columns(3, |cols| {
            cols[0].horizontal(|ui| {
                ui.label(RichText::new(timecode::frames(self.display_pos(), fps)).monospace().size(20.0).color(ACCENT))
                    .on_hover_text("Current time (HH:MM:SS:FF)");
                if let Some(label) = self.speed_label() {
                    ui.label(RichText::new(label).monospace().size(14.0).color(Color32::from_rgb(0xff, 0xc8, 0x57)));
                }
            });

            let ui = &mut cols[1];
            ui.horizontal(|ui| {
                let row_width = 9.0 * 34.0 + 8.0 * ui.spacing().item_spacing.x + 2.0 * 14.0;
                ui.add_space(((ui.available_width() - row_width) / 2.0).max(0.0));
                let marker = ui
                    .add(egui::Button::new("").min_size(vec2(34.0, 26.0)))
                    .on_hover_text("Add note at playhead (M)");
                let c = marker.rect.center();
                ui.painter().add(Shape::convex_polygon(
                    vec![
                        pos2(c.x - 5.0, c.y - 7.0),
                        pos2(c.x + 5.0, c.y - 7.0),
                        pos2(c.x + 5.0, c.y + 2.0),
                        pos2(c.x, c.y + 7.0),
                        pos2(c.x - 5.0, c.y + 2.0),
                    ],
                    Category::Note.color(),
                    Stroke::NONE,
                ));
                if marker.clicked() {
                    actions.push(Action::AddMarker);
                }
                ui.add_space(6.0);
                if button(ui, "{", "Mark In (I)") {
                    actions.push(Action::MarkIn);
                }
                if button(ui, "}", "Mark Out (O)") {
                    actions.push(Action::MarkOut);
                }
                if button(ui, "{◀", "Go to In (Shift+I)") {
                    actions.push(Action::GoIn);
                }
                if button(ui, "◀|", "Step back one frame (Left arrow)") {
                    actions.push(Action::Step(-1, false));
                }
                let play = if self.pb.paused { "▶" } else { "⏸" };
                if button(ui, play, "Play / stop (Space)") {
                    actions.push(Action::TogglePlay);
                }
                if button(ui, "|▶", "Step forward one frame (Right arrow)") {
                    actions.push(Action::Step(1, false));
                }
                if button(ui, "▶}", "Go to Out (Shift+O)") {
                    actions.push(Action::GoOut);
                }
                ui.add_space(6.0);
                if button(ui, "📷", "Export frame as PNG (Ctrl+Shift+E)") {
                    actions.push(Action::ExportFrame);
                }
            });

            let width = cols[2].available_width();
            cols[2].allocate_ui_with_layout(vec2(width, 28.0), Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(timecode::frames(self.pb.duration, fps)).monospace().size(20.0).color(TEXT_DIM))
                    .on_hover_text("Duration");
                ui.add_space(12.0);
                let mut volume = self.pb.volume;
                if ui.add(egui::Slider::new(&mut volume, 0.0..=130.0).show_value(false)).changed() {
                    self.pb.volume = volume;
                    self.set("volume", &format!("{volume:.0}"));
                }
                let icon = if self.pb.mute || self.pb.volume <= 0.0 { "🔇" } else { "🔊" };
                if ui.add(egui::Button::new(icon).frame(false)).on_hover_text("Mute (0)").clicked() {
                    actions.push(Action::ToggleMute);
                }
            });
        });
        ui.add_space(4.0);
        self.ui_timeline(ui);
    }

    fn speed_label(&self) -> Option<String> {
        if self.pb.paused || !self.pb.loaded {
            return None;
        }
        if !self.pb.backward && (self.pb.speed - 1.0).abs() < 1e-6 {
            return None;
        }
        let arrow = if self.pb.backward { "◀" } else { "▶" };
        Some(format!("{arrow} {}x", self.pb.speed))
    }

    fn view(&mut self, rect: Rect) -> View {
        let dur = self.pb.duration.max(1e-3);
        self.tl.zoom = self.tl.zoom.clamp(1.0, self.max_zoom());
        let len = dur / self.tl.zoom;
        self.tl.start = self.tl.start.clamp(0.0, (dur - len).max(0.0));
        View { start: self.tl.start, len, left: rect.left(), width: rect.width().max(1.0) }
    }

    fn ui_timeline(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, PANEL_DARK);
        let dur = self.pb.duration;
        if dur <= 0.0 || !self.pb.loaded {
            painter.text(rect.center(), Align2::CENTER_CENTER, "No video loaded", FontId::proportional(12.0), TEXT_DIM);
            return;
        }
        let fps = self.fps();
        let ruler = Rect::from_min_size(rect.min, vec2(rect.width(), 20.0));
        let lane = Rect::from_min_max(pos2(rect.left(), ruler.bottom()), rect.max);

        // Mouse wheel: zoom around the cursor; horizontal (or Shift+wheel) pans.
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            let view = self.view(rect);
            if delta.y != 0.0 {
                let anchor = response.hover_pos().map(|p| view.t(p.x));
                self.zoom_by(1.0035f64.powf(delta.y as f64), anchor);
            }
            if delta.x != 0.0 {
                self.tl.start -= delta.x as f64 / rect.width() as f64 * view.len;
            }
        }

        // Keep the playhead visible while playing or after jumps, but let the user pan freely while paused.
        let pos = self.display_pos();
        if self.scrub.is_none() && self.tl.zoom > 1.0 && (!self.pb.paused || pos != self.tl.last_pos) {
            let view = self.view(rect);
            if pos < view.start || pos > view.start + view.len {
                self.tl.start = pos - view.len * 0.1;
            }
        }
        self.tl.last_pos = pos;
        let view = self.view(rect);

        self.handle_scrub(ui, &response, rect, lane, view);
        let view = self.view(rect);
        let pos = self.display_pos();

        // In/Out range.
        let in_out_fill = Color32::from_rgba_unmultiplied(0x2d, 0x8c, 0xeb, 38);
        if self.mark_in.is_some() || self.mark_out.is_some() {
            let a = view.x(self.mark_in.unwrap_or(0.0)).max(rect.left());
            let b = view.x(self.mark_out.unwrap_or(dur)).min(rect.right());
            if b > a {
                painter.rect_filled(Rect::from_x_y_ranges(a..=b, rect.y_range()), 0.0, in_out_fill);
            }
            for (t, open) in [(self.mark_in, true), (self.mark_out, false)] {
                if let Some(t) = t {
                    let x = view.x(t);
                    let stroke = Stroke::new(1.5, ACCENT);
                    painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], stroke);
                    let tick = if open { 5.0 } else { -5.0 };
                    painter.line_segment([pos2(x, rect.top()), pos2(x + tick, rect.top())], stroke);
                    painter.line_segment([pos2(x, rect.bottom()), pos2(x + tick, rect.bottom())], stroke);
                }
            }
        }

        // Ruler.
        painter.line_segment(
            [pos2(rect.left(), ruler.bottom()), pos2(rect.right(), ruler.bottom())],
            Stroke::new(1.0, Color32::from_gray(50)),
        );
        let px_per_sec = rect.width() as f64 / view.len;
        let frame = 1.0 / fps;
        let steps = [
            frame, 2.0 * frame, 5.0 * frame, 10.0 * frame, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0,
            600.0, 900.0, 1800.0, 3600.0, 7200.0,
        ];
        let step = steps.iter().copied().find(|s| s * px_per_sec >= 90.0).unwrap_or(7200.0);
        let minor = step / 5.0;
        let mut t = (view.start / minor).floor() * minor;
        while t <= view.start + view.len {
            let x = view.x(t);
            let major = ((t / step).round() * step - t).abs() < minor * 0.01;
            let h = if major { 8.0 } else { 4.0 };
            painter.line_segment(
                [pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())],
                Stroke::new(1.0, Color32::from_gray(if major { 120 } else { 70 })),
            );
            if major {
                painter.text(
                    pos2(x + 3.0, ruler.top() + 1.0),
                    Align2::LEFT_TOP,
                    timecode::ruler(t, step, fps),
                    FontId::monospace(10.0),
                    Color32::from_gray(140),
                );
            }
            t += minor;
        }

        // Notes: range bars along the bottom of the lane, flags at their start.
        let mut hovered_note: Option<&Note> = None;
        let hover = response.hover_pos();
        for note in &self.review.notes {
            let x = view.x(note.start);
            let end_x = note.end.map(|e| view.x(e));
            if x > rect.right() + 8.0 || end_x.unwrap_or(x) < rect.left() - 8.0 {
                continue;
            }
            let color = note.category.color();
            let selected = self.selected == Some(note.id);
            if let Some(end_x) = end_x {
                let bar = Rect::from_x_y_ranges(x.max(rect.left())..=end_x.min(rect.right()), (lane.bottom() - 9.0)..=(lane.bottom() - 3.0));
                painter.rect_filled(bar, 2.0, color.gamma_multiply(0.65));
            }
            painter.line_segment([pos2(x, ruler.top()), pos2(x, lane.bottom())], Stroke::new(1.0, color.gamma_multiply(0.45)));
            let flag = Rect::from_min_size(pos2(x - 4.0, lane.top() + 3.0), vec2(8.0, 12.0));
            painter.add(Shape::convex_polygon(
                vec![flag.left_top(), flag.right_top(), flag.right_bottom() - vec2(0.0, 3.0), pos2(x, flag.bottom()), flag.left_bottom() - vec2(0.0, 3.0)],
                color,
                if selected { Stroke::new(1.5, Color32::WHITE) } else { Stroke::NONE },
            ));
            if let Some(p) = hover
                && lane.contains(p)
                && (p.x - x).abs() <= 6.0
            {
                hovered_note = Some(note);
            }
        }

        // Playhead.
        let x = view.x(pos);
        if (rect.left()..=rect.right()).contains(&x) {
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.5, ACCENT));
            painter.add(Shape::convex_polygon(
                vec![pos2(x - 6.0, rect.top()), pos2(x + 6.0, rect.top()), pos2(x + 6.0, rect.top() + 6.0), pos2(x, rect.top() + 12.0), pos2(x - 6.0, rect.top() + 6.0)],
                ACCENT,
                Stroke::NONE,
            ));
        }

        // Hover readout.
        if let Some(p) = hover
            && self.scrub.is_none()
        {
            painter.line_segment([pos2(p.x, rect.top()), pos2(p.x, rect.bottom())], Stroke::new(1.0, Color32::from_white_alpha(40)));
            if let Some(note) = hovered_note {
                let text = if note.text.trim().is_empty() { "(empty note)" } else { note.text.trim() };
                let tip = format!("{} · {}\n{}", timecode::hms(note.start), note.category.label(), text);
                response.clone().on_hover_text_at_pointer(tip);
            } else {
                response.clone().on_hover_text_at_pointer(timecode::frames(view.t(p.x).clamp(0.0, dur), fps));
            }
        }
    }

    /// Press anywhere to jump; drag to scrub. While moving, cheap keyframe seeks keep up with the
    /// cursor; as soon as it rests (or on release) a frame-exact seek lands on the precise frame.
    fn handle_scrub(&mut self, ui: &egui::Ui, response: &egui::Response, rect: Rect, lane: Rect, view: View) {
        let dur = self.pb.duration;
        let pointer = response.interact_pointer_pos();
        let mut send: Option<(f64, bool)> = None;

        if response.is_pointer_button_down_on()
            && let Some(p) = pointer
        {
            if self.scrub.is_none() && !self.tl.marker_press {
                let hit = self
                    .review
                    .notes
                    .iter()
                    .filter(|n| lane.contains(p) && (view.x(n.start) - p.x).abs() <= 6.0)
                    .min_by(|a, b| (view.x(a.start) - p.x).abs().total_cmp(&(view.x(b.start) - p.x).abs()));
                if let Some(note) = hit {
                    let (id, start) = (note.id, note.start);
                    self.selected = Some(id);
                    self.scroll_to_selected = true;
                    self.tl.marker_press = true;
                    self.seek(start, true);
                    return;
                }
                let was_paused = self.pb.paused;
                if !was_paused {
                    self.set("pause", "yes");
                }
                self.scrub = Some(Scrub { was_paused, target: f64::NAN, last_move: Instant::now(), exact_sent: false });
            }
            if let Some(scrub) = &mut self.scrub {
                let t = view.t(p.x).clamp(0.0, dur);
                if t != scrub.target {
                    scrub.target = t;
                    scrub.last_move = Instant::now();
                    scrub.exact_sent = self.exact_scrub;
                    send = Some((t, self.exact_scrub));
                } else if !scrub.exact_sent && scrub.last_move.elapsed() >= SCRUB_SETTLE {
                    scrub.exact_sent = true;
                    send = Some((t, true));
                }
                if self.tl.zoom > 1.0 {
                    if p.x < rect.left() + 4.0 {
                        self.tl.start -= view.len * 0.02;
                    } else if p.x > rect.right() - 4.0 {
                        self.tl.start += view.len * 0.02;
                    }
                }
                ui.ctx().request_repaint_after(Duration::from_millis(30));
            }
        } else {
            self.tl.marker_press = false;
            if let Some(scrub) = self.scrub.take() {
                if scrub.target.is_finite() && !scrub.exact_sent {
                    send = Some((scrub.target, true));
                }
                if !scrub.was_paused {
                    self.set("pause", "no");
                }
            }
        }

        if let Some((t, exact)) = send {
            let mode = if exact { "absolute+exact" } else { "absolute+keyframes" };
            self.cmd(&["seek", &format!("{t:.6}"), mode]);
        }
    }

    // ---------------------------------------------------------------- notes panel

    fn ui_notes(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Notes").size(16.0).strong());
            ui.label(RichText::new(self.review.notes.len().to_string()).color(TEXT_DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Export…").on_hover_text("Export notes (Ctrl+M)").clicked() {
                    actions.push(Action::ShowExport);
                }
                if ui.button("+ Note").on_hover_text("Add a note at the playhead (M)").clicked() {
                    actions.push(Action::AddMarker);
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search notes")
                    .desired_width((ui.available_width() - 118.0).max(60.0)),
            );
            egui::ComboBox::from_id_salt("category_filter")
                .width(104.0)
                .selected_text(self.category_filter.map_or("All types", Category::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category_filter, None, "All types");
                    for c in Category::ALL {
                        ui.selectable_value(&mut self.category_filter, Some(c), RichText::new(c.label()).color(c.color()));
                    }
                });
        });
        let mut details_changed = false;
        egui::CollapsingHeader::new("Review details").default_open(false).show(ui, |ui| {
            egui::Grid::new("details").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Title");
                details_changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.review.title).hint_text(&self.review.video).desired_width(f32::INFINITY))
                    .changed();
                ui.end_row();
                ui.label("VOD URL");
                details_changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut self.review.source_url)
                            .hint_text("twitch.tv/videos/… or YouTube link")
                            .desired_width(f32::INFINITY),
                    )
                    .on_hover_text("When set, exported timestamps link straight to that moment in the online VOD")
                    .changed();
                ui.end_row();
            });
        });
        if details_changed {
            self.mark_dirty();
        }
        ui.separator();

        let fps = self.fps();
        let pos = self.display_pos();
        let active = self.review.notes.iter().rposition(|n| n.start <= pos + 0.5 / fps);
        let filter = self.filter.trim().to_lowercase();
        let category_filter = self.category_filter;
        let selected = self.selected;
        let scroll_to = std::mem::take(&mut self.scroll_to_selected);
        let mut focus_editor = std::mem::take(&mut self.focus_editor);
        let mut seek_to = None;
        let mut select = None;
        let mut delete = None;
        let mut edit = None;
        let mut text_snapshot = false;
        let mut text_changed = false;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.review.notes.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        "No notes yet.\n\nPress M to drop a note at the playhead, type it, then Enter.\n\
                         Use I / O and Alt+M to note a whole range.",
                    )
                    .color(TEXT_DIM),
                );
            }
            for (index, note) in self.review.notes.iter_mut().enumerate() {
                if category_filter.is_some_and(|c| c != note.category) {
                    continue;
                }
                if !filter.is_empty()
                    && !note.text.to_lowercase().contains(&filter)
                    && !note.category.label().to_lowercase().contains(&filter)
                {
                    continue;
                }
                let is_selected = selected == Some(note.id);
                let fill = if is_selected {
                    Color32::from_rgb(0x26, 0x34, 0x47)
                } else if active == Some(index) {
                    Color32::from_gray(0x33)
                } else {
                    Color32::from_gray(0x2b)
                };
                let row = egui::Frame::new()
                    .fill(fill)
                    .corner_radius(4.0)
                    .inner_margin(Margin::symmetric(8, 6))
                    .stroke(if is_selected { Stroke::new(1.0, ACCENT) } else { Stroke::NONE })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (chip, _) = ui.allocate_exact_size(vec2(4.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(chip, 1.0, note.category.color());
                            let stamp = match note.end {
                                Some(end) => format!("{} – {}", timecode::frames(note.start, fps), timecode::frames(end, fps)),
                                None => timecode::frames(note.start, fps),
                            };
                            if ui
                                .add(egui::Button::new(RichText::new(stamp).monospace().color(ACCENT)).frame(false))
                                .on_hover_text("Jump here")
                                .clicked()
                            {
                                seek_to = Some(note.start);
                                select = Some(note.id);
                            }
                            ui.label(RichText::new(note.category.label()).color(note.category.color()).small());
                            if is_selected {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("Delete").on_hover_text("Delete note (Del)").clicked() {
                                        delete = Some(note.id);
                                    }
                                });
                            }
                        });

                        if !is_selected {
                            let text = note.text.trim();
                            if text.is_empty() {
                                ui.label(RichText::new("(empty)").italics().color(TEXT_DIM));
                            } else {
                                let preview: String = text.lines().take(3).collect::<Vec<_>>().join("\n");
                                ui.add(egui::Label::new(preview).wrap());
                            }
                            return;
                        }

                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt(("category", note.id))
                                .width(96.0)
                                .selected_text(RichText::new(note.category.label()).color(note.category.color()))
                                .show_ui(ui, |ui| {
                                    for (i, c) in Category::ALL.into_iter().enumerate() {
                                        let label = RichText::new(format!("{}   Ctrl+{}", c.label(), i + 1)).color(c.color());
                                        if ui.selectable_label(note.category == c, label).clicked() && note.category != c {
                                            edit = Some(NoteEdit::Category(note.id, c));
                                        }
                                    }
                                });
                            if ui.small_button("Set start").on_hover_text("Move start to the playhead").clicked() {
                                edit = Some(NoteEdit::Start(note.id, pos));
                            }
                            if ui.small_button("Set end").on_hover_text("Set end to the playhead (makes a range)").clicked()
                                && pos > note.start
                            {
                                edit = Some(NoteEdit::End(note.id, Some(pos)));
                            }
                            if note.end.is_some() && ui.small_button("Clear end").on_hover_text("Make this a point note").clicked() {
                                edit = Some(NoteEdit::End(note.id, None));
                            }
                        });

                        let text_id = egui::Id::unique(("note_text", note.id));
                        let commit = ui.memory(|m| m.has_focus(text_id)) && ui.input_mut(take_plain_enter);
                        let text = ui.add(
                            egui::TextEdit::multiline(&mut note.text)
                                .id(text_id)
                                .hint_text("Type your note…  Enter = done · Shift+Enter = new line")
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        if focus_editor {
                            text.request_focus();
                            focus_editor = false;
                        }
                        if text.gained_focus() {
                            text_snapshot = true;
                        }
                        if text.changed() {
                            text_changed = true;
                        }
                        if commit {
                            text.surrender_focus();
                        }
                    });

                if !is_selected {
                    let click = ui.interact(row.response.rect, egui::Id::unique(("note_row", note.id)), Sense::click());
                    if click.clicked() {
                        select = Some(note.id);
                        seek_to = Some(note.start);
                    }
                }
                if is_selected && scroll_to {
                    row.response.scroll_to_me(Some(Align::Center));
                }
                ui.add_space(4.0);
            }
        });

        if text_snapshot {
            self.snapshot();
        }
        if text_changed {
            self.mark_dirty();
        }
        if let Some(id) = select {
            self.selected = Some(id);
        }
        if let Some(t) = seek_to {
            self.seek(t, true);
        }
        if let Some(id) = delete {
            self.selected = Some(id);
            self.delete_selected();
        }
        if let Some(edit) = edit {
            self.snapshot();
            let id = match edit {
                NoteEdit::Category(id, _) | NoteEdit::Start(id, _) | NoteEdit::End(id, _) => id,
            };
            if let Some(note) = self.review.notes.iter_mut().find(|n| n.id == id) {
                match edit {
                    NoteEdit::Category(_, c) => {
                        note.category = c;
                        self.last_category = c;
                    }
                    NoteEdit::Start(_, t) => {
                        note.start = t;
                        if note.end.is_some_and(|e| e <= t) {
                            note.end = None;
                        }
                    }
                    NoteEdit::End(_, e) => note.end = e,
                }
            }
            self.review.sort();
            self.mark_dirty();
        }
    }

    // ---------------------------------------------------------------- windows

    fn ui_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::MenuBar::new().ui(ui, |ui| {
            let mut item = |ui: &mut egui::Ui, label: &str, shortcut: &str, action: Action| {
                if ui.add(egui::Button::new(label).shortcut_text(shortcut)).clicked() {
                    actions.push(action);
                    ui.close();
                }
            };
            ui.menu_button("File", |ui| {
                item(ui, "Open video…", "Ctrl+O", Action::Open);
                item(ui, "Open URL…", "Ctrl+U", Action::OpenUrl);
                ui.separator();
                item(ui, "Save notes", "Ctrl+S", Action::Save);
                item(ui, "Export notes…", "Ctrl+M", Action::ShowExport);
                item(ui, "Export frame", "Ctrl+Shift+E", Action::ExportFrame);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, "Undo", "Ctrl+Z", Action::Undo);
                item(ui, "Redo", "Ctrl+Shift+Z", Action::Redo);
                ui.separator();
                item(ui, "Add note", "M", Action::AddMarker);
                item(ui, "Add range note (In to Out)", "Alt+M", Action::AddRangeNote);
                item(ui, "Delete note", "Del", Action::DeleteNote);
                ui.separator();
                item(ui, "Mark In", "I", Action::MarkIn);
                item(ui, "Mark Out", "O", Action::MarkOut);
                item(ui, "Clear In and Out", "Ctrl+Shift+X", Action::ClearInOut);
            });
            ui.menu_button("Playback", |ui| {
                item(ui, "Play / stop", "Space", Action::TogglePlay);
                item(ui, "Shuttle forward", "L", Action::Shuttle(1));
                item(ui, "Shuttle reverse", "J", Action::Shuttle(-1));
                item(ui, "Previous note", "Up", Action::PrevNote);
                item(ui, "Next note", "Down", Action::NextNote);
                ui.separator();
                ui.checkbox(&mut self.exact_scrub, "Frame-exact scrubbing")
                    .on_hover_text("Every scrub position decodes the exact frame. Slower on long-GOP footage.");
                ui.checkbox(&mut self.pause_on_note, "Pause when adding a note");
            });
            ui.menu_button("View", |ui| {
                item(ui, "Notes panel", "`", Action::ToggleNotes);
                item(ui, "Fullscreen", "F11", Action::ToggleFullscreen);
                ui.separator();
                item(ui, "Zoom in timeline", "=", Action::ZoomIn);
                item(ui, "Zoom out timeline", "-", Action::ZoomOut);
                item(ui, "Fit timeline", "\\", Action::ZoomFit);
            });
            ui.menu_button("Help", |ui| {
                item(ui, "Keyboard shortcuts", "F1", Action::Help);
            });
        });
    }

    fn ui_status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let small = |s: String| RichText::new(s).size(11.5).color(TEXT_DIM);
            if self.pb.loaded {
                if let Some(source) = &self.source {
                    ui.label(small(source.display_name()));
                }
                ui.label(small(format!("{}×{}", self.pb.width, self.pb.height)));
                if !self.pb.codec.is_empty() {
                    ui.label(small(self.pb.codec.clone()));
                }
                ui.label(small(format!("{:.3} fps", self.fps())));
                let (decode, color) = match self.pb.hwdec.as_str() {
                    "" | "no" => ("Software decode".to_string(), Color32::from_rgb(0xf0, 0xa0, 0x50)),
                    hw => (format!("HW decode: {hw}"), Color32::from_rgb(0x6c, 0xc6, 0x7f)),
                };
                ui.label(RichText::new(decode).size(11.5).color(color));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.source.is_some() {
                    let saved = if self.dirty { "Unsaved changes" } else { "Notes saved" };
                    ui.label(small(saved.into()));
                }
                if let Some((msg, at)) = &self.status {
                    if at.elapsed() < Duration::from_secs(6) {
                        ui.label(RichText::new(msg).size(11.5).color(Color32::from_rgb(0xe0, 0xe0, 0xe0)));
                    } else {
                        self.status = None;
                    }
                }
            });
        });
    }

    fn ui_export(&mut self, ctx: &egui::Context) {
        let mut open = self.show_export;
        let fps = self.fps();
        egui::Window::new("Export notes").open(&mut open).default_width(620.0).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("export_options").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Format");
                ui.horizontal(|ui| {
                    for f in ExportFormat::ALL {
                        ui.radio_value(&mut self.export.format, f, f.label());
                    }
                });
                ui.end_row();
                ui.label("Timestamps");
                ui.horizontal(|ui| {
                    for s in TcStyle::ALL {
                        ui.radio_value(&mut self.export.timecode, s, s.label());
                    }
                });
                ui.end_row();
                ui.label("Include");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.export.include_category, "Categories");
                    let has_range = self.mark_in.is_some() || self.mark_out.is_some();
                    ui.add_enabled(has_range, egui::Checkbox::new(&mut self.export_in_out_only, "Only notes between In and Out"));
                });
                ui.end_row();
            });
            self.export.range = (self.export_in_out_only && (self.mark_in.is_some() || self.mark_out.is_some()))
                .then(|| (self.mark_in.unwrap_or(0.0), self.mark_out.unwrap_or(f64::MAX)));
            if self.pb.fps <= 1.0 {
                self.review.fps = fps;
            }
            let text = notes::export(&self.review, &self.export);
            if self.review.source_url.trim().is_empty() && self.export.format != ExportFormat::Srt {
                ui.label(
                    RichText::new("Tip: add the VOD URL under Review details to get clickable timestamp links.")
                        .small()
                        .color(TEXT_DIM),
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Copy to clipboard").clicked() {
                    ctx.copy_text(text.clone());
                    self.set_status("Notes copied to clipboard".into());
                }
                if ui.button("Save as…").clicked() {
                    let ext = self.export.format.extension();
                    let stem = self.source.as_ref().map_or_else(|| "review".into(), Source::stem);
                    let mut dialog = rfd::FileDialog::new()
                        .set_file_name(format!("{stem}_notes.{ext}"))
                        .add_filter(self.export.format.label(), &[ext]);
                    if let Some(Source::File(p)) = &self.source
                        && let Some(dir) = p.parent()
                    {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(path) = dialog.save_file() {
                        match std::fs::write(&path, &text) {
                            Ok(()) => self.set_status(format!("Exported to {}", path.display())),
                            Err(e) => self.set_status(format!("Export failed: {e}")),
                        }
                    }
                }
                ui.label(RichText::new(format!("{} lines", text.lines().count())).small().color(TEXT_DIM));
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut text.as_str()).code_editor().desired_width(f32::INFINITY));
            });
        });
        self.show_export = open;
    }

    fn ui_help(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Keyboard shortcuts").open(&mut open).default_width(520.0).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new("Premiere Pro layout. Markers act as edit points.").color(TEXT_DIM));
            ui.add_space(4.0);
            egui::Grid::new("shortcuts").striped(true).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for (keys, what) in SHORTCUTS {
                    ui.label(RichText::new(*keys).monospace().color(ACCENT));
                    ui.label(*what);
                    ui.end_row();
                }
            });
        });
        self.show_help = open;
    }

    fn ui_open_url(&mut self, ctx: &egui::Context) {
        let mut open = self.show_open_url;
        let mut submit = false;
        egui::Window::new("Open URL").open(&mut open).default_width(480.0).collapsible(false).show(ctx, |ui| {
            ui.label("Twitch VOD, YouTube video, or any direct media URL.");
            ui.label(RichText::new("Streaming sites need yt-dlp installed and on PATH.").small().color(TEXT_DIM));
            let edit = ui.add(egui::TextEdit::singleline(&mut self.url_input).hint_text("https://").desired_width(f32::INFINITY));
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            if ui.button("Open").clicked() {
                submit = true;
            }
        });
        self.show_open_url = open;
        let url = self.url_input.trim().to_string();
        if submit && !url.is_empty() {
            self.show_open_url = false;
            self.open(ctx, Source::Url(url));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_mpv(&ctx);

        if let Some(path) = self.pending_open.take() {
            self.open(&ctx, Source::File(path));
        }
        let dropped = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).find(|p| !p.as_os_str().is_empty())
        });
        if let Some(path) = dropped {
            self.open(&ctx, Source::File(path));
        }

        for action in self.take_shortcuts(&ctx) {
            self.apply(&ctx, action);
        }

        let mut actions = Vec::new();
        egui::Panel::top("menu").frame(egui::Frame::new().fill(PANEL_DARK).inner_margin(Margin::symmetric(6, 3))).show(ui, |ui| {
            self.ui_menu(ui, &mut actions);
        });
        egui::Panel::bottom("status")
            .exact_size(22.0)
            .frame(egui::Frame::new().fill(PANEL_DARK).inner_margin(Margin::symmetric(8, 0)))
            .show(ui, |ui| self.ui_status(ui));
        egui::Panel::bottom("transport")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(10, 8)))
            .show(ui, |ui| self.ui_transport(ui, &mut actions));
        if self.show_notes {
            egui::Panel::right("notes")
                .resizable(true)
                .default_size(360.0)
                .min_size(260.0)
                .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(10, 4)))
                .show(ui, |ui| self.ui_notes(ui, &mut actions));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ui, |ui| self.draw_video(ui, frame));

        self.ui_export(&ctx);
        self.ui_help(&ctx);
        self.ui_open_url(&ctx);

        if !actions.is_empty() {
            for action in actions {
                self.apply(&ctx, action);
            }
            ctx.request_repaint();
        }

        if self.dirty {
            if self.last_edit.elapsed() >= AUTOSAVE_DELAY && !ctx.text_edit_focused() {
                self.save_notes(false);
            } else {
                ctx.request_repaint_after(AUTOSAVE_DELAY);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.save_notes(false);
        if let Some(mpv) = &mut self.mpv {
            mpv.free_render();
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = Color32::from_rgb(0x2a, 0x2a, 0x2a);
        style.visuals.extreme_bg_color = PANEL_DARK;
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.6);
        style.visuals.hyperlink_color = ACCENT;
        style.spacing.item_spacing = vec2(6.0, 5.0);
        style.spacing.button_padding = vec2(8.0, 3.0);
    });
}

/// Removes an Enter press (without Shift) from the queue, so Enter commits a note while
/// Shift+Enter still inserts a line break.
fn take_plain_enter(input: &mut egui::InputState) -> bool {
    let mut hit = false;
    input.events.retain(|e| {
        if let egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } = e
            && !modifiers.shift
        {
            hit = true;
            return false;
        }
        true
    });
    hit
}

unsafe fn create_surface(gl: &glow::Context, size: (i32, i32)) -> Result<(glow::Texture, glow::Framebuffer), String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        let fbo = gl.create_framebuffer()?;
        resize_surface(gl, texture, fbo, size);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        restore_gl_state(gl);
        if status != glow::FRAMEBUFFER_COMPLETE {
            return Err(format!("framebuffer incomplete (0x{status:x})"));
        }
        Ok((texture, fbo))
    }
}

/// Reallocates the texture storage and clears it to black.
unsafe fn resize_surface(gl: &glow::Context, texture: glow::Texture, fbo: glow::Framebuffer, size: (i32, i32)) {
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            size.0,
            size.1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        gl.disable(glow::SCISSOR_TEST);
        gl.viewport(0, 0, size.0, size.1);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        restore_gl_state(gl);
    }
}

/// mpv's renderer leaves its own GL state behind; reset what egui's painter relies on.
unsafe fn restore_gl_state(gl: &glow::Context) {
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE0);
        gl.use_program(None);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
    }
}
