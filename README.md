# ScrubNotes

> **Beta.** Expect rough edges. Please report bugs in [Issues](../../issues).

A fast video player for reviewing VODs and footage: scrub, drop timestamped notes, export them.
Written in Rust (egui) on top of libmpv for hardware-accelerated decoding.

## Download

Grab the latest Windows build from [Releases](../../releases), unzip it, and run `scrubnotes.exe`.
Keep `libmpv-2.dll` in the same folder. No install needed.

## Why it's fast

- **Hardware decoding** (NVDEC / D3D11VA / DXVA2, auto-detected; shown in the status bar).
- **Zero-copy display**: mpv renders straight into an OpenGL texture that the UI draws, so frames never leave the GPU.
- **Two-stage scrubbing**: while you drag, cheap keyframe seeks keep up with the mouse. When the cursor rests (~110 ms) or you release, a frame-exact seek lands on the precise frame. Turn on *Playback → Frame-exact scrubbing* to make every position exact.
- **384 MiB seek-back cache** in RAM, so jumping back and forth over recently watched footage rarely touches the disk.
- **Idle at 0% CPU**: the UI only repaints when mpv delivers a frame or you interact. 1080p60 playback is about 1 core.

## Notes

- `M` drops a note at the playhead and focuses the text box. Type, then `Enter` to finish (`Shift+Enter` for a new line).
- `I` / `O` mark a range, then `Alt+M` makes a range note.
- Categories: Note, Highlight, Mistake, Rule break, Question, Clip (`Ctrl+1`…`Ctrl+6` on the selected note).
- Notes auto-save next to the video as `<video>.notes.json`. For URLs they go to `%APPDATA%\ScrubNotes\notes`.
- **Export** (`Ctrl+M`): Text, Markdown, CSV, SRT subtitles, JSON, with timestamps as `HH:MM:SS`, `HH:MM:SS.mmm`, or `HH:MM:SS:FF`.
  Set the **VOD URL** (under *Review details*) and Markdown/CSV/JSON exports link each note straight to that moment
  on Twitch (`?t=1h02m03s`) or YouTube (`&t=3723s`).
- `Ctrl+Shift+E` saves the current frame as a PNG next to the video.

## Keyboard (Premiere Pro layout)

| Keys | Action |
| --- | --- |
| `Space` | Play / stop |
| `J` `K` `L` | Shuttle reverse / stop / forward. Press again for 2x, 4x, 8x |
| `Shift+J` / `Shift+L` | Slow motion (0.25x steps) |
| hold `K` + `J` / `L` | Step one frame |
| `←` / `→` | Step one frame (`Shift` = 5 frames) |
| `↑` / `↓`, `Ctrl+Shift+M` / `Shift+M` | Previous / next note |
| `Home` / `End` | Start / end |
| `I` / `O`, `Shift+I` / `Shift+O` | Mark In / Out, go to In / Out |
| `Ctrl+Shift+X` | Clear In and Out |
| `M`, `Alt+M` | Add note, add range note |
| `Del` | Delete selected note |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `=` `-` `\` | Zoom timeline in / out / fit (mouse wheel zooms around cursor) |
| `` ` `` | Toggle notes panel |
| `F11`, double-click video | Fullscreen |
| `Ctrl+O`, `Ctrl+U` | Open file, open URL (Twitch/YouTube need `yt-dlp` on PATH) |
| `F1` | Shortcut list |

## Building (Windows)

Requirements: Rust (GNU toolchain) and MinGW-w64 on PATH (`choco install mingw`), plus 7-Zip for the fetch script.

```powershell
.\scripts\fetch-libmpv.ps1      # downloads libmpv-2.dll into .\libmpv
cargo run --release -- "D:\vods\run.mp4"
```

`build.rs` copies `libmpv-2.dll` next to the executable. To distribute, ship `scrubnotes.exe` and `libmpv-2.dll` together.

On Linux/macOS, install libmpv from your package manager (`libmpv2`, `mpv`) instead; the app loads it at runtime.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Release builds bundle `libmpv-2.dll` from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake)
([mpv](https://github.com/mpv-player/mpv) + [FFmpeg](https://ffmpeg.org)), which is distributed under the GPL.
Source for that build is available from those projects.
