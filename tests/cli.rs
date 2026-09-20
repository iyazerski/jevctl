use std::process::{Command, Stdio};

fn jevctl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jevctl"))
}

#[test]
fn validate_is_silent_on_success() {
    let output = jevctl()
        .args(["validate", "tests/fixtures/all-kinds.json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_request_uses_input_exit_code() {
    let mut child = jevctl()
        .arg("validate")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(
        child.stdin.take().unwrap(),
        &serde_json::json!({"context": "x", "questions": {}}),
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "jevctl: questions must not be empty\n"
    );
}

#[test]
fn missing_key_fails_before_evaluation() {
    let output = jevctl()
        .args(["evaluate", "tests/fixtures/all-kinds.json"])
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "jevctl: TYPESAFE_API_KEY is not set\n"
    );
}

#[test]
fn help_does_not_expose_provider_contract() {
    let output = jevctl().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap().to_lowercase();
    for hidden in ["jev-latest", "noul", "choice", "score", "systemone"] {
        assert!(!help.contains(hidden), "help leaked {hidden}: {help}");
    }
}
