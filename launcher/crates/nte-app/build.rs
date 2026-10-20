fn main() {
    println!("cargo:rerun-if-changed=../../ui/dist");
    tauri_build::build()
}
