//! Liga a libjsonw da plataforma (terceiros/jsonw/lib/<plataforma>/libjsonw.a), sem modificação.
//! `JSONW_LIB_DIR` troca a pasta (por exemplo, uma libjsonw instalada no sistema).

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=JSONW_LIB_DIR");
    let pasta = if let Ok(pasta) = env::var("JSONW_LIB_DIR") {
        PathBuf::from(pasta)
    } else {
        let sistema = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let arquitetura = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let plataforma = match (sistema.as_str(), arquitetura.as_str()) {
            ("linux", "x86_64") => "linux-x86_64",
            ("linux", "aarch64") => "linux-arm64",
            ("macos", "aarch64") => "macos-arm64",
            ("windows", "x86_64") => "windows-x86_64",
            outra => panic!(
                "a libjsonw não tem pacote para {outra:?}: defina JSONW_LIB_DIR com a pasta da libjsonw.a"
            ),
        };
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo define"))
            .join("../../terceiros/jsonw/lib")
            .join(plataforma)
    };
    println!("cargo:rustc-link-search=native={}", pasta.display());
    println!("cargo:rustc-link-lib=static=jsonw");
    println!(
        "cargo:rerun-if-changed={}",
        pasta.join("libjsonw.a").display()
    );
    // a libjsonw usa a libm e as threads do sistema
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
}
