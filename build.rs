// Windows only: puts the application icon in the .exe (Explorer, taskbar). A missing icon fails the build.
fn main() {
    for f in ["rustyzoo.rc", "packaging/icon/rustyzoo.ico", "build.rs"] {
        println!("cargo:rerun-if-changed={f}");
    }
    #[cfg(windows)]
    embed_resource::compile_for_everything("rustyzoo.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
