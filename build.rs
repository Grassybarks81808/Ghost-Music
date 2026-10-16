//! Build script: on Windows, stamp the executable with the Ghost Music icon,
//! version information and a DPI-aware manifest.
//!
//! The resource step is best-effort: if the Windows SDK resource compiler is not
//! available the app still builds, it just loses the Explorer icon.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/ghost.ico");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/ghost.ico");
    res.set("ProductName", "Ghost Music");
    res.set("FileDescription", "Ghost Music - YouTube Music style desktop player");
    res.set("CompanyName", "Ghost Music");
    res.set("LegalCopyright", "MIT licensed - https://github.com/Grassybarks81808/Ghost-Music");
    res.set("OriginalFilename", "Ghost-Music.exe");

    if let Err(error) = res.compile() {
        // Never fail the build over cosmetics, but say so loudly in the log.
        println!("cargo:warning=Ghost Music: could not embed the Windows icon/version info ({error})");
    }
}
