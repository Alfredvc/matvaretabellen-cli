use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=data");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let data_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("data");

    compress_tree(&data_dir, &out_dir).expect("compress data tree");

    let version = fs::read_to_string(data_dir.join("VERSION"))
        .expect("data/VERSION missing")
        .trim()
        .to_string();
    fs::write(out_dir.join("data_version.txt"), &version).unwrap();
    println!("cargo:rustc-env=MVT_DATA_VERSION={version}");
}

fn compress_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let rel = dst.join(&name);
        if path.is_dir() {
            fs::create_dir_all(&rel)?;
            compress_tree(&path, &rel)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let bytes = fs::read(&path)?;
            let compressed = zstd::encode_all(bytes.as_slice(), 19)?;
            let out_path = rel.with_extension("json.zst");
            fs::write(out_path, compressed)?;
        }
    }
    Ok(())
}
