//! Embed the authored application icon, the version information and the
//! application manifest without requiring a separate rc.exe
//! (`tools/winres.rs` writes the .res file).
use std::{env, fs, io, path::PathBuf};

#[path = "../tools/winres.rs"]
mod winres;
use winres::word;

/// Per-monitor DPI awareness, the UTF-8 code page, Windows 10 and 11, and
/// no elevation.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=../assets/icons/openxer.ico");
    println!("cargo:rerun-if-changed=../tools/winres.rs");
    println!("cargo:rerun-if-changed=../LICENSE-MIT");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return Err(io::Error::other(
            "The Windows desktop build uses the MSVC target for icon resource linking",
        ));
    }
    let bytes = fs::read("../assets/icons/openxer.ico")?;
    let read_word = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let read_dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    if bytes.len() < 6 || read_word(0) != 0 || read_word(2) != 1 {
        return Err(io::Error::other(
            "OpenXER app icon is not a Windows ICO file",
        ));
    }
    let count = read_word(4);
    if count == 0 || 6 + usize::from(count) * 16 > bytes.len() {
        return Err(io::Error::other(
            "OpenXER app icon has an incomplete image directory",
        ));
    }
    let mut out = winres::new_res();
    let mut group = bytes[..6].to_vec();
    for index in 0..count {
        let entry = 6 + usize::from(index) * 16;
        let length = read_dword(entry + 8) as usize;
        let offset = read_dword(entry + 12) as usize;
        let payload = offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
            .filter(|data| !data.is_empty())
            .ok_or_else(|| io::Error::other("OpenXER app icon image is truncated"))?;
        let id = index + 1;
        winres::resource(&mut out, winres::RT_ICON, id, winres::LANG_NEUTRAL, payload);
        group.extend(&bytes[entry..entry + 12]);
        word(&mut group, id); // Group entries use a resource ID instead of a file offset.
    }
    // The lowest group ID is the app icon.
    winres::resource(
        &mut out,
        winres::RT_GROUP_ICON,
        1,
        winres::LANG_NEUTRAL,
        &group,
    );
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let copyright = winres::copyright_line(&fs::read_to_string("../LICENSE-MIT")?);
    let info = winres::version_info(
        &version,
        &[
            ("FileDescription", "OpenXER scheduler"),
            ("FileVersion", &version),
            ("InternalName", "OpenXER"),
            ("LegalCopyright", &copyright),
            ("OriginalFilename", "OpenXER.exe"),
            ("ProductName", "OpenXER"),
            ("ProductVersion", &version),
        ],
    );
    winres::resource(&mut out, winres::RT_VERSION, 1, winres::LANG_EN_US, &info);
    winres::resource(
        &mut out,
        winres::RT_MANIFEST,
        1,
        winres::LANG_EN_US,
        MANIFEST.as_bytes(),
    );
    let out = winres::finish_res(out);
    let path =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR")).join("openxer-icon.res");
    fs::write(&path, out)?;
    println!("cargo:rustc-link-arg-bin=OpenXER={}", path.display());
    // The manifest above is the application's; the linker must not add its
    // own default one (two manifests with ID 1 do not link).
    println!("cargo:rustc-link-arg-bin=OpenXER=/MANIFEST:NO");
    Ok(())
}
