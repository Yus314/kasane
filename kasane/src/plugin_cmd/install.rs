use std::path::Path;

use anyhow::Result;

pub fn run(path: Option<&str>) -> Result<()> {
    let package_path = match path {
        Some(path) if Path::new(path).extension().and_then(|ext| ext.to_str()) == Some("kpk") => {
            Path::new(path).to_path_buf()
        }
        Some(project_dir) => {
            super::package_artifact::build_project_package(project_dir, true)?.path
        }
        None => super::package_artifact::build_project_package(".", true)?.path,
    };

    let installed = super::package_artifact::install_package_file(&package_path)?;
    let size = std::fs::metadata(&installed.path)?.len();
    println!(
        "Installed package: {}@{}",
        installed.inspected.header.package.name, installed.inspected.header.package.version
    );
    println!("Plugin: {}", installed.inspected.header.plugin.id);
    println!("State: active");
    println!("File: {} ({} KiB)", installed.path.display(), size / 1024);
    println!("Lock: {}", installed.lock_path.display());

    Ok(())
}
