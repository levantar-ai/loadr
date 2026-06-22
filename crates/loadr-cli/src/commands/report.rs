//! `loadr report` — render an HTML report from a summary JSON file.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct ReportArgs {
    /// Summary JSON produced by `loadr run --summary-export`
    pub input: PathBuf,
    /// Output path
    #[arg(short, long, default_value = "loadr-report.html")]
    pub output: PathBuf,
    /// Report format
    #[arg(long, value_enum, default_value_t = ReportFormat::Html)]
    pub format: ReportFormat,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ReportFormat {
    /// Self-contained HTML report with charts
    Html,
    /// JUnit XML (thresholds + checks as testcases) for CI test panels
    Junit,
}

pub fn execute(args: ReportArgs) -> anyhow::Result<i32> {
    let raw = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", args.input.display()))?;
    let summary: loadr_core::Summary = serde_json::from_str(&raw).map_err(|e| {
        anyhow::anyhow!(
            "{} is not a loadr summary export: {e}",
            args.input.display()
        )
    })?;
    let rendered = match args.format {
        ReportFormat::Html => crate::report_html::render(&summary),
        ReportFormat::Junit => summary.render_junit(),
    };
    std::fs::write(&args.output, rendered)?;
    eprintln!("{} wrote {}", "✓".green(), args.output.display());
    Ok(0)
}
