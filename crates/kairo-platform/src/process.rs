use anyhow::{bail, Result};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
pub fn output(cmd: &mut Command, seconds: u64, limit: usize) -> Result<Output> {
    let mut child = cmd
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let _watch = match crate::guardian::ParentWatch::new(child.id()) {
        Ok(w) => w,
        Err(e) => {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            };
            child.wait()?;
            return Err(e);
        }
    };
    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(limit as u64 + 1).read_to_end(&mut bytes);
        (result, bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() > deadline || crate::runtime::stopping() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            };
            child.wait()?;
            let _ = reader.join();
            bail!("process_timeout / 子进程超时或已收到退出信号")
        };
        thread::sleep(Duration::from_millis(20));
    };
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let (result, bytes) = reader
        .join()
        .map_err(|_| anyhow::anyhow!("process_read_failed / 子进程输出读取失败"))?;
    result?;
    if bytes.len() > limit {
        bail!("process_output_too_large / 子进程输出过大")
    };
    Ok(Output {
        status,
        stdout: bytes,
        stderr: Vec::new(),
    })
}
pub fn check(cmd: &mut Command) -> Result<bool> {
    Ok(output(cmd, 10, 1024 * 1024)?.status.success())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_output_and_hung_process() {
        assert!(output(Command::new("/bin/sh").args(["-c", "yes synthetic"]), 2, 64).is_err());
        assert!(output(
            Command::new("/bin/sh").args(["-c", "sleep 10 & wait"]),
            1,
            1024
        )
        .is_err());
    }
    #[test]
    fn harmless_command() {
        assert!(check(Command::new("/bin/sh").args(["-c", "exit 0"])).unwrap());
        assert!(!check(Command::new("/bin/sh").args(["-c", "exit 7"])).unwrap());
    }
}
