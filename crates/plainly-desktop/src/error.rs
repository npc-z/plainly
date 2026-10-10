//! Why a window action could not be done, in the interface's language.
//!
//! The window's errors are not the CLI's: nothing here carries an exit code, and
//! everything here is destined for a sentence in the interface — which is
//! Chinese, so a failure that reached the banner in English would be the one
//! English sentence in the window. The window therefore says what *it* could not
//! do, and the core's own message travels after the colon: core words a failure
//! precisely (it names the path, or the key, or the provider) and translating
//! that detail away would leave a person with less than the CLI gets.
//!
//! A failure of a lower layer is not re-worded, either: `Store`'s own `Display`
//! is the same sentence `plainly history` would print, so two surfaces describing
//! one broken file describe it the same way.

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
