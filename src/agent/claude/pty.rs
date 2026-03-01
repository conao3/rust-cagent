use std::io::Read;
use std::mem::MaybeUninit;
use std::path::Path;
use std::sync::mpsc;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::oneshot;

pub struct RawModeGuard {
    original: libc::termios,
}

impl RawModeGuard {
    pub fn enter() -> anyhow::Result<Self> {
        let mut original = unsafe { MaybeUninit::<libc::termios>::zeroed().assume_init() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } != 0 {
            anyhow::bail!("tcgetattr failed");
        }
        let mut raw = original;
        unsafe { libc::cfmakeraw(&mut raw) };
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            anyhow::bail!("tcsetattr failed");
        }
        Ok(Self { original })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
    }
}

fn terminal_size() -> PtySize {
    let mut ws = unsafe { MaybeUninit::<libc::winsize>::zeroed().assume_init() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } != 0 {
        return PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
    }
    PtySize {
        rows: ws.ws_row,
        cols: ws.ws_col,
        pixel_width: ws.ws_xpixel,
        pixel_height: ws.ws_ypixel,
    }
}

pub struct PtyHandle {
    pub child_exited: oneshot::Receiver<anyhow::Result<u32>>,
    pub input_tx: mpsc::Sender<Vec<u8>>,
    pub output_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
}

pub fn spawn_claude(cwd: &Path, command: &str, initial_prompt: Option<&str>) -> anyhow::Result<PtyHandle> {
    let size = terminal_size();
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(size)?;

    let mut cmd = CommandBuilder::new(command);
    cmd.arg("--dangerously-skip-permissions");
    if let Some(prompt) = initial_prompt {
        cmd.arg(prompt);
    }
    cmd.cwd(cwd);

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let mut reader = pair.master.try_clone_reader()?;
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if output_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
    let mut writer = pair.master.take_writer()?;
    std::thread::spawn(move || {
        while let Ok(data) = input_rx.recv() {
            if writer.write_all(&data).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let result = child
            .wait()
            .map(|status| status.exit_code())
            .map_err(anyhow::Error::from);
        let _ = tx.send(result);
    });

    Ok(PtyHandle {
        child_exited: rx,
        input_tx,
        output_rx,
    })
}
