use assert_cmd::Command;

fn rs_cli() -> Command {
    Command::cargo_bin("rs_cli").unwrap()
}

#[test]
fn detect_unknown_exits_two() {
    let f = std::env::temp_dir().join("rs_cli_unknown.bin");
    std::fs::write(&f, b"not a real magic").unwrap();
    rs_cli().arg("read").arg(&f).assert().code(2);
}

#[test]
fn transform_bin_roundtrip_when_sample_present() {
    let sample = std::path::Path::new("tests/fixtures/sample.bin");
    if !sample.exists() {
        eprintln!("skipping: no sample.bin fixture");
        return;
    }
    let tmp = std::env::temp_dir();
    let text = tmp.join("rt.ritobin");
    let back = tmp.join("rt.bin");
    rs_cli()
        .args([
            "transform",
            sample.to_str().unwrap(),
            text.to_str().unwrap(),
            "--keep-hashed",
        ])
        .assert()
        .success();
    rs_cli()
        .args(["transform", text.to_str().unwrap(), back.to_str().unwrap()])
        .assert()
        .success();
    let a = std::fs::read(sample).unwrap();
    let b = std::fs::read(&back).unwrap();
    assert_eq!(a, b, "bin -> text -> bin must be byte-identical");
}

#[test]
fn bin_diff_identical_is_empty() {
    let sample = std::path::Path::new("tests/fixtures/sample.bin");
    if !sample.exists() {
        eprintln!("skipping: no sample.bin fixture");
        return;
    }
    rs_cli()
        .args([
            "bin",
            "diff",
            sample.to_str().unwrap(),
            sample.to_str().unwrap(),
            "--no-color",
        ])
        .assert()
        .success();
}

#[test]
fn wad_list_when_sample_present() {
    let sample = std::path::Path::new("tests/fixtures/sample.wad.client");
    if !sample.exists() {
        eprintln!("skipping: no sample wad fixture");
        return;
    }
    rs_cli()
        .args(["wad", "list", sample.to_str().unwrap()])
        .assert()
        .success();
}
