// Build script: embeds Windows icon resource and handles platform-specific setup.
fn main() {
    // Embed the Windows icon as a resource so it appears in Explorer and the taskbar.
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile()
            .expect("Failed to embed Windows icon resource");
    }
}
