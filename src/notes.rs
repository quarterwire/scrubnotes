//! Review notes: the data model, sidecar persistence and export formats.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::timecode::{self, TcStyle};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    #[default]
    Note,
    Highlight,
    Mistake,
    RuleBreak,
    Question,
    Clip,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Note,
        Category::Highlight,
        Category::Mistake,
        Category::RuleBreak,
        Category::Question,
        Category::Clip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Note => "Note",
            Category::Highlight => "Highlight",
            Category::Mistake => "Mistake",
            Category::RuleBreak => "Rule break",
            Category::Question => "Question",
            Category::Clip => "Clip",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Category::Note => Color32::from_rgb(0xe8, 0xc5, 0x47),
            Category::Highlight => Color32::from_rgb(0x4c, 0xc2, 0x6a),
            Category::Mistake => Color32::from_rgb(0xf0, 0x8a, 0x3c),
            Category::RuleBreak => Color32::from_rgb(0xe5, 0x48, 0x4d),
            Category::Question => Color32::from_rgb(0x4d, 0xa3, 0xff),
            Category::Clip => Color32::from_rgb(0xb0, 0x7c, 0xf0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    /// Seconds from the start of the video.
    pub start: f64,
    /// End of a range note; `None` for a point marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default)]
    pub category: Category,
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Review {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub video: String,
    /// Online VOD URL; when set, exports link each timestamp straight to that moment.
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl Review {
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut review: Review =
            serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        review.sort();
        Ok(review)
    }

    /// Writes to a temporary file first so a crash mid-write never truncates existing notes.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }

    pub fn sort(&mut self) {
        self.notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.id.cmp(&b.id)));
    }

    pub fn next_id(&self) -> u64 {
        self.notes.iter().map(|n| n.id).max().map_or(1, |m| m + 1)
    }
}

/// `clip.mp4` → `clip.mp4.notes.json`, kept next to the video.
pub fn sidecar_path(video: &Path) -> PathBuf {
    let mut s = video.as_os_str().to_owned();
    s.push(".notes.json");
    PathBuf::from(s)
}

/// Notes for streamed URLs live in the per-user data directory.
pub fn url_notes_path(url: &str) -> PathBuf {
    let mut name: String = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
        .collect();
    name.truncate(150);
    data_dir().join("notes").join(format!("{name}.notes.json"))
}

pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("ScrubNotes")
}

/// Appends a start-time parameter to a Twitch or YouTube (or similar) VOD URL.
pub fn timestamp_url(base: &str, t: f64) -> Option<String> {
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    let (path, query) = base.split_once('?').unwrap_or((base, ""));
    let mut params: Vec<&str> = query.split('&').filter(|p| !p.is_empty() && !p.starts_with("t=")).collect();
    let stamp = if base.contains("twitch.tv") {
        format!("t={}", timecode::compact(t))
    } else {
        format!("t={}s", t.max(0.0).floor() as u64)
    };
    params.push(&stamp);
    Some(format!("{path}?{}", params.join("&")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Text,
    Markdown,
    Csv,
    Srt,
    Json,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] =
        [ExportFormat::Text, ExportFormat::Markdown, ExportFormat::Csv, ExportFormat::Srt, ExportFormat::Json];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Text => "Text",
            ExportFormat::Markdown => "Markdown",
            ExportFormat::Csv => "CSV",
            ExportFormat::Srt => "SRT subtitles",
            ExportFormat::Json => "JSON",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Text => "txt",
            ExportFormat::Markdown => "md",
            ExportFormat::Csv => "csv",
            ExportFormat::Srt => "srt",
            ExportFormat::Json => "json",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub timecode: TcStyle,
    pub include_category: bool,
    /// Only export notes starting inside this range (the In/Out marks).
    pub range: Option<(f64, f64)>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { format: ExportFormat::Text, timecode: TcStyle::Hms, include_category: true, range: None }
    }
}

pub fn export(review: &Review, opts: &ExportOptions) -> String {
    let notes: Vec<&Note> = review
        .notes
        .iter()
        .filter(|n| opts.range.is_none_or(|(a, b)| n.start >= a && n.start <= b))
        .collect();
    let tc = |t: f64| timecode::format(t, opts.timecode, review.fps);
    let span = |n: &Note| match n.end {
        Some(end) => format!("{} - {}", tc(n.start), tc(end)),
        None => tc(n.start),
    };
    let title = if review.title.trim().is_empty() { review.video.as_str() } else { review.title.trim() };
    let mut out = String::new();

    match opts.format {
        ExportFormat::Text => {
            for n in notes {
                let text = n.text.trim().replace('\n', " / ");
                if opts.include_category {
                    let _ = writeln!(out, "{} [{}] {}", span(n), n.category.label(), text);
                } else {
                    let _ = writeln!(out, "{} {}", span(n), text);
                }
            }
        }
        ExportFormat::Markdown => {
            let _ = writeln!(out, "# {}\n", if title.is_empty() { "Review notes" } else { title });
            if !review.source_url.trim().is_empty() {
                let _ = writeln!(out, "Source: <{}>\n", review.source_url.trim());
            }
            for n in notes {
                let stamp = match timestamp_url(&review.source_url, n.start) {
                    Some(url) => format!("[{}]({url})", span(n)),
                    None => span(n),
                };
                let mut lines = n.text.trim().lines();
                let first = lines.next().unwrap_or("");
                if opts.include_category {
                    let _ = write!(out, "- **{stamp}** · *{}* — {first}", n.category.label());
                } else {
                    let _ = write!(out, "- **{stamp}** — {first}");
                }
                for line in lines {
                    let _ = write!(out, "  \n  {line}");
                }
                out.push('\n');
            }
        }
        ExportFormat::Csv => {
            out.push_str("start,end,start_seconds,end_seconds,category,note,link\n");
            for n in notes {
                let fields = [
                    tc(n.start),
                    n.end.map(tc).unwrap_or_default(),
                    format!("{:.3}", n.start),
                    n.end.map(|e| format!("{e:.3}")).unwrap_or_default(),
                    n.category.label().to_string(),
                    n.text.trim().to_string(),
                    timestamp_url(&review.source_url, n.start).unwrap_or_default(),
                ];
                let row: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
                let _ = writeln!(out, "{}", row.join(","));
            }
        }
        ExportFormat::Srt => {
            for (i, n) in notes.iter().enumerate() {
                let next_start = notes.get(i + 1).map(|m| m.start);
                let end = n.end.unwrap_or_else(|| {
                    let default = n.start + 3.0;
                    next_start.filter(|&s| s > n.start + 0.5).map_or(default, |s| s.min(default))
                });
                let text = if opts.include_category {
                    format!("[{}] {}", n.category.label(), n.text.trim())
                } else {
                    n.text.trim().to_string()
                };
                let _ = writeln!(
                    out,
                    "{}\n{} --> {}\n{}\n",
                    i + 1,
                    timecode::millis(n.start, ','),
                    timecode::millis(end, ','),
                    text
                );
            }
        }
        ExportFormat::Json => {
            #[derive(Serialize)]
            struct Exported<'a> {
                start: f64,
                end: Option<f64>,
                start_tc: String,
                end_tc: Option<String>,
                category: &'a str,
                text: &'a str,
                link: Option<String>,
            }
            #[derive(Serialize)]
            struct Doc<'a> {
                title: &'a str,
                video: &'a str,
                source_url: &'a str,
                notes: Vec<Exported<'a>>,
            }
            let doc = Doc {
                title,
                video: &review.video,
                source_url: review.source_url.trim(),
                notes: notes
                    .iter()
                    .map(|n| Exported {
                        start: n.start,
                        end: n.end,
                        start_tc: tc(n.start),
                        end_tc: n.end.map(tc),
                        category: n.category.label(),
                        text: n.text.trim(),
                        link: timestamp_url(&review.source_url, n.start),
                    })
                    .collect(),
            };
            out = serde_json::to_string_pretty(&doc).unwrap_or_default();
        }
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(
            timestamp_url("https://www.twitch.tv/videos/123?t=0h1m0s", 3723.4).unwrap(),
            "https://www.twitch.tv/videos/123?t=1h02m03s"
        );
        assert_eq!(
            timestamp_url("https://www.youtube.com/watch?v=abc", 83.9).unwrap(),
            "https://www.youtube.com/watch?v=abc&t=83s"
        );
        assert_eq!(timestamp_url("https://youtu.be/abc", 5.0).unwrap(), "https://youtu.be/abc?t=5s");
        assert!(timestamp_url("  ", 5.0).is_none());
    }

    #[test]
    fn exports() {
        let review = Review {
            video: "run.mp4".into(),
            fps: 60.0,
            notes: vec![
                Note { id: 1, start: 83.0, end: None, category: Category::Mistake, text: "Missed, \"jump\"".into() },
                Note { id: 2, start: 130.0, end: Some(165.5), category: Category::Highlight, text: "Combo".into() },
            ],
            ..Default::default()
        };
        let text = export(&review, &ExportOptions::default());
        assert_eq!(text, "00:01:23 [Mistake] Missed, \"jump\"\n00:02:10 - 00:02:45 [Highlight] Combo\n");
        let csv = export(&review, &ExportOptions { format: ExportFormat::Csv, ..Default::default() });
        assert!(csv.contains("\"Missed, \"\"jump\"\"\""));
        let srt = export(&review, &ExportOptions { format: ExportFormat::Srt, ..Default::default() });
        assert!(srt.starts_with("1\n00:01:23,000 --> 00:01:26,000\n[Mistake]"));
        let ranged = export(&review, &ExportOptions { range: Some((100.0, 200.0)), ..Default::default() });
        assert!(!ranged.contains("Mistake"));
    }
}
