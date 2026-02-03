fn main() {
    // Only embed Windows resources on Windows targets
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "Lo-phi Feature Reduction Tool");
        res.set("ProductName", "Lo-phi");

        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resources: {}", e);
        }
    }
}
