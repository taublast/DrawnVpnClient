use std::{env, fs, path::Path};

// Windows: the exe's icon (resource 1), shown by Explorer, the window and the taskbar.
// Desktop: the assets go next to the exe, where drawnui reads them first, so the app also runs
// when it is started from another folder. Linux reads the window icon from icon.ico next to the exe.
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=assets");
    // The build machine runs this script: look at the target, not at the host. The web build
    // takes its assets and favicon from web.ps1 / web.sh, Android from the APK.
    let target = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target == "emscripten" || target == "android" {
        return;
    }
    if target == "ios" {
        // The iOS side (ios/app/glue.m), compiled with Xcode's clang and linked into the app.
        // The bundle with its assets is made by ios/ios.sh.
        apple_glue(&env::var("OUT_DIR").unwrap(), false);
        return;
    }
    if target == "macos" {
        // The same glue for macOS (AppKit, the system extension); the bundle is made by mac/mac.sh.
        apple_glue(&env::var("OUT_DIR").unwrap(), true);
    }
    if target == "windows" {
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
    // OUT_DIR is target/<profile>/build/<package>-<hash>/out; the exe is in target/<profile>.
    let out = env::var("OUT_DIR").unwrap();
    let exe_folder = Path::new(&out).ancestors().nth(3).unwrap();
    copy_folder(Path::new("assets"), &exe_folder.join("assets"));
    if target == "linux" {
        fs::copy("icon.ico", exe_folder.join("icon.ico")).unwrap();
    }
}

fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_folder(&source, &target);
        } else {
            fs::copy(&source, &target).unwrap();
        }
    }
}

fn apple_glue(out: &str, macos: bool) {
    use std::process::Command;
    println!("cargo:rerun-if-changed=ios/app/glue.m");
    // Another Xcode (SDK) relinks the app.
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    let rust_target = env::var("TARGET").unwrap_or_default();
    let sim = rust_target.ends_with("-sim");
    let arch = if rust_target.starts_with("x86_64") { "x86_64" } else { "arm64" };
    let clang_target = if macos { format!("{arch}-apple-macos13.0") } else if sim { "arm64-apple-ios14.0-simulator".into() } else { "arm64-apple-ios14.0".into() };
    let (sdk, target) = (if macos { "macosx" } else if sim { "iphonesimulator" } else { "iphoneos" }, clang_target.as_str());
    let object = Path::new(out).join("glue.o");
    let library = Path::new(out).join("libnetconnectglue.a");
    let ok = Command::new("xcrun")
        .args(["--sdk", sdk, "clang", "-c", "-fobjc-arc", "-O2", "-target", target, "ios/app/glue.m", "-o"])
        .arg(&object)
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok, "clang failed on ios/app/glue.m");
    let _ = fs::remove_file(&library);
    let ok = Command::new("xcrun").args(["ar", "rcs"]).arg(&library).arg(&object).status().is_ok_and(|s| s.success());
    assert!(ok, "ar failed");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=netconnectglue");
    let frameworks: &[&str] = if macos { &["NetworkExtension", "SystemExtensions", "AppKit", "Foundation"] } else { &["NetworkExtension", "UIKit", "Foundation"] };
    for framework in frameworks {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
