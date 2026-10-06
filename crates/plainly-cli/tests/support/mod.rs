//! A throwaway directory for tests that spawn the real binary against a real
//! (temporary) set of XDG directories.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "plainly-cli-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is creatable");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    /// The configuration file the CLI will use for this directory.
    pub fn config_file(&self) -> PathBuf {
        self.join("config/dev.plainly.app/config.toml")
    }

    /// Run `plainly` with every XDG directory pointed at this directory, and
    /// with the keyring switched off so the run never depends on a session bus.
    pub fn plainly(&self, args: &[&str]) -> std::process::Output {
        self.plainly_with_stdin(args, "")
    }

    pub fn plainly_with_stdin(&self, args: &[&str], stdin: &str) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(env!("CARGO_BIN_EXE_plainly"))
            .args(args)
            .env("HOME", self.path())
            .env("XDG_CONFIG_HOME", self.join("config"))
            .env("XDG_DATA_HOME", self.join("data"))
            .env("XDG_CACHE_HOME", self.join("cache"))
            .env("PLAINLY_DISABLE_KEYRING", "1")
            .env_remove("PLAINLY_DEEPSEEK_API_KEY")
            .env_remove("PLAINLY_OPENAI_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the plainly binary is built alongside its tests");

        child
            .stdin
            .as_mut()
            .expect("stdin is piped")
            .write_all(stdin.as_bytes())
            .expect("stdin accepts the test's input");
        child.wait_with_output().expect("the process is waitable")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The exit code, or a panic explaining that a signal killed the process.
pub fn code(output: &std::process::Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("killed by a signal: {output:?}"))
}

pub fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
