# Local eframe 0.31.1 patch

Source: crates.io eframe 0.31.1, upstream commit
`1669e52a7ccfc3489c1b0999b9ed48894a0b3887` (see `.cargo_vcs_info.json`).
Dual licensed MIT OR Apache-2.0; upstream license texts are included here.

The native redraw scheduler now recomputes control flow on every check. Due
requests for hidden or missing windows are consumed without polling; an empty
queue waits, and future requests retain their deadline. Visible redraws still
request a normal frame. This prevents Win32 hidden windows, which may not receive
WM_PAINT, from leaving the event loop spinning in Poll indefinitely.
Hidden close requests explicitly run one frame, retaining `CancelClose` behavior
without waiting for WM_PAINT, polling, or briefly showing the window.

Changes: `src/native/run.rs`, `mod.rs`, and `repaint_schedule.rs`.
The scheduling helper tests are also included in the application unit tests so
they run with the project's locked dependency set, without resolving optional
eframe backend dependencies. Upstream registry cache files remain unchanged.
