use clawpal::runtime::zeroclaw::sanitize::sanitize_output;

#[test]
fn sanitize_removes_ansi_and_runtime_info_lines() {
    let raw =
        "[2m2026-02-25T08:04:09.490132Z[0m [32m INFO[0m zeroclaw::config::schema\nFinal answer";
    let out = sanitize_output(raw);
    assert_eq!(out, "Final answer");
}
