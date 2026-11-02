fn main() {
    println!("cargo:rerun-if-changed=resources/fonts/OFL.txt");
    println!("cargo:rerun-if-changed=resources/dunyu.rc");
    println!("cargo:rerun-if-changed=resources/dunyu.manifest");
    println!("cargo:rerun-if-changed=resources/dunyu.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/dunyu.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Windows visual styles manifest must be embedded");
    }
}
