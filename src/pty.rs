#![allow(unused)]
#![allow(dead_code)]
//! Local PTY — spawns a shell with a pseudo-terminal.
//! Uses raw libc for the PTY + CommandExt::pre_exec for the child process.
//! No extra crate dependencies.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// A local pseudo-terminal connected to a shell process.
pub struct LocalPty {
    master: File,
    child: Child,
}

impl LocalPty {
    /// Spawn a shell process connected to a new PTY.
    /// `shell` should be the path to the shell binary (e.g. "/bin/bash").
    pub fn spawn(shell: &str) -> io::Result<Self> {
        // Open PTY master
        let master_fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        if master_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Grant and unlock slave
        if unsafe { libc::grantpt(master_fd) } != 0 || unsafe { libc::unlockpt(master_fd) } != 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(master_fd); }
            return Err(e);
        }

        let master = unsafe { File::from_raw_fd(master_fd) };

        // Spawn child with slave PTY as stdio
        let child = unsafe {
            Command::new(shell)
                .pre_exec(move || {
                    // Create new session, become session leader
                    if libc::setsid() == -1 {
                        return Err(io::Error::last_os_error());
                    }

                    // Open slave
                    let slave_name = libc::ptsname(master_fd);
                    if slave_name.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let slave_fd = libc::open(slave_name, libc::O_RDWR);
                    if slave_fd < 0 {
                        return Err(io::Error::last_os_error());
                    }

                    // Dup slave to stdin/stdout/stderr
                    libc::dup2(slave_fd, 0);
                    libc::dup2(slave_fd, 1);
                    libc::dup2(slave_fd, 2);
                    if slave_fd > 2 {
                        libc::close(slave_fd);
                    }

                    Ok(())
                })
                .spawn()?
        };

        Ok(LocalPty { master, child })
    }

    /// Write data to the PTY master (sends keystrokes to the shell).
    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.master.write_all(data)
    }

    /// Read available data from the PTY master (output from the shell).
    /// Returns 0 if no data is available (non-blocking).
    pub fn read_available(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Set non-blocking for this read
        let fd = self.master.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags >= 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK); }
        }

        let result = self.master.read(buf);

        // Restore blocking
        if flags >= 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags); }
        }

        match result {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// Get the raw file descriptor of the PTY master.
    pub fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    /// Clone the master file descriptor for sharing across threads.
    pub fn try_clone_master(&self) -> io::Result<File> {
        self.master.try_clone()
    }

    /// Check if the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the child process.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
