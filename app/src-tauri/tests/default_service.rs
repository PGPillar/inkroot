use std::path::PathBuf;

fn script() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../boot-service/inkroot-default-service.sh"
    ))
}

#[test]
fn default_service_is_harmless() {
    let text = std::fs::read_to_string(script()).unwrap();
    assert!(text.starts_with("#!/system/bin/sh"));
    for forbidden in [
        "/sys/power/state",
        "wakealarm",
        "svc ",
        "scaling_max_freq",
        "settings put",
    ] {
        assert!(!text.contains(forbidden), "found {forbidden}");
    }
    assert!(text.contains("/data/local/tmp/service.log"));
    assert!(text.contains("sleep 300"));
}

#[cfg(unix)]
#[test]
fn default_service_parses_with_sh() {
    let st = std::process::Command::new("sh")
        .arg("-n")
        .arg(script())
        .status()
        .unwrap();
    assert!(st.success());
}
