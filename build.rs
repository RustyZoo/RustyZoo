// Windows only: puts the application icon in the .exe (Explorer, taskbar). A missing icon fails the build.
fn main() {
    #[cfg(windows)]
    embed_resource::compile_for_everything("rustyzoo.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
