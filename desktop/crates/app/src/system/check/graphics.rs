use super::report::Report;

const FRAME_MS: f64 = 1000.0 / 60.0;

pub fn check(report: &mut Report) {
    report.section("Graphics");
    let adapters = render_engine::benchmark::adapters();
    if adapters.is_empty() {
        return report.fail("No GPU adapter found");
    }
    for adapter in adapters {
        report.info(adapter);
    }
    match render_engine::benchmark::effects_and_shader((1920, 1080), 120) {
        Ok(bench) => {
            let line = format!(
                "1080p lyrics with shadow, outline, glow and the Aurora background on {}: {:.1} ms a frame, worst {:.1} ms",
                bench.adapter, bench.mean_ms, bench.worst_ms
            );
            if bench.mean_ms > FRAME_MS * 0.6 {
                report.warn(format!("{line}; effects may drop frames at 60 fps"));
            } else {
                report.ok(line);
            }
        }
        Err(e) => report.fail(format!("Rendering test failed: {e:#}")),
    }
}
