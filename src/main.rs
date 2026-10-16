//! Ghost Music - a YouTube Music style desktop player for Windows.
//!
//! Modes:
//!   (default)      launch the app
//!   --selftest     run headless checks (tools, API, store) and exit
//!   --api-only     run just the local HTTP API (no window) - handy for scripts
//!   --version/-h   print information

mod app;
mod engine;
mod ghost;
mod http_api;
mod model;
mod player;
mod store;
mod theme;
mod thumbs;
mod tools;
mod views;
mod widgets;

use std::sync::Arc;

use engine::Engine;

const APP_NAME: &str = "Ghost Music";

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);

    if has("--version") || has("-V") {
        println!(
            "{APP_NAME} {} (api v{})",
            env!("CARGO_PKG_VERSION"),
            http_api::API_VERSION_INFO
        );
        println!("yt-dlp: {}", tools::tool_version("yt-dlp").unwrap_or_else(|| "not found".into()));
        println!("ffmpeg: {}", tools::tool_version("ffmpeg").unwrap_or_else(|| "not found".into()));
        return Ok(());
    }
    if has("--help") || has("-h") {
        print_help();
        return Ok(());
    }

    let engine = Engine::new();
    let library = store::Library::load();
    engine.set_download_dir(library.directory());

    let api = http_api::ApiServer::start(
        engine.clone(),
        tools::tool_version("yt-dlp"),
        tools::tool_version("ffmpeg"),
    );
    match &api {
        Some(api) => println!("{APP_NAME}: local API on {}", api.base_url()),
        None => eprintln!("{APP_NAME}: could not bind the local API port (audio playback may not work)"),
    }

    if has("--selftest") {
        publish_idle_state(&engine);
        return run_selftest(engine, api);
    }
    if has("--api-only") || has("--headless") {
        publish_idle_state(&engine);
        println!("{APP_NAME}: running without a window - press Ctrl+C to stop.");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }

    hide_own_console();

    let icon = load_icon();
    let options = eframe::NativeOptions {
        viewport: {
            let viewport = egui::ViewportBuilder::default()
                .with_title(APP_NAME)
                .with_app_id("GhostMusic")
                .with_inner_size([1220.0, 830.0])
                .with_min_inner_size([960.0, 640.0]);
            match icon {
                Some(icon) => viewport.with_icon(icon),
                None => viewport,
            }
        },
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::App::new(engine, api, library)))
        }),
    )
}

/// Windows creates a console window for the process when the app is started by
/// double-clicking, which makes a media player look unfinished. Hide it - but
/// only when this process owns the console: when Ghost Music is started from an
/// existing terminal the console is shared and `--version`/`--selftest` output
/// must stay visible.
#[cfg(windows)]
fn hide_own_console() {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        fn GetConsoleWindow() -> *mut c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(window: *mut c_void, command: i32) -> i32;
    }

    const SW_HIDE: i32 = 0;

    unsafe {
        let mut owners = [0u32; 2];
        let owners = GetConsoleProcessList(owners.as_mut_ptr(), owners.len() as u32);
        let window = GetConsoleWindow();
        if owners <= 1 && !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn hide_own_console() {}

/// Headless runs have no window, so nothing would ever call `publish_state`.
/// Publishing an idle snapshot here keeps `GET /player/state` meaningful.
fn publish_idle_state(engine: &Engine) {
    engine.publish_state(serde_json::json!({
        "state": "idle",
        "headless": true,
        "app": APP_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": http_api::API_VERSION_INFO,
        "queue": [],
        "has_track": false,
    }));
}

fn print_help() {
    println!(
        "{APP_NAME} {version} - YouTube Music style player powered by yt-dlp

USAGE:
  Ghost-Music.exe [options]

OPTIONS:
  -h, --help        show this help
  -V, --version     show versions of the app, yt-dlp and ffmpeg
      --selftest    run headless checks and exit with a non-zero code on failure
      --api-only    run only the local HTTP API (no window)
      --quick       short splash screen (for screenshots/tests)

MEDIA TOOLS:
  Put yt-dlp.exe (required) and ffmpeg.exe (recommended) next to Ghost-Music.exe,
  or install them on PATH. The app also checks a ./bin folder.

FILES:
  Settings, playlists and likes: %APPDATA%/Ghost Music/library.json
  Downloads:                     %USERPROFILE%/Downloads/Ghost Music
  Stream cache:                  %LOCALAPPDATA%/Ghost Music/cache

Only download or stream content you have the rights to use.",
        version = env!("CARGO_PKG_VERSION")
    );
}

/// Window/taskbar icon, decoded from the bundled PNG artwork.
fn load_icon() -> Option<Arc<egui::IconData>> {
    let bytes = include_bytes!("../assets/ghost-logo-256.png");
    let image = image::load_from_memory(bytes).ok()?;
    let image = image.resize(128, 128, image::imageops::FilterType::Triangle).to_rgba8();
    let (width, height) = image.dimensions();
    Some(Arc::new(egui::IconData { rgba: image.into_raw(), width, height }))
}

/// Headless checks: proves the API, the stream registry and the store work.
fn run_selftest(engine: Arc<Engine>, api: Option<Arc<http_api::ApiServer>>) -> eframe::Result<()> {
    let mut failures = 0;
    let mut check = |name: &str, ok: bool, optional: bool, detail: &str| {
        let mark = if ok {
            "PASS"
        } else if optional {
            "SKIP"
        } else {
            "FAIL"
        };
        if !ok && !optional {
            failures += 1;
        }
        println!("{mark}  {name}{}", if detail.is_empty() { String::new() } else { format!("  ({detail})") });
    };

    println!("{APP_NAME} self test");
    println!("-----------------");
    println!(
        "env: os={} exe={} cwd={}",
        std::env::consts::OS,
        std::env::current_exe().map(|path| path.display().to_string()).unwrap_or_default(),
        std::env::current_dir().map(|path| path.display().to_string()).unwrap_or_default()
    );
    println!(
        "env: yt-dlp={} ffmpeg={} cache={}",
        tools::resolve("yt-dlp"),
        tools::resolve("ffmpeg"),
        tools::cache_dir().display()
    );

    // Store round-trip (in memory: never touch the user's library here).
    let library = store::Library::default();
    let round_trip = serde_json::to_string(&library)
        .ok()
        .and_then(|json| serde_json::from_str::<store::Library>(&json).ok())
        .map(|back| back.version == store::FORMAT_VERSION)
        .unwrap_or(false);
    check("library JSON round-trip", round_trip, false, "");

    // Formatting helpers.
    check(
        "duration formatting",
        model::format_duration(Some(372.0)) == "6:12"
            && model::format_duration(None) == "—:—"
            && model::format_views(Some(1_500_000)) == "1.5M views",
        false,
        "",
    );

    // Stream registry.
    check("stream registry helpers", http_api::registry_smoke_test(&engine), false, "");

    // Local API.
    match &api {
        Some(api) => {
            check(
                "API port bound",
                api.port > 0 && tools::port_open("127.0.0.1", api.port),
                false,
                &api.base_url(),
            );
            let (status, body) = http_api::http_get(api.port, "/health").unwrap_or((0, String::new()));
            check("GET /health", status == 200 && body.contains(APP_NAME), false, &format!("HTTP {status}"));
            match http_api::http_get(api.port, "/player/state") {
                Some((status, body)) => {
                    let json_ok = body.contains("\"state\"") || body.contains("\"status\"");
                    check(
                        "GET /player/state",
                        status == 200 && json_ok,
                        false,
                        &format!("HTTP {status}{}", if json_ok { "" } else { " (no state field)" }),
                    );
                }
                None => check("GET /player/state", false, false, "no reply"),
            }
            let (status, body) =
                http_api::http_get(api.port, "/downloads").unwrap_or((0, String::new()));
            check("GET /downloads", status == 200, false, &format!("HTTP {status}"));
            let _ = body;
            let (status, _) = http_api::http_get(api.port, "/").unwrap_or((0, String::new()));
            check("GET / (remote page)", status == 200, false, &format!("HTTP {status}"));
        }
        None => check("local API", false, false, "could not bind a loopback port"),
    }

    // Audio output (informational: CI machines have no sound card).
    {
        let probe = player::Player::new(0.8);
        check(
            "audio output device",
            probe.is_available(),
            true,
            probe.error.as_deref().unwrap_or("ok"),
        );
    }

    // Media tools.
    let yt_dlp = tools::available("yt-dlp");
    check("yt-dlp available", yt_dlp, true, "required for search/streaming");
    check("ffmpeg available", tools::available("ffmpeg"), true, "needed for MP3 + HLS");

    // Live YouTube lookups depend on the network and on YouTube's tolerance for
    // the calling IP, so they are reported but never fail the run.
    if yt_dlp {
        match engine::search_sync("lofi hip hop", 3) {
            Ok(entries) => check(
                "yt-dlp search",
                !entries.is_empty(),
                true,
                &format!("{} results", entries.len()),
            ),
            Err(error) => check("yt-dlp search", false, true, &error),
        }
    } else {
        check("yt-dlp search", false, true, "skipped: yt-dlp not installed");
    }

    println!("-----------------");
    if failures == 0 {
        println!("All required checks passed. 👻");
        Ok(())
    } else {
        println!("{failures} check(s) failed.");
        std::process::exit(1);
    }
}
