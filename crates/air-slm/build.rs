use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LLAMA_CPP_FFI");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_GGML_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_LIB_DIR");
    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-changed=src/llama_cpp_worker.cpp");
    if env::var_os("CARGO_FEATURE_LLAMA_CPP_FFI").is_none() { return; }
    let include=env::var("LLAMA_CPP_INCLUDE_DIR").expect("llama-cpp-ffi requires LLAMA_CPP_INCLUDE_DIR");
    let ggml=env::var("LLAMA_CPP_GGML_INCLUDE_DIR").expect("llama-cpp-ffi requires LLAMA_CPP_GGML_INCLUDE_DIR");
    let lib=env::var("LLAMA_CPP_LIB_DIR").expect("llama-cpp-ffi requires LLAMA_CPP_LIB_DIR");
    let cxx=env::var("CXX").unwrap_or_else(|_|"c++".into());
    let out=PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let exe=out.join("air-llama-cpp-worker");
    let status=Command::new(cxx).args(["-std=c++17","-O2","-pthread"])
        .arg("-I").arg(include).arg("-I").arg(ggml)
        .arg("src/llama_cpp_worker.cpp").arg("-L").arg(&lib).arg(format!("-Wl,-rpath,{}",lib))
        .arg("-llama").arg("-o").arg(&exe).status()
        .expect("failed to invoke C++ compiler for llama.cpp worker");
    assert!(status.success(),"failed to compile llama.cpp FFI worker against the supplied local headers/library");
    println!("cargo:rustc-env=AIR_LLAMA_CPP_WORKER_PATH={}",exe.display());
}
