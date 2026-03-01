use std::io::{Read, Write};
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

fn terminal_size() -> anyhow::Result<PtySize> {
    let mut ws = unsafe { MaybeUninit::<libc::winsize>::zeroed().assume_init() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } != 0 {
        anyhow::bail!("ioctl TIOCGWINSZ failed");
    }
    Ok(PtySize {
        rows: ws.ws_row,
        cols: ws.ws_col,
        pixel_width: ws.ws_xpixel,
        pixel_height: ws.ws_ypixel,
    })
}

pub struct PtyHandle {
    pub child_exited: oneshot::Receiver<anyhow::Result<u32>>,
    pub input_tx: mpsc::Sender<Vec<u8>>,
}

pub fn spawn_claude(cwd: &Path) -> anyhow::Result<PtyHandle> {
    let size = terminal_size()?;
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(size)?;

    let mut cmd = CommandBuilder::new("claude");
    cmd.cwd(cwd);

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    std::thread::spawn(move || {
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
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
    })
}
