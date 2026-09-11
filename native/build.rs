//! Embeds the Windows icon and version info into musika.exe.
//!
//! Without this the executable carries no resources at all and Explorer, the
//! taskbar and Alt-Tab all fall back to the generic "unknown program" icon -
//! setting the icon on a shortcut only fixes the shortcut, not the exe. There
//! is no way to do this from Rust alone: an icon has to go in as a Win32
//! resource, which means invoking a resource compiler, which is what
//! `winresource` wraps. It is a build dependency only and adds nothing to the
//! shipped binary beyond the resource itself.

fn main() {
    println!("cargo:rerun-if-changed=../icons/musika.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../icons/musika.ico");
        res.set("ProductName", "Musika");
        res.set("FileDescription", "Musika - a seven-chord organ");
        res.set("LegalCopyright", "Cameron Crow");

        // A missing resource compiler should not stop the build - you get a
        // working instrument with a plain icon, and a warning saying why.
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
