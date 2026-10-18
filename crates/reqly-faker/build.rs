//! Embeds the vendored faker-js modules: `FILES` maps `faker/<path>` to source.

use std::fs;
use std::path::Path;

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).expect("vendor/faker exists") {
        let path = entry.expect("readable entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            walk(&path, &format!("{prefix}{name}/"), out);
        } else if name.ends_with(".js") {
            out.push((format!("{prefix}{name}"), path.display().to_string()));
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/faker");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, "faker/", &mut files);
    files.sort();
    let mut code = String::from("pub(crate) static FILES: &[(&str, &str)] = &[\n");
    for (name, path) in files {
        code.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    code.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("files.rs");
    fs::write(out, code).expect("OUT_DIR is writable");
}
