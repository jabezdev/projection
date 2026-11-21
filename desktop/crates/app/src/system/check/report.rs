use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Ok => "OK  ",
            Level::Info => "    ",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
        }
    }
}

#[derive(Default)]
pub struct Report {
    sections: Vec<(String, Vec<(Level, String)>)>,
}

impl Report {
    pub fn section(&mut self, name: &str) {
        self.sections.push((name.to_string(), Vec::new()));
    }

    pub fn add(&mut self, level: Level, text: impl Into<String>) {
        if self.sections.is_empty() {
            self.section("General");
        }
        self.sections.last_mut().expect("a section exists").1.push((level, text.into()));
    }

    pub fn ok(&mut self, text: impl Into<String>) {
        self.add(Level::Ok, text);
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.add(Level::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.add(Level::Warn, text);
    }

    pub fn fail(&mut self, text: impl Into<String>) {
        self.add(Level::Fail, text);
    }

    pub fn problems(&self) -> Vec<&str> {
        self.sections
            .iter()
            .flat_map(|(_, lines)| lines)
            .filter(|(level, _)| matches!(level, Level::Warn | Level::Fail))
            .map(|(_, text)| text.as_str())
            .collect()
    }

    pub fn render(&self, header: &[String]) -> String {
        let mut out = String::new();
        for line in header {
            let _ = writeln!(out, "{line}");
        }
        for (name, lines) in &self.sections {
            let _ = writeln!(out, "\n== {name} ==");
            for (level, text) in lines {
                let _ = writeln!(out, "  {} {text}", level.tag());
            }
        }
        let problems = self.problems();
        let _ = writeln!(out, "\n== Summary ==");
        if problems.is_empty() {
            let _ = writeln!(out, "  Nothing needs attention.");
        }
        for problem in problems {
            let _ = writeln!(out, "  - {problem}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_lists_warnings_and_failures_only() {
        let mut report = Report::default();
        report.section("Computer");
        report.ok("8 GB memory");
        report.warn("Only one display");
        report.info("Driver 1.2");
        report.fail("No audio output");
        assert_eq!(report.problems(), ["Only one display", "No audio output"]);
        let text = report.render(&["PALTO system check".into()]);
        assert!(text.starts_with("PALTO system check\n\n== Computer ==\n  OK   8 GB memory\n  WARN Only one display\n"));
        assert!(text.ends_with("== Summary ==\n  - Only one display\n  - No audio output\n"));
    }
}
