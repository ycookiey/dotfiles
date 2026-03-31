fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dotfiles = std::path::Path::new(&manifest).parent().unwrap();
    println!("cargo:rustc-env=DOTFILES_DIR={}", dotfiles.display());
}
