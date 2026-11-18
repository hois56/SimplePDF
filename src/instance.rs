//! Hand Explorer launches to the viewer in this Windows user session.
use anyhow::{Context, Result};
use std::ffi::c_void;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
#[link(name = "kernel32")]
extern "system" {
    fn FindFirstChangeNotificationW(path: *const u16, subtree: i32, filter: u32) -> *mut c_void;
    fn FindNextChangeNotification(handle: *mut c_void) -> i32;
    fn FindCloseChangeNotification(handle: *mut c_void) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, timeout: u32) -> u32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
}

pub struct Inbox {
    _owner: File,
    dir: PathBuf,
    changed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

fn claim(dir: &Path) -> io::Result<Option<File>> {
    match OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(dir.join("viewer.lock"))
    {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.raw_os_error() == Some(32) => Ok(None),
        Err(e) => Err(e),
    }
}

impl Inbox {
    pub fn launch(paths: Vec<PathBuf>) -> Result<Option<(Self, Vec<PathBuf>)>> {
        let mut session = 0;
        if unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("사용자 프로그램 폴더")?)
            .join("SimplePDF")
            .join(format!("Open-{session}"));
        fs::create_dir_all(&dir)?;
        let mut request = None;
        loop {
            if let Some(file) = &request {
                if !Path::new(file).try_exists()? {
                    return Ok(None);
                }
            }
            if let Some(owner) = claim(&dir)? {
                let startup = if request.is_none() { paths } else { vec![] };
                return Ok(Some((
                    Self {
                        _owner: owner,
                        dir,
                        changed: Arc::new(AtomicBool::new(true)),
                        stop: Default::default(),
                    },
                    startup,
                )));
            }
            if request.is_none() {
                let mut staged = tempfile::NamedTempFile::new_in(&dir)?;
                for path in &paths {
                    let path = std::path::absolute(path)?;
                    let bytes: Vec<_> = path
                        .as_os_str()
                        .encode_wide()
                        .chain(Some(0))
                        .flat_map(u16::to_le_bytes)
                        .collect();
                    staged.write_all(&bytes)?;
                }
                let file = staged.path().with_extension("open");
                staged.persist_noclobber(&file)?;
                request = Some(file);
            }
            thread::sleep(Duration::from_millis(60));
        }
    }

    pub fn watch(&self, ctx: eframe::egui::Context) {
        let dir = self.dir.clone();
        let changed = self.changed.clone();
        let stop = self.stop.clone();
        thread::spawn(move || {
            let path: Vec<_> = OsStr::new(&dir).encode_wide().chain(Some(0)).collect();
            let handle = unsafe { FindFirstChangeNotificationW(path.as_ptr(), 0, 1) };
            let mut watching = handle as isize != -1;
            // Arm the watch before the initial scan so a concurrent launch cannot be missed.
            changed.store(true, Ordering::Release);
            ctx.request_repaint();
            while !stop.load(Ordering::Acquire) {
                if watching {
                    match unsafe { WaitForSingleObject(handle, 1000) } {
                        258 => continue, // WAIT_TIMEOUT
                        0 => watching = unsafe { FindNextChangeNotification(handle) } != 0,
                        _ => watching = false,
                    }
                } else {
                    thread::sleep(Duration::from_millis(350));
                }
                changed.store(true, Ordering::Release);
                ctx.request_repaint();
            }
            if handle as isize != -1 {
                unsafe {
                    FindCloseChangeNotification(handle);
                }
            }
        });
    }

    pub fn receive(&self, pending: &mut std::collections::VecDeque<PathBuf>) -> Result<bool> {
        if !self.changed.swap(false, Ordering::AcqRel) {
            return Ok(false);
        }
        let mut requests = Vec::new();
        for file in fs::read_dir(&self.dir)? {
            let file = file?;
            if file.path().extension() == Some(OsStr::new("open")) && file.file_type()?.is_file() {
                requests.push((file.metadata()?.modified()?, file.path()));
            }
        }
        requests.sort();
        let any = !requests.is_empty();
        for (_, file) in requests {
            let data = fs::read(&file)?;
            let words: Vec<u16> = data
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            fs::remove_file(file)?;
            pending.extend(
                words
                    .split(|&c| c == 0)
                    .filter(|p| !p.is_empty())
                    .map(|s| PathBuf::from(OsString::from_wide(s))),
            );
        }
        Ok(any)
    }
}
impl Drop for Inbox {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
