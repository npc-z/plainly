//! Why a window action could not be done, in the interface's language.
//!
//! Nothing here carries an exit code: every failure is destined for a Chinese
//! sentence in the window, and the core's own message travels after the colon.
//! Core words a failure precisely, so re-wording it would leave the person with
//! less than the CLI gets, and `Store`'s `Display` is the same sentence
//! `plainly history` prints.

use std::path::PathBuf;

/// A failed window action.
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("读不了历史库：{0}")]
    Store(#[from] plainly_core::StoreError),
    #[error("读不了配置文件：{0}")]
    Config(#[from] plainly_core::ConfigError),
    #[error("找不到数据目录：{0}")]
    Paths(#[from] plainly_core::PathError),
    /// A setting that Plainly will not write, with the reason in words.
    #[error("{message}")]
    Value { message: String },
    #[error("写不进 {path}：{source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("复制不到剪贴板：{message}")]
    Clipboard { message: String },
}
