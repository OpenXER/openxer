//! Windows resources for the build scripts of `desktop` and `automation`,
//! written as a compiled `.res` file without rc.exe or a resource crate.
//! MSVC's linker accepts the documented .res format directly:
//! https://learn.microsoft.com/windows/win32/menurc/resourceheader
//! https://learn.microsoft.com/windows/win32/menurc/vs-versioninfo
//! https://devblogs.microsoft.com/oldnewthing/20120720-00/?p=7083
#![allow(dead_code)]

pub const RT_ICON: u16 = 3;
pub const RT_GROUP_ICON: u16 = 14;
pub const RT_VERSION: u16 = 16;
pub const RT_MANIFEST: u16 = 24;
/// Language-neutral, for artwork.
pub const LANG_NEUTRAL: u16 = 0;
/// US English, the language the version strings are written in.
pub const LANG_EN_US: u16 = 0x0409;

pub fn word(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}

pub fn dword(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

fn align(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn wide(out: &mut Vec<u8>, text: &str) {
    for unit in text.encode_utf16() {
        word(out, unit);
    }
    word(out, 0);
}

/// One resource entry of a .res file, the type and name given as ordinals.
pub fn resource(out: &mut Vec<u8>, kind: u16, id: u16, language: u16, data: &[u8]) {
    align(out);
    dword(out, data.len() as u32);
    dword(out, 32);
    word(out, 0xffff);
    word(out, kind);
    word(out, 0xffff);
    word(out, id);
    dword(out, 0);
    // Moveable, pure and discardable, as rc.exe writes them; the empty
    // header entry has no flags.
    word(out, if kind == 0 { 0 } else { 0x1030 });
    word(out, language);
    dword(out, 0);
    dword(out, 0);
    out.extend(data);
}

/// A new .res file: the required empty header entry.
pub fn new_res() -> Vec<u8> {
    let mut out = vec![];
    resource(&mut out, 0, 0, 0, &[]);
    out
}

pub fn finish_res(mut out: Vec<u8>) -> Vec<u8> {
    align(&mut out);
    out
}

/// One node of the version resource: length, value length, type, key,
/// value, children, each part on a 32-bit boundary.
fn node(key: &str, value: &[u8], value_length: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0, 0];
    word(&mut out, value_length);
    word(&mut out, u16::from(text));
    wide(&mut out, key);
    align(&mut out);
    out.extend(value);
    for child in children {
        align(&mut out);
        out.extend(child);
    }
    let length = out.len() as u16;
    out[..2].copy_from_slice(&length.to_le_bytes());
    out
}

/// `major.minor.patch` from a Cargo version (pre-release text is dropped).
pub fn version_numbers(version: &str) -> [u16; 4] {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let mut parts = core.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        0,
    ]
}

/// The first `Copyright` line of a license text.
pub fn copyright_line(license: &str) -> String {
    license
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Copyright"))
        .unwrap_or("")
        .to_string()
}

/// A VS_VERSIONINFO resource for an application.
pub fn version_info(version: &str, strings: &[(&str, &str)]) -> Vec<u8> {
    let [a, b, c, d] = version_numbers(version);
    let ms = (u32::from(a) << 16) | u32::from(b);
    let ls = (u32::from(c) << 16) | u32::from(d);
    let mut fixed = vec![];
    for value in [
        0xFEEF_04BD, // signature
        0x0001_0000, // structure version 1.0
        ms,
        ls, // file version
        ms,
        ls,          // product version
        0x3F,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0,
        0,
        0, // subtype, date
    ] {
        dword(&mut fixed, value);
    }
    let entries: Vec<Vec<u8>> = strings
        .iter()
        .map(|(key, value)| {
            let mut text = vec![];
            wide(&mut text, value);
            node(
                key,
                &text,
                (value.encode_utf16().count() + 1) as u16,
                true,
                &[],
            )
        })
        .collect();
    // US English, Unicode (code page 1200).
    let table = node("040904B0", &[], 0, true, &entries);
    let string_info = node("StringFileInfo", &[], 0, true, &[table]);
    let mut translation = vec![];
    word(&mut translation, LANG_EN_US);
    word(&mut translation, 1200);
    let var = node("Translation", &translation, 4, false, &[]);
    let var_info = node("VarFileInfo", &[], 0, true, &[var]);
    node(
        "VS_VERSION_INFO",
        &fixed,
        fixed.len() as u16,
        false,
        &[string_info, var_info],
    )
}
