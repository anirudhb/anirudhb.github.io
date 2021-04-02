use std::path::Path;
use syntect::highlighting::ThemeSet;

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=themes");
    let dest_path = Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("themes.themedump");
    let mut ts = ThemeSet::new();
    ts.add_from_folder("themes")?;
    syntect::dumps::dump_to_file(&ts, dest_path)?;
    Ok(())
}
