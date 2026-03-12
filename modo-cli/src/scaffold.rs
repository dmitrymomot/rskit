use anyhow::{Context, Result};
use include_dir::Dir;
use minijinja::Environment;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn scaffold(
    target_dir: &Path,
    template_dir: &Dir<'static>,
    shared_dir: &Dir<'static>,
    context: &HashMap<&str, &str>,
) -> Result<()> {
    let env = Environment::new();

    // Process shared files first, then template-specific files
    write_dir(&env, shared_dir, target_dir, context, Path::new(""))?;
    write_dir(&env, template_dir, target_dir, context, Path::new(""))?;

    Ok(())
}

fn write_dir(
    env: &Environment,
    dir: &Dir<'static>,
    target_dir: &Path,
    context: &HashMap<&str, &str>,
    prefix: &Path,
) -> Result<()> {
    for file in dir.files() {
        let file_name = file.path().file_name().expect("include_dir path missing file_name");
        let relative = prefix.join(file_name);
        let relative_str = relative.to_string_lossy();

        let content = file
            .contents_utf8()
            .with_context(|| format!("non-UTF8 template: {relative_str}"))?;

        // Render through MiniJinja
        let rendered = env
            .render_str(
                content,
                minijinja::context! {
                    project_name => context["project_name"],
                    db_driver => context["db_driver"],
                },
            )
            .with_context(|| format!("render failed: {relative_str}"))?;

        // Skip files that render to empty (conditional files like docker-compose for sqlite)
        // but only for .jinja files (don't skip intentionally empty files like .gitkeep)
        if relative_str.ends_with(".jinja") && rendered.trim().is_empty() {
            continue;
        }

        // Strip .jinja extension
        let out_path = relative_str
            .strip_suffix(".jinja")
            .unwrap_or(&relative_str)
            .to_string();

        let dest = target_dir.join(&out_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir: {}", parent.display()))?;
        }
        fs::write(&dest, rendered)
            .with_context(|| format!("write file: {}", dest.display()))?;
    }

    // Recurse into subdirectories with updated prefix
    for subdir in dir.dirs() {
        let dir_name = subdir.path().file_name().expect("include_dir path missing file_name");
        let sub_prefix = prefix.join(dir_name);
        write_dir(env, subdir, target_dir, context, &sub_prefix)?;
    }

    Ok(())
}
