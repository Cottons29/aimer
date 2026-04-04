use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    fs::create_dir_all(dir.join("builds/windows")).unwrap();
    fs::write(
        dir.join("builds/windows/app.manifest"),
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="com.example.app" type="win32"/>
  <description>Aimer App</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
    )
    .unwrap();

    // Default application icon
    fs::write(dir.join("builds/windows/app.ico"), include_bytes!("../../../templates/icons/app.ico")).unwrap();
}
