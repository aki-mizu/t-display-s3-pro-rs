use bip39::Language;
use std::{fmt::Write as _, path::PathBuf};

const APP_WINDOW_SOURCE: &str = include_str!("ui/app-window.slint");

fn main() {
    let _ = APP_WINDOW_SOURCE;
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut glyphs = String::new();
    for word in Language::SimplifiedChinese.word_list() {
        for character in word.chars() {
            write!(glyphs, "\\u{{{:x}}}", u32::from(character))
                .expect("write Simplified Chinese glyph coverage");
        }
    }
    std::fs::write(
        out_dir.join("bip39-chinese-glyphs.slint"),
        format!(
            "export component SimplifiedChineseGlyphCoverage inherits Text {{\n    width: 0px;\n    height: 0px;\n    visible: false;\n    text: \"{glyphs}\";\n    font-family: \"Noto Sans CJK SC\";\n    font-size: 20px;\n    font-weight: 400;\n}}\n"
        ),
    )
    .expect("write Simplified Chinese glyph coverage");

    println!("cargo:EMBED_TEXTURES=1");
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=ui/common.slint");
    println!("cargo:rerun-if-changed=ui/fonts/NotoSansSC-BIP39.otf");

    slint_build::compile_with_config(
        manifest_dir.join("ui/app-window.slint"),
        slint_build::CompilerConfiguration::new()
            .with_include_paths(vec![out_dir])
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
            .with_sdf_fonts(true),
    )
    .expect("Slint build failed");
}
