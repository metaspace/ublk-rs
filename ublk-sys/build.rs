fn main() {
    //let flags = vec!["-I/lib/modules/6.2.11-arch1-1/build/include"];
    let flags = vec!["-I./include"];

    // Generate bindings for xnvme
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .allowlist_type("ublk_.*")
        .allowlist_type("ublksrv_.*")
        .allowlist_var("UBLK_.*")
        .allowlist_var("UBLKSRV_.*")
        .clang_args(flags)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Failed to generate bindings");

    // Write bindings to file
    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Failed to write bindings to file");
}
