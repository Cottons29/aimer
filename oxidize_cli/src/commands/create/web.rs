use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let web_dir = dir.join("builds/web");
    fs::create_dir_all(&web_dir).unwrap();

    // Generate package.json
    fs::write(
        web_dir.join("package.json"),
        format!(
            r#"{{
  "name": "{}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "vite",
    "build": "vite build"
  }},
  "dependencies": {{}},
  "devDependencies": {{
    "vite": "^5.0.0",
    "typescript": "^5.0.0"
  }}
}}"#,
            project_name
        ),
    )
    .unwrap();

    // Generate vite.config.ts
    fs::write(
        web_dir.join("vite.config.ts"),
        r#"import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    port: 3000
  }
});
"#,
    )
    .unwrap();

    // Generate index.html
    fs::write(
        web_dir.join("index.html"),
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{}</title>
  </head>
  <style>

    html, body {{
      margin: 0;
      padding: 0;
      width: 100%;
      height: 100%;
    }}
    #oxidize_app {{
      width: 100%;
      height: 100%;
      outline: none;
    }}
    </style>
  <body>
    <div id="app"></div>
    <script type="module" src="/main.ts"></script>
  </body>
</html>
"#,
            project_name
        ),
    )
    .unwrap();

    // Generate main.ts
    fs::write(
        web_dir.join("main.ts"),
        format!(
            r#"import init, {{ __oxidize_generated_entrance_point }} from './pkg/{}.js';
// @ts-ignore
async function main() {{
  await init();
  __oxidize_generated_entrance_point();
}}

main().catch((err) => {{}});
"#,
            project_name.replace("-", "_")
        ),
    )
    .unwrap();
}
