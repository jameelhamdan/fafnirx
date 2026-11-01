//! The tray's questions with their own button texts are Windows task dialogs (rfd's `common-controls-v6`), which need Common
//! Controls version 6: this puts it in the manifest of the program and of its tests, which link the same code. Without it
//! Windows shows plain Yes / No / Cancel instead, and a program built with the feature does not start.

fn main() {
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") == "windows" && target("CARGO_CFG_TARGET_ENV") == "msvc" {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
