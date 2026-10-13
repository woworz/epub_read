fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app.res");
    let status = std::process::Command::new("rc.exe")
        .args(["/nologo", "/I", "assets", "/fo"])
        .arg(&out)
        .arg("assets/app.rc")
        .status()
        .expect("需要 Windows SDK 的 rc.exe，请运行 build.bat");
    assert!(status.success(), "图标资源编译失败");
    println!("cargo:rustc-link-arg={}", out.display());
}
