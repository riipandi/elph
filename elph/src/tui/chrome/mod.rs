//! Top and mid chrome: header, status row, live stats.

mod fit;
mod header;
mod stats;
mod status_row;

pub use fit::{
    chrome_footer_widths, chrome_half_width, fit_editor_border_project, fit_footer_status_left,
    fit_footer_status_right, fit_footer_status_right_with_select, footer_mode_model_width,
};
pub use header::Header;
pub use stats::ChromeStats;
pub use stats::{chrome_stats_from_session, count_user_turns, read_git_footer_info, refresh_chrome_stats};
pub use status_row::StatusRow;
pub use status_row::format_elapsed_secs;
