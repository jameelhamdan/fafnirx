//! The web app is embedded from `frontend/dist`. Without a build of it (a fresh checkout that only
//! builds the master) a placeholder page keeps the compile working and says what is missing.

fn main() {
    let dist = std::path::Path::new("../../frontend/dist");
    if !dist.join("index.html").is_file() {
        std::fs::create_dir_all(dist).expect("create frontend/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>fafnirx</title><p>The web app is not built. Run <code>npm run build</code> in <code>frontend/</code> and rebuild the master.</p>",
        )
        .expect("write placeholder page");
    }
    println!("cargo:rerun-if-changed=../../frontend/dist");
}
