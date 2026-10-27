//! Guard probes run out of process so a regression cannot hang the test runner.
use super::*;
#[::core::prelude::v1::test]
fn child() {
    let Ok(mode) = std::env::var("RYNE_LAYOUT_GUARD_CHILD") else {
        return;
    };
    let document = Document::new(
        "| $x$ | b |\n| --- | --- |\n| | $y$ |\n\n$z$",
        SessionServices {
            features: std::sync::Arc::new(
                ryne_features::MarkdownFeatures::new(
                    std::sync::Arc::new(ryne_services::math::FormularyRenderer::default()),
                    vec![],
                )
                .unwrap(),
            ),
            files: std::sync::Arc::new(ryne_services::files::AtomicFileStore),
        },
    );
    let mut display = document.display();
    assert!(display.validate_layout().is_ok());
    match mode.as_str() {
        "table" => display.tables[0].range = 0..0,
        "formula" => display.formulas.swap(1, 2),
        "cell" => display.tables[0].rows[0][0].end = display.text.len(),
        "dimensions" => display.tables[0].source.align.clear(),
        _ => panic!("unexpected guard test mode"),
    }
    assert!(display.validate_layout().is_err());
    for (start, end) in [(0, 0), (3, 2), (0, usize::MAX), (0, 1)] {
        assert!(display.checked_layout_advance(start, end).is_err());
    }
    assert!(display.validate_inline_layout(&(0..usize::MAX)).is_err());
}

#[::core::prelude::v1::test]
fn malformed_consumer_inputs_terminate_in_two_seconds() {
    for mode in ["table", "formula", "cell", "dimensions"] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "layout_guard_tests::child", "--nocapture"])
            .env("RYNE_LAYOUT_GUARD_CHILD", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{mode}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("layout guard timed out: {mode}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
