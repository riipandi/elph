//! Top and mid chrome: header, status row, live stats.

mod fit;
mod header;
mod stats;
mod status_row;

pub use fit::{chrome_half_width, fit_footer_left, fit_footer_right};
pub use header::Header;
pub use stats::ChromeStats;
pub use stats::{read_git_footer_info, refresh_chrome_stats};
pub use status_row::StatusRow;
pub use status_row::format_elapsed_secs;
