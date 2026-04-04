use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let web_dir = dir.join("builds/web");
    fs::create_dir_all(&web_dir).unwrap();

    // Generate package.json
    fs::write(
        web_dir.join("package.json"),
        include_str!("../../../templates/web/package.json.template").replace("${package_name}", project_name),
    )
    .unwrap();

    // Generate vite.config.ts
    fs::write(web_dir.join("vite.config.ts"), include_str!("../../../templates/web/vite.config.ts.template")).unwrap();

    // Generate index.html
    fs::write(web_dir.join("index.html"), include_str!("../../../templates/web/index.html.template").replace("${app_title}", project_name))
        .unwrap();

    let wasm_name = project_name.replace("-", "_");
    // Generate main.ts
    fs::write(web_dir.join("main.ts"), include_str!("../../../templates/web/main.ts.template").replace("${wasm_name}", &*wasm_name))
        .unwrap();

    // Default favicon and icons
    fs::write(web_dir.join("favicon.ico"), include_bytes!("../../../templates/icons/favicon.ico")).unwrap();
    fs::write(web_dir.join("apple-touch-icon.png"), include_bytes!("../../../templates/icons/icon_180.png")).unwrap();
}
