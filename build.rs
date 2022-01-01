use std::{env, path::PathBuf};

pub fn main() {
    let libh264bitstream_out = PathBuf::from("./deps/h264bitstream/out")
        .canonicalize()
        .unwrap();
    assert!(
        libh264bitstream_out.is_dir(),
        "{}",
        libh264bitstream_out.display()
    );

    println!(
        "cargo:rustc-link-search=native={}",
        libh264bitstream_out.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=h264bitstream");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        .clang_arg(format!(
            "-I{}",
            libh264bitstream_out.join("include").display()
        ))
        .whitelist_function("h264_new")
        .whitelist_function("h264_free")
        .whitelist_function("find_nal_unit")
        .whitelist_function("read_nal_unit")
        .whitelist_function("write_nal_unit")
        .whitelist_function("rbsp_to_nal")
        .whitelist_function("nal_to_rbsp")
        .whitelist_function("debug_nal")
        .whitelist_type("h264_stream_t")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
