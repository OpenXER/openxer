//! Embed the version information in `openxer-cli.exe` without requiring a
//! separate rc.exe (`tools/winres.rs` writes the .res file).
use std::{env, fs, io, path::PathBuf};

#[path = "../tools/winres.rs"]
mod winres;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=../tools/winres.rs");
    println!("cargo:rerun-if-changed=../LICENSE-MIT");
    // Other targets, and Windows builds without the MSVC linker, get no
    // version resource; the command line works the same without it.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return Ok(());
    }
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let copyright = winres::copyright_line(&fs::read_to_string("../LICENSE-MIT")?);
    let info = winres::version_info(
        &version,
        &[
            ("FileDescription", "OpenXER command line and MCP server"),
            ("FileVersion", &version),
            ("InternalName", "openxer-cli"),
            ("LegalCopyright", &copyright),
            ("OriginalFilename", "openxer-cli.exe"),
            ("ProductName", "OpenXER"),
            ("ProductVersion", &version),
        ],
    );
    let mut out = winres::new_res();
    winres::resource(&mut out, winres::RT_VERSION, 1, winres::LANG_EN_US, &info);
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("openxer-cli-version.res");
    fs::write(&path, winres::finish_res(out))?;
    println!("cargo:rustc-link-arg-bin=openxer-cli={}", path.display());
    Ok(())
}
