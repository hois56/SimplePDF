fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Cargo binaries look for PDFium beside the executable (also works from the installed source tree).
        let local = std::path::Path::new("native/pdfium/pdfium.dll");
        let pdfium = std::env::var_os("SIMPLEPDF_PDFIUM")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                if local.exists() {
                    local.into()
                } else {
                    "../pdfium.dll".into()
                }
            });
        println!("cargo:rerun-if-env-changed=SIMPLEPDF_PDFIUM");
        println!("cargo:rerun-if-changed={}", pdfium.display());
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let dst = out.ancestors().nth(3).unwrap().join("pdfium.dll");
        let data = std::fs::read(&pdfium).expect("native/pdfium/pdfium.dll");
        if std::fs::read(&dst).ok().as_deref() != Some(&data) {
            std::fs::write(dst, data).expect("stage PDFium");
        }
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico")
            .set("FileDescription", "SimplePDF")
            .set("ProductName", "SimplePDF");
        res.compile().expect("SimplePDF icon and version resources");
    }
}
