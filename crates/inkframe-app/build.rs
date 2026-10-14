//! Gives the Windows executable its icon and version details.
//!
//! Windows reads an executable's icon from resources compiled into the file
//! itself (unlike macOS, where it sits beside the program in the app bundle).
//! Explorer, the taskbar and Start menu shortcuts all show it from there.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // Build scripts run on the machine doing the build, so check the
    // *target* OS: this is what decides whether resources are needed.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo::rerun-if-changed=../../assets/icon/inkframe.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/icon/inkframe.ico");
        resource.set("ProductName", "Inkframe");
        resource.set("FileDescription", "Inkframe animation editor");
        resource.compile().expect("couldn't embed the Windows icon");
    }
}
