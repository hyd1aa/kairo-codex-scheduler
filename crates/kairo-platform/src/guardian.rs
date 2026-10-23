//! Linux parent-death watchdog. The fork child uses only async-signal-safe syscalls.
use anyhow::Result;
use std::{
    os::fd::{FromRawFd, OwnedFd},
    thread,
    time::{Duration, Instant},
};
pub struct ParentWatch {
    writer: Option<OwnedFd>,
    guardian: libc::pid_t,
    group: libc::pid_t,
}
impl ParentWatch {
    pub fn new(group: u32) -> Result<Self> {
        let mut pipe = [-1; 2];
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(pipe[0]);
                libc::close(pipe[1]);
            }
            return Err(error.into());
        }
        if pid == 0 {
            unsafe {
                libc::close(pipe[1]);
                let mut byte = 0u8;
                loop {
                    let n = libc::read(pipe[0], (&mut byte as *mut u8).cast(), 1);
                    if n == 0 {
                        break;
                    }
                    if n < 0 && *libc::__errno_location() != libc::EINTR {
                        break;
                    }
                }
                libc::kill(-(group as i32), libc::SIGKILL);
                libc::close(pipe[0]);
                libc::_exit(0)
            }
        }
        unsafe {
            libc::close(pipe[0]);
        }
        Ok(Self {
            writer: Some(unsafe { OwnedFd::from_raw_fd(pipe[1]) }),
            guardian: pid,
            group: group as i32,
        })
    }
}
impl Drop for ParentWatch {
    fn drop(&mut self) {
        drop(self.writer.take());
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let n = unsafe { libc::waitpid(self.guardian, std::ptr::null_mut(), libc::WNOHANG) };
            if n == self.guardian || n < 0 {
                break;
            }
            if Instant::now() > until {
                unsafe {
                    libc::kill(-self.group, libc::SIGKILL);
                    libc::kill(self.guardian, libc::SIGKILL);
                    libc::waitpid(self.guardian, std::ptr::null_mut(), 0);
                }
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };
    #[test]
    #[ignore]
    fn parent_worker() {
        let Ok(root) = std::env::var("KAIRO_TEST_GUARDIAN_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                "sleep 60 & printf '%s' \"$!\" > \"$1\"; wait",
                "kairo",
            ])
            .arg(root.join("child"))
            .process_group(0)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let _watch = ParentWatch::new(child.id()).unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(60));
        let _ = child.wait();
    }
    #[test]
    fn killed_parent_kills_descendants() {
        let root = std::env::temp_dir().join(format!("kairo-parent-death-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut worker = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "guardian::tests::parent_worker"])
            .env("KAIRO_TEST_GUARDIAN_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !(root.join("ready").exists()
            && root.join("child").metadata().is_ok_and(|m| m.len() > 0))
            && Instant::now() < until
        {
            thread::sleep(Duration::from_millis(10))
        }
        worker.kill().unwrap();
        worker.wait().unwrap();
        assert!(root.join("ready").exists());
        let pid = fs::read_to_string(root.join("child")).unwrap();
        let status = std::path::PathBuf::from("/proc").join(pid).join("status");
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let alive = fs::read_to_string(&status).ok().is_some_and(|s| {
                s.lines()
                    .any(|line| line.starts_with("State:") && !line.contains('Z'))
            });
            if !alive {
                break;
            }
            assert!(Instant::now() < until, "descendant survived parent SIGKILL");
            thread::sleep(Duration::from_millis(20))
        }
        fs::remove_dir_all(root).unwrap();
    }
}
