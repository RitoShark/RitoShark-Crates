use std::io::Write;

use assert_cmd::Command;

#[test]
fn tex_info_reads_metadata_without_a_mip_payload() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"TEX\0\x00\x04\x20\x00\x01\x0c\x00\x01")
        .unwrap();
    let output = Command::cargo_bin("rs_cli")
        .unwrap()
        .args(["tex", "info", "--json"])
        .arg(file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let info: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(info["width"], 1024);
    assert_eq!(info["height"], 32);
    assert_eq!(info["format"], "Bc3");
    assert_eq!(info["has_mipmaps"], true);
    assert!(info["mips"].is_null());
}

#[test]
fn tex_info_accepts_dds_without_a_pixel_payload() {
    let image = image::RgbaImage::new(8, 4);
    let bytes = ritoshark::tex::write_dds_bytes(&image).unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes[..128]).unwrap();
    let output = Command::cargo_bin("rs_cli")
        .unwrap()
        .args(["tex", "info", "--json"])
        .arg(file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let info: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(info["container"], "Dds");
    assert_eq!(info["width"], 8);
    assert_eq!(info["height"], 4);
    assert_eq!(info["mips"], 1);
}
