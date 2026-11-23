// Windows: the icon of finder.exe is the one of the app
fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../examples/app_icon/finder.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../examples/app_icon/finder.ico");
        res.set("ProductName", "Finder").set("FileDescription", "Finder");
        if let Err(err) = res.compile() {
            println!("cargo:warning=icon of the exe not embedded: {err}");
        }
    }
}
