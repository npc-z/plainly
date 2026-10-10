//! One main window per session, and what a second launch does about it.
//!
//! The panel opens this window (⤢), and a key binding can be pressed twice
//! before a person looks up. Two windows would then be two views of one store,
//! two searches, and one of them behind the other — so the second
//! `plainly-desktop` asks the first to come forward and exits (spec §13).
//!
//! The spec names `tauri-plugin-single-instance` for this, which does it over
//! D-Bus. That plugin is not reachable from this machine — there is no route to
//! crates.io here — and the handover it performs is small enough to speak
//! directly: a **lock file** in the session's runtime directory decides who is
//! the window, and the one who is listens on a socket beside it for `activate`.
//!
//! The lock is an `flock`, and that choice is the whole design:
//!
//! - **It needs no cleanup.** The kernel releases an `flock` when the process
//!   dies, however it dies — a `SIGKILL`, a panic, a logout. There is no stale
//!   state to detect and none to remove, which is the failure a lock file
//!   otherwise always has.
//! - **It cannot be raced.** Deciding who is the window is one atomic
//!   `flock(LOCK_EX | LOCK_NB)`, not "look at the socket file, then bind it".
//!   The check-then-act version has a window between the two steps in which a
//!   second launch can decide the same thing, and two windows is exactly what
//!   this module exists to prevent.
//! - **The lock is never unlinked, and the socket file is.** The two files are
//!   treated differently because they are different things: an `flock` lives in
//!   the kernel and only needs a path to be found at, while a socket file a dead
//!   window left behind *cannot* be rebound — the kernel answers `EADDRINUSE` for
//!   a socket inode, listener or not — so whoever takes the lock clears it and
//!   binds a fresh one. That removal is safe for exactly one reason: the lock has
//!   already said there is only one window, so nobody else can be at that line.
//!
//! The lock and the socket are two files for two jobs: the lock says *who* is
//! the window, the socket is *how* a second launch talks to them. Losing the
//! socket (a window that died between taking the lock and binding) costs the
//! handover of one launch, not the single-instance guarantee.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// The file names the window owns inside `$XDG_RUNTIME_DIR`.
///
/// They carry the application identifier because the runtime directory is shared
/// by everything the session runs: a name as general as `window.sock` would be a
/// claim on somebody else's socket.
pub const SOCKET_NAME: &str = "dev.plainly.app-desktop.sock";
/// The lock file beside the socket.
pub const LOCK_NAME: &str = "dev.plainly.app-desktop.lock";

/// What a second launch writes to the window.
const ACTIVATE: &str = "activate";

/// How long a launch keeps trying to reach a window that holds the lock, and how
/// long it waits between tries.
///
/// The window that holds the lock may still be starting: it takes the lock
/// before it binds the socket, so for that moment it is the window with nobody
/// listening — and the work between those two points is opening the history
/// store and building a webview, which on a cold, loaded machine is seconds
/// rather than milliseconds. A launch that gave up early would look to a person
/// like the shortcut did nothing, so the budget is generous: five seconds.
const HANDOVER_TRIES: usize = 100;
const HANDOVER_WAIT: Duration = Duration::from_millis(50);

/// What a launch did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// This process is the window: run it.
    First,
    /// Another window is already open, and it has been asked to come forward.
    Handed,
}

/// Take the window's lock, or hand this launch to the instance that has it.
///
/// `activate` is called on every request to come forward — the first launch
/// included, so that "it just opened" and "it was already open" are one path
/// rather than two. It is called from the listener thread, so what travels on
/// that channel has to be something the main loop can act on from there.
pub fn claim(activate: Sender<()>) -> Result<Role, String> {
    let paths = Paths::resolve()?;

    match hold(&paths.lock)? {
        Hold::Taken => {
            hand_over(&paths.socket)?;
            Ok(Role::Handed)
        }
        Hold::Mine(held) => {
            // The socket is bound *after* the lock is held, so nothing can
            // mistake this process for one that is still starting.
            let listener = match bind(&paths.socket) {
                Ok(listener) => listener,
                Err(error) => {
                    // The lock is released with `held`; the next launch becomes
                    // the window and binds a fresh socket. Reported all the same:
                    // a handover this launch cannot serve is a bug worth seeing.
                    return Err(error);
                }
            };
            listen(listener, activate.clone());
            // The window comes forward when it opens because this is where that
            // happens, not because startup remembered to do it.
            activate
                .send(())
                .map_err(|_| "the window closed while it was starting".to_string())?;

            // Held for as long as the window lives; the kernel releases it when
            // the process ends. Leaked rather than dropped because the `File`
            // must outlive this call, and the process's whole life is what the
            // lock is for; the descriptor dies with the process either way.
            std::mem::forget(held);
            Ok(Role::First)
        }
    }
}

/// The lock file and the socket beside it.
struct Paths {
    lock: PathBuf,
    socket: PathBuf,
}

impl Paths {
    /// Where this session's lock and socket live.
    ///
    /// `$XDG_RUNTIME_DIR` when the session published one — `/run/user/<uid>`:
    /// private to the user, on a local filesystem, and removed by the session on
    /// logout. The system temporary directory is the fallback, and there the
    /// names carry the user id, because `/tmp` is shared.
    fn resolve() -> Result<Self, String> {
        Self::resolve_in(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
    }

    /// The same, from a runtime directory given as data.
    ///
    /// The directory is an argument rather than read here, for the reason
    /// [`plainly_core::Paths`] takes its environment as data: an environment read
    /// is process-wide, and a test that changed it to check the fallback would be
    /// changing it under every other test in the binary. [`Paths::resolve`] is
    /// the one place that reads the process environment.
    fn resolve_in(runtime: Option<&std::ffi::OsStr>) -> Result<Self, String> {
        let directory = match runtime.filter(|value| !value.is_empty()) {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                if !dir.is_absolute() {
                    return Err(format!(
                        "XDG_RUNTIME_DIR is not an absolute path: {}",
                        dir.display()
                    ));
                }
                dir
            }
            None => std::env::temp_dir(),
        };

        let prefix = match runtime.filter(|value| !value.is_empty()) {
            Some(_) => String::new(),
            None => format!("{}-", user_id()),
        };

        Ok(Self {
            lock: directory.join(format!("{prefix}{LOCK_NAME}")),
            socket: directory.join(format!("{prefix}{SOCKET_NAME}")),
        })
    }
}

/// Whether this process is now the window.
enum Hold {
    /// This process holds the lock: it is the window.
    Mine(File),
    /// Another process holds it.
    Taken,
}

/// The lock file, opened and locked for this process's life.
///
/// `flock(LOCK_EX | LOCK_NB)`: the first process gets it, every other process
/// gets `EWOULDBLOCK`, and the kernel gives it back when the holder dies. That
/// last part is why there is no staleness question here — which is the whole
/// reason this is a lock and not a "is there a socket file" check.
fn hold(path: &Path) -> Result<Hold, String> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;

    // SAFETY: `flock` takes a file descriptor this call owns for the duration of
    // the call, and an operation code; it has no other preconditions.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if locked == 0 {
        return Ok(Hold::Mine(file));
    }

    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        // What a lock somebody else holds looks like.
        Some(libc::EWOULDBLOCK) => Ok(Hold::Taken),
        _ => Err(format!("cannot lock {}: {error}", path.display())),
    }
}

/// Bind the socket a second launch reaches the window on.
///
/// A socket file an earlier window left is **cleared**, because there is no
/// other way to use the path: the kernel refuses to bind a socket *inode* a
/// second time, listener or not. That removal is safe for one reason and it is
/// worth being precise about — the caller has already taken the lock, so no
/// other process can be at this line, and the path is this window's to use.
///
/// It is also why the lock and not the socket is what decides who is the window.
/// "Is there a socket here, and can I bind it?" is a check whose answer can
/// change between the two halves, and two windows is what this module exists to
/// prevent.
fn bind(path: &Path) -> Result<UnixListener, String> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            std::fs::remove_file(path)
                .map_err(|error| format!("cannot clear {}: {error}", path.display()))?;
            UnixListener::bind(path)
                .map_err(|error| format!("cannot listen on {}: {error}", path.display()))
        }
        Err(error) => Err(format!("cannot listen on {}: {error}", path.display())),
    }
}

/// Serve the socket: every connection is one request to come forward.
fn listen(listener: UnixListener, activate: Sender<()>) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut line);
            if line.trim() == ACTIVATE {
                let _ = activate.send(());
            }
        }
    });
}

/// Ask the window that holds the lock to come forward.
///
/// Tried more than once because the holder takes the lock before it binds: a
/// launch that arrived in between would otherwise give up on a window that is
/// about to be listening, and from a key binding that looks like nothing
/// happening at all.
fn hand_over(path: &Path) -> Result<(), String> {
    let mut last = String::new();

    for _ in 0..HANDOVER_TRIES {
        match UnixStream::connect(path) {
            Ok(mut stream) => {
                stream
                    .write_all(format!("{ACTIVATE}\n").as_bytes())
                    .map_err(|error| format!("cannot ask the window to come forward: {error}"))?;
                return Ok(());
            }
            // `ENOENT` and `ECONNREFUSED` are both "not yet", and both are what
            // a window still starting looks like.
            Err(error) => last = error.to_string(),
        }
        thread::sleep(HANDOVER_WAIT);
    }

    Err(format!(
        "另一个 plainly 窗口还占着位置，但等了 {} 秒也没开始听（{last}）；稍后再按一次",
        (HANDOVER_TRIES as u64 * HANDOVER_WAIT.as_millis() as u64) / 1000
    ))
}

/// The user id, read from the process's own credentials.
///
/// `std::os::unix::fs::MetadataExt` would want a file to look at; the process's
/// uid is what the name needs, and this is the one syscall that answers it
/// without a dependency.
fn user_id() -> u32 {
    // SAFETY: `getuid` cannot fail and takes no arguments; it reports the calling
    // process's own credentials.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::mpsc::channel;

    /// A directory of this test's own, so that two tests running at once cannot
    /// take each other's window.
    ///
    /// Short on purpose: a Unix socket path has to fit in `sun_path` (108 bytes
    /// on Linux), and the test's temporary directory is already most of that on
    /// some machines — the first version of this test failed with "path must be
    /// shorter than SUN_LEN", which is a fact about the environment rather than
    /// about the code under test.
    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "pd-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is creatable");
        path
    }

    #[test]
    fn the_runtime_directory_is_used_when_the_session_published_one() {
        let paths = Paths::resolve_in(Some(OsStr::new("/run/user/1000")))
            .expect("an absolute runtime directory is usable");

        assert_eq!(paths.lock, PathBuf::from("/run/user/1000").join(LOCK_NAME));
        assert_eq!(
            paths.socket,
            PathBuf::from("/run/user/1000").join(SOCKET_NAME)
        );
    }

    #[test]
    fn a_relative_runtime_directory_is_refused() {
        assert!(
            Paths::resolve_in(Some(OsStr::new("run/user/1000"))).is_err(),
            "a relative runtime directory would put the socket somewhere unpredictable"
        );
    }

    #[test]
    fn the_fallback_names_this_user() {
        // The runtime directory a session with a compositor has is already
        // private to the user; `/tmp`, the fallback, is not, so the names have to
        // carry the id. An empty value counts as unset, as it does everywhere
        // else in Plainly's paths.
        let uid = unsafe { libc::getuid() }.to_string();
        for absent in [None, Some(OsStr::new(""))] {
            let paths = Paths::resolve_in(absent).expect("the fallback resolves");

            assert!(
                paths.socket.to_string_lossy().contains(&uid),
                "{} should name the user in a shared directory",
                paths.socket.display()
            );
            assert!(paths.lock.to_string_lossy().contains(&uid));
        }
    }

    #[test]
    fn the_first_holder_is_the_window_and_the_second_is_not() {
        let dir = temp_dir("hold");
        let lock = dir.join(LOCK_NAME);

        let first = hold(&lock).expect("the lock opens");
        assert!(
            matches!(first, Hold::Mine(_)),
            "the first launch is the window"
        );

        let second = hold(&lock).expect("the lock opens a second time");
        assert!(
            matches!(second, Hold::Taken),
            "a launch while a window holds the lock is not the window"
        );

        // Dropping the holder is what a window exiting does, and the kernel is
        // what makes the lock available again.
        drop(first);
        let third = hold(&lock).expect("the lock opens again");
        assert!(
            matches!(third, Hold::Mine(_)),
            "a window that exited leaves nothing behind to work around"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_launch_asks_the_window_to_come_forward() {
        let dir = temp_dir("handover");
        let socket = dir.join(SOCKET_NAME);
        let lock = dir.join(LOCK_NAME);
        let (sender, receiver) = channel();

        let _held = hold(&lock).expect("the lock opens");
        let listener = bind(&socket).expect("the socket is bindable");
        listen(listener, sender);

        hand_over(&socket).expect("the window is reachable");
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the request arrives");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_handover_to_a_window_that_is_not_listening_yet_keeps_trying() {
        let dir = temp_dir("starting");
        let socket = dir.join(SOCKET_NAME);

        // The holder's socket, appearing a moment after the launch asks.
        let appearing = socket.clone();
        let listening = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            let listener = UnixListener::bind(&appearing).expect("the socket is bindable");
            let (stream, _) = listener.accept().expect("the request arrives");
            drop(stream);
        });

        hand_over(&socket).expect("a window that starts in time is still reached");
        listening.join().expect("the helper thread does not panic");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_nobody_holds_is_taken_over_without_cleaning_anything_up() {
        let dir = temp_dir("stale");
        let lock = dir.join(LOCK_NAME);
        let socket = dir.join(SOCKET_NAME);

        // What a window killed with SIGKILL leaves: two files. The lock is free
        // because the kernel released it, and the socket file cannot be bound
        // again, so `bind` removes it — but only after the lock has said this
        // process is the window, which is what makes that removal safe.
        {
            let _listener = UnixListener::bind(&socket).expect("the socket is bindable");
            let _file = lock;
        }
        assert!(socket.exists(), "the socket file outlives its listener");

        let held = hold(&dir.join(LOCK_NAME)).expect("the lock opens");
        assert!(matches!(held, Hold::Mine(_)));
        assert!(
            bind(&socket).is_ok(),
            "the window that holds the lock may clear and rebind the socket file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
