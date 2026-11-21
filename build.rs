// Windows only: compiles the icon, the screensaver's display name and the version info into the .scr
fn main() {
    println!("cargo:rerun-if-changed=assets/hajsaver.rc");
    println!("cargo:rerun-if-changed=assets/hajsaver.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/hajsaver.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
