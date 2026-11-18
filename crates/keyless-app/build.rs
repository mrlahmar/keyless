fn main() {
    // Translations: lang/<language>/LC_MESSAGES/keyless.po, bundled into the executable.
    // Strings have no per-component context, so each one is translated once.
    println!("cargo:rerun-if-changed=lang");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("UI compiles");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}

#[cfg(windows)]
fn windows_resources() {
    println!("cargo:rerun-if-changed=../../assets/keyless.ico");
    println!("cargo:rerun-if-changed=keyless.manifest");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/keyless.ico")
        .set_manifest_file("keyless.manifest")
        .set("ProductName", "Keyless")
        .set("FileDescription", "Keyless")
        .set("LegalCopyright", "MIT License");
    if let Err(error) = res.compile() {
        // Release builds in CI always have a resource compiler. Local GNU builds without one
        // still produce a working executable, just without icon and version info.
        if std::env::var("KEYLESS_REQUIRE_RESOURCES").is_ok() {
            panic!("resource compilation failed: {error}");
        }
        println!("cargo:warning=building without icon and version info: {error}");
    }
}

#[cfg(not(windows))]
fn windows_resources() {}
