fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/ruststrap.ico");
        res.set("ProductName", "Ruststrap");
        res.set("FileDescription", "Ruststrap");
        let _ = res.compile();
    }
}
