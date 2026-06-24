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
