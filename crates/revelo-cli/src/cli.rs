use clap::{Parser, Subcommand};
use revelo_core::StreamKind;

const BANNER: &str = include_str!("banner.txt");

#[derive(Parser)]
#[command(
    name = "revelo",
    version,
    about = None,
    long_about = None,
    before_help = BANNER,
    args_conflicts_with_subcommands = true,
)]
pub(crate) struct Cli {
    /// File path to analyze
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    /// XML output
    #[arg(short = 'x', long)]
    pub xml: bool,

    /// JSON output
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Text output (default)
    #[arg(short = 't', long)]
    pub text: bool,

    /// Demux level
    #[arg(
        short = 'd',
        long = "demux",
        value_name = "LEVEL",
        default_value = "frame",
        value_parser = ["frame", "container", "elementary"],
    )]
    pub demux: String,

    /// Trace verbosity (0-9)
    #[arg(
        short = 'r',
        long = "trace",
        value_name = "N",
        default_value = "0",
        value_parser = clap::builder::PossibleValuesParser::new(
            ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"],
        ),
    )]
    pub trace: String,

    /// Scan companion files (BDMV M2TS, sidecar subtitles)
    #[arg(short = 'm', long)]
    pub multi_file: bool,

    /// Video streams only
    #[arg(long)]
    pub video_only: bool,

    /// Audio streams only
    #[arg(long)]
    pub audio_only: bool,

    /// Select specific streams by kind and index (e.g. 0:1 for General=0,
    /// Video=1). May be repeated.
    #[arg(long, value_name = "KIND:INDEX", value_parser = parse_stream_selector)]
    pub stream: Vec<(StreamKind, usize)>,

    /// Print structural integrity information
    #[arg(long)]
    pub verify: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Parse a `"KIND:INDEX"` string into a `(StreamKind, usize)` pair.
fn parse_stream_selector(s: &str) -> Result<(StreamKind, usize), String> {
    let (kind_str, idx_str) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid stream selector '{s}': expected KIND:INDEX (e.g. 0:1)"))?;
    let kind = match kind_str {
        "0" | "General" => StreamKind::General,
        "1" | "Video" => StreamKind::Video,
        "2" | "Audio" => StreamKind::Audio,
        "3" | "Text" => StreamKind::Text,
        "4" | "Other" => StreamKind::Other,
        "5" | "Image" => StreamKind::Image,
        "6" | "Menu" => StreamKind::Menu,
        _ => {
            return Err(format!(
                "unknown stream kind '{kind_str}': use 0-6 or General/Video/Audio/Text/Other/Image/Menu"
            ));
        }
    };
    let index: usize = idx_str.parse().map_err(|_| {
        format!("invalid stream index '{idx_str}': expected a non-negative integer")
    })?;
    Ok((kind, index))
}

/// Future subcommands (inspect, diff, batch, verify, extract)
#[derive(Subcommand)]
pub(crate) enum Command {
    /// Inspect a media file
    Inspect {
        /// File path to analyze
        path: Option<String>,
    },
}
