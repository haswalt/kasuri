use std::{env, fs, path::PathBuf};
fn main() {
    let root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../examples/workflows");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    let mut code = String::from("pub const EXAMPLES: &[(&str,&str)] = &[\n");
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        code.push_str(&format!(
            "({:?},include_str!({:?})),\n",
            path.file_stem().unwrap().to_str().unwrap(),
            path.to_str().unwrap()
        ));
    }
    code.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("examples.rs"),
        code,
    )
    .unwrap();
}
