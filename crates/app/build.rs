//! Incorpora icona e informazioni di versione nell'eseguibile Windows.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "SimpleAudioRec")
            .set("FileDescription", "SimpleAudioRec — registrazione audio dei meeting")
            .set("LegalCopyright", "GPL-3.0-or-later");
        if let Err(e) = res.compile() {
            println!("cargo:warning=risorse Windows non incorporate: {e}");
        }
    }
}
