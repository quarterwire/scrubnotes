//! Time formatting. Frame-accurate timecodes keep HH:MM:SS aligned to wall-clock time
//! (rather than non-drop-frame counting) so they match VOD timestamps on Twitch/YouTube.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TcStyle {
    /// 01:02:03
    #[default]
    Hms,
    /// 01:02:03.456
    Millis,
    /// 01:02:03:12
    Frames,
}

impl TcStyle {
    pub const ALL: [TcStyle; 3] = [TcStyle::Hms, TcStyle::Millis, TcStyle::Frames];

    pub fn label(self) -> &'static str {
        match self {
            TcStyle::Hms => "HH:MM:SS",
            TcStyle::Millis => "HH:MM:SS.mmm",
            TcStyle::Frames => "HH:MM:SS:FF",
        }
    }
}

pub fn format(t: f64, style: TcStyle, fps: f64) -> String {
    match style {
        TcStyle::Hms => hms(t),
        TcStyle::Millis => millis(t, '.'),
        TcStyle::Frames => frames(t, fps),
    }
}

fn split(total_secs: u64) -> (u64, u64, u64) {
    (total_secs / 3600, (total_secs / 60) % 60, total_secs % 60)
}

pub fn hms(t: f64) -> String {
    let (h, m, s) = split(t.max(0.0).floor() as u64);
    format!("{h:02}:{m:02}:{s:02}")
}

/// `sep` separates seconds from milliseconds ('.' normally, ',' for SRT).
pub fn millis(t: f64, sep: char) -> String {
    let total_ms = (t.max(0.0) * 1000.0).round() as u64;
    let (h, m, s) = split(total_ms / 1000);
    format!("{h:02}:{m:02}:{s:02}{sep}{:03}", total_ms % 1000)
}

pub fn frames(t: f64, fps: f64) -> String {
    let t = t.max(0.0);
    let fps = if fps > 1.0 { fps } else { 30.0 };
    let secs = (t + 1e-6).floor();
    let max_ff = (fps.ceil() as u64).saturating_sub(1);
    let ff = (((t - secs) * fps + 1e-3).floor().max(0.0) as u64).min(max_ff);
    let (h, m, s) = split(secs as u64);
    format!("{h:02}:{m:02}:{s:02}:{ff:02}")
}

/// Compact label for timeline ruler ticks spaced `step` seconds apart.
pub fn ruler(t: f64, step: f64, fps: f64) -> String {
    if step < 1.0 {
        return frames(t, fps);
    }
    let (h, m, s) = split(t.max(0.0).round() as u64);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Twitch-style `1h02m03s`.
pub fn compact(t: f64) -> String {
    let (h, m, s) = split(t.max(0.0).floor() as u64);
    format!("{h}h{m:02}m{s:02}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(hms(3723.9), "01:02:03");
        assert_eq!(millis(59.9996, '.'), "00:01:00.000");
        assert_eq!(millis(1.5, ','), "00:00:01,500");
        assert_eq!(frames(1.0 + 12.0 / 60.0, 60.0), "00:00:01:12");
        assert_eq!(frames(1.9999, 30.0), "00:00:01:29");
        assert_eq!(frames(10.0 / 29.97, 29.97), "00:00:00:10");
        assert_eq!(compact(3723.0), "1h02m03s");
    }
}
