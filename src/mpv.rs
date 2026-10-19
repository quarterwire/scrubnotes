//! Runtime-loaded bindings to libmpv's client API and its OpenGL render API.
//!
//! libmpv is loaded with `libloading` instead of being linked, so building the app needs no
//! import libraries or C toolchain; the DLL only has to sit next to the executable at runtime.

use std::ffi::{CStr, CString, OsString, c_char, c_int, c_void};
use std::path::Path;
use std::ptr;

const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;

const EVENT_NONE: c_int = 0;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_COMMAND_REPLY: c_int = 5;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const END_FILE_REASON_ERROR: c_int = 4;

const RENDER_PARAM_INVALID: c_int = 0;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_PARAM_OPENGL_FBO: c_int = 3;
const RENDER_PARAM_FLIP_Y: c_int = 4;
const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;

const RENDER_UPDATE_FRAME: u64 = 1;

/// Options applied before `mpv_initialize`. Tuned for review work: hardware decoding,
/// a large in-memory demuxer cache in both directions so scrubbing back and forth rarely
/// touches the disk, and no built-in OSD/keybindings since the app draws its own UI.
const OPTIONS: &[(&str, &str)] = &[
    ("vo", "libmpv"),
    ("hwdec", "auto-safe"),
    ("keep-open", "yes"),
    ("idle", "yes"),
    ("osc", "no"),
    ("osd-level", "0"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    // Required when rendering with BLOCK_FOR_TARGET_TIME=0, otherwise frames are shown early.
    ("video-timing-offset", "0"),
    ("hr-seek-framedrop", "yes"),
    ("cache", "yes"),
    ("demuxer-seekable-cache", "yes"),
    ("demuxer-max-bytes", "384MiB"),
    ("demuxer-max-back-bytes", "384MiB"),
    ("screenshot-format", "png"),
];

type Callback = unsafe extern "C" fn(*mut c_void);
type GetProcAddress = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
}

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: GetProcAddress,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

macro_rules! mpv_api {
    ($($field:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        struct Api {
            $($field: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
            _lib: libloading::Library,
        }

        impl Api {
            fn load(lib: libloading::Library) -> Result<Self, String> {
                $(
                    let $field = unsafe {
                        *lib.get::<unsafe extern "C" fn($($arg),*) $(-> $ret)?>(
                            concat!("mpv_", stringify!($field), "\0").as_bytes(),
                        )
                        .map_err(|e| format!("libmpv is missing mpv_{}: {e}", stringify!($field)))?
                    };
                )*
                Ok(Self { $($field,)* _lib: lib })
            }
        }
    };
}

mpv_api! {
    create: fn() -> *mut c_void;
    initialize: fn(*mut c_void) -> c_int;
    terminate_destroy: fn(*mut c_void);
    set_option_string: fn(*mut c_void, *const c_char, *const c_char) -> c_int;
    command_async: fn(*mut c_void, u64, *mut *const c_char) -> c_int;
    observe_property: fn(*mut c_void, u64, *const c_char, c_int) -> c_int;
    wait_event: fn(*mut c_void, f64) -> *mut RawEvent;
    set_wakeup_callback: fn(*mut c_void, Option<Callback>, *mut c_void);
    error_string: fn(c_int) -> *const c_char;
    render_context_create: fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int;
    render_context_set_update_callback: fn(*mut c_void, Option<Callback>, *mut c_void);
    render_context_update: fn(*mut c_void) -> u64;
    render_context_render: fn(*mut c_void, *mut RenderParam) -> c_int;
    render_context_free: fn(*mut c_void);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    TimePos,
    Duration,
    Pause,
    Speed,
    ContainerFps,
    EstimatedFps,
    Volume,
    Mute,
    Hwdec,
    Codec,
    Width,
    Height,
    PlayDir,
    Seeking,
}

impl Prop {
    const ALL: [Prop; 14] = [
        Prop::TimePos,
        Prop::Duration,
        Prop::Pause,
        Prop::Speed,
        Prop::ContainerFps,
        Prop::EstimatedFps,
        Prop::Volume,
        Prop::Mute,
        Prop::Hwdec,
        Prop::Codec,
        Prop::Width,
        Prop::Height,
        Prop::PlayDir,
        Prop::Seeking,
    ];

    fn name(self) -> &'static CStr {
        match self {
            Prop::TimePos => c"time-pos",
            Prop::Duration => c"duration",
            Prop::Pause => c"pause",
            Prop::Speed => c"speed",
            Prop::ContainerFps => c"container-fps",
            Prop::EstimatedFps => c"estimated-vf-fps",
            Prop::Volume => c"volume",
            Prop::Mute => c"mute",
            Prop::Hwdec => c"hwdec-current",
            Prop::Codec => c"current-tracks/video/codec",
            Prop::Width => c"width",
            Prop::Height => c"height",
            Prop::PlayDir => c"play-dir",
            Prop::Seeking => c"seeking",
        }
    }

    fn format(self) -> c_int {
        match self {
            Prop::Pause | Prop::Mute | Prop::Seeking => FORMAT_FLAG,
            Prop::Hwdec | Prop::Codec | Prop::PlayDir => FORMAT_STRING,
            Prop::Width | Prop::Height => FORMAT_INT64,
            _ => FORMAT_DOUBLE,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    None,
    Flag(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

#[derive(Debug)]
pub enum Event {
    Property(Prop, Value),
    FileLoaded,
    EndFile { error: Option<String> },
    CommandFailed(String),
    Shutdown,
}

pub struct Mpv {
    api: Api,
    handle: *mut c_void,
    render: *mut c_void,
    waker: *mut egui::Context,
}

unsafe extern "C" fn wake(ctx: *mut c_void) {
    let ctx = unsafe { &*(ctx as *const egui::Context) };
    ctx.request_repaint();
}

impl Mpv {
    /// Loads libmpv and starts a player core. `waker` is repainted whenever mpv has
    /// events or a new video frame, so the UI idles at zero CPU while nothing changes.
    pub fn new(waker: egui::Context) -> Result<Self, String> {
        let api = Api::load(load_library()?)?;
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let mpv = Mpv {
            api,
            handle,
            render: ptr::null_mut(),
            waker: Box::into_raw(Box::new(waker)),
        };
        for (name, value) in OPTIONS {
            let (n, v) = (CString::new(*name).unwrap(), CString::new(*value).unwrap());
            let err = unsafe { (mpv.api.set_option_string)(handle, n.as_ptr(), v.as_ptr()) };
            if err < 0 {
                eprintln!("mpv: option {name}={value} rejected: {}", mpv.error_string(err));
            }
        }
        let err = unsafe { (mpv.api.initialize)(handle) };
        if err < 0 {
            return Err(format!("mpv_initialize failed: {}", mpv.error_string(err)));
        }
        for (i, prop) in Prop::ALL.iter().enumerate() {
            unsafe { (mpv.api.observe_property)(handle, i as u64, prop.name().as_ptr(), prop.format()) };
        }
        unsafe { (mpv.api.set_wakeup_callback)(handle, Some(wake), mpv.waker.cast()) };
        Ok(mpv)
    }

    /// Creates the OpenGL render context. The GL context that will be used for
    /// [`Self::render`] must be current on the calling thread.
    pub fn init_render(&mut self) -> Result<(), String> {
        let mut init = OpenGlInitParams {
            get_proc_address: gl::get_proc_address,
            get_proc_address_ctx: ptr::null_mut(),
        };
        let mut params = [
            RenderParam { kind: RENDER_PARAM_API_TYPE, data: c"opengl".as_ptr() as *mut c_void },
            RenderParam {
                kind: RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&mut init as *mut OpenGlInitParams).cast(),
            },
            RenderParam { kind: RENDER_PARAM_INVALID, data: ptr::null_mut() },
        ];
        let mut ctx = ptr::null_mut();
        let err = unsafe { (self.api.render_context_create)(&mut ctx, self.handle, params.as_mut_ptr()) };
        if err < 0 {
            return Err(format!("mpv render context: {}", self.error_string(err)));
        }
        self.render = ctx;
        unsafe { (self.api.render_context_set_update_callback)(ctx, Some(wake), self.waker.cast()) };
        Ok(())
    }

    /// Returns true when mpv has a new frame that should be rendered.
    pub fn wants_render(&self) -> bool {
        !self.render.is_null() && unsafe { (self.api.render_context_update)(self.render) } & RENDER_UPDATE_FRAME != 0
    }

    /// Renders the current frame into `fbo` (letterboxed to `w`×`h`), without blocking
    /// for the frame's display time.
    pub fn render(&self, fbo: u32, w: i32, h: i32) {
        if self.render.is_null() {
            return;
        }
        let mut target = OpenGlFbo { fbo: fbo as c_int, w, h, internal_format: 0 };
        let mut flip: c_int = 0;
        let mut block: c_int = 0;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_OPENGL_FBO, data: (&mut target as *mut OpenGlFbo).cast() },
            RenderParam { kind: RENDER_PARAM_FLIP_Y, data: (&mut flip as *mut c_int).cast() },
            RenderParam { kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME, data: (&mut block as *mut c_int).cast() },
            RenderParam { kind: RENDER_PARAM_INVALID, data: ptr::null_mut() },
        ];
        unsafe { (self.api.render_context_render)(self.render, params.as_mut_ptr()) };
    }

    /// Frees the render context. Must run while the GL context is still current.
    pub fn free_render(&mut self) {
        if !self.render.is_null() {
            unsafe { (self.api.render_context_free)(self.render) };
            self.render = ptr::null_mut();
        }
    }

    /// Runs an mpv command asynchronously so the UI thread never waits on the core.
    pub fn command(&self, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap_or_default()).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        unsafe { (self.api.command_async)(self.handle, 0, ptrs.as_mut_ptr()) };
    }

    pub fn set(&self, property: &str, value: &str) {
        self.command(&["set", property, value]);
    }

    pub fn poll_events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            let ev = unsafe { &*(self.api.wait_event)(self.handle, 0.0) };
            match ev.event_id {
                EVENT_NONE => break,
                EVENT_PROPERTY_CHANGE => {
                    let Some(prop) = Prop::ALL.get(ev.reply_userdata as usize) else { continue };
                    let raw = unsafe { &*(ev.data as *const RawEventProperty) };
                    out.push(Event::Property(*prop, unsafe { read_value(raw.format, raw.data) }));
                }
                EVENT_FILE_LOADED => out.push(Event::FileLoaded),
                EVENT_END_FILE => {
                    let raw = unsafe { &*(ev.data as *const RawEndFile) };
                    let error = (raw.reason == END_FILE_REASON_ERROR).then(|| self.error_string(raw.error));
                    out.push(Event::EndFile { error });
                }
                EVENT_COMMAND_REPLY if ev.error < 0 => {
                    out.push(Event::CommandFailed(self.error_string(ev.error)));
                }
                EVENT_SHUTDOWN => {
                    out.push(Event::Shutdown);
                    break;
                }
                _ => {}
            }
        }
        out
    }

    fn error_string(&self, err: c_int) -> String {
        let s = unsafe { (self.api.error_string)(err) };
        if s.is_null() {
            format!("error {err}")
        } else {
            unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        self.free_render();
        unsafe {
            (self.api.terminate_destroy)(self.handle);
            drop(Box::from_raw(self.waker));
        }
    }
}

unsafe fn read_value(format: c_int, data: *mut c_void) -> Value {
    if data.is_null() {
        return Value::None;
    }
    unsafe {
        match format {
            FORMAT_FLAG => Value::Flag(*(data as *const c_int) != 0),
            FORMAT_INT64 => Value::Int(*(data as *const i64)),
            FORMAT_DOUBLE => Value::Double(*(data as *const f64)),
            FORMAT_STRING => {
                let s = *(data as *const *const c_char);
                if s.is_null() {
                    Value::None
                } else {
                    Value::Str(CStr::from_ptr(s).to_string_lossy().into_owned())
                }
            }
            _ => Value::None,
        }
    }
}

fn load_library() -> Result<libloading::Library, String> {
    #[cfg(windows)]
    const NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll", "mpv-1.dll"];
    #[cfg(target_os = "macos")]
    const NAMES: &[&str] = &["libmpv.2.dylib", "libmpv.dylib"];
    #[cfg(all(unix, not(target_os = "macos")))]
    const NAMES: &[&str] = &["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let mut tried = Vec::new();
    for name in NAMES {
        // Next to the exe, then in a libmpv/ folder beside it (the layout of the repo root, where
        // build.ps1 puts ScrubNotes.exe), then wherever the system search path finds it.
        let candidates = exe_dir
            .iter()
            .flat_map(|dir| [dir.join(name), dir.join("libmpv").join(name)])
            .map(|p| p.into_os_string())
            .chain(std::iter::once(OsString::from(name)));
        for candidate in candidates {
            match unsafe { libloading::Library::new(&candidate) } {
                Ok(lib) => return Ok(lib),
                Err(e) => tried.push(format!("{}: {e}", candidate.to_string_lossy())),
            }
        }
    }
    Err(format!(
        "Could not load libmpv. Place libmpv-2.dll next to the executable \
         (scripts/fetch-libmpv.ps1 downloads it).\n\nTried:\n{}",
        tried.join("\n")
    ))
}

/// Resolves OpenGL entry points for mpv from whatever GL implementation eframe is using.
mod gl {
    use std::ffi::{CStr, c_char, c_void};
    use std::ptr;
    use std::sync::OnceLock;

    type PlatformGetProc = unsafe extern "system" fn(*const c_char) -> *mut c_void;

    struct Loader {
        lib: Option<libloading::Library>,
        get_proc: Option<PlatformGetProc>,
    }

    static LOADER: OnceLock<Loader> = OnceLock::new();

    fn open(lib: &str, get_proc: &[u8]) -> Option<Loader> {
        let lib = unsafe { libloading::Library::new(lib) }.ok()?;
        let get_proc = unsafe { lib.get::<PlatformGetProc>(get_proc) }.ok().map(|s| *s);
        Some(Loader { lib: Some(lib), get_proc })
    }

    fn init() -> Loader {
        #[cfg(windows)]
        let found = open("opengl32.dll", b"wglGetProcAddress\0");
        #[cfg(target_os = "macos")]
        let found = open("/System/Library/Frameworks/OpenGL.framework/OpenGL", b"\0");
        #[cfg(all(unix, not(target_os = "macos")))]
        let found = open("libEGL.so.1", b"eglGetProcAddress\0").or_else(|| open("libGL.so.1", b"glXGetProcAddressARB\0"));
        found.unwrap_or(Loader { lib: None, get_proc: None })
    }

    pub unsafe extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
        let loader = LOADER.get_or_init(init);
        if let Some(get_proc) = loader.get_proc {
            let p = unsafe { get_proc(name) };
            // wglGetProcAddress signals failure with 0, 1, 2, 3 or -1 rather than only null.
            if !(-1..=3).contains(&(p as isize)) {
                return p;
            }
        }
        let Some(lib) = &loader.lib else { return ptr::null_mut() };
        let name = unsafe { CStr::from_ptr(name) }.to_bytes_with_nul();
        unsafe { lib.get::<*mut c_void>(name) }.map(|s| *s).unwrap_or(ptr::null_mut())
    }
}
