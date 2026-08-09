const APP_WINDOW_SOURCE: &str = include_str!("ui/app-window.slint");

fn main() {
    let _ = APP_WINDOW_SOURCE;
    println!("cargo:EMBED_TEXTURES=1");
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=ui/common.slint");

    slint_build::compile_with_config(
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("ui/app-window.slint"),
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("Slint build failed");
}
