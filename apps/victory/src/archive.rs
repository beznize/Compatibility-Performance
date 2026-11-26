//! Read-only archive mapping. Large game archives need no equally large private allocation.
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READONLY,
    UnmapViewOfFile,
};

pub struct Archive {
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
    _file: File,
}

impl Archive {
    pub fn open(path: &Path) -> io::Result<Self> {
        // FILE_SHARE_READ also excludes truncation while workers borrow the mapped bytes.
        let file = OpenOptions::new().read(true).share_mode(1).open(path)?;
        let len = usize::try_from(file.metadata()?.len())
            .ok()
            .filter(|&n| n <= isize::MAX as usize)
            .ok_or_else(|| io::Error::other("archive exceeds address space"))?;
        if len == 0 {
            return Ok(Self {
                view: MEMORY_MAPPED_VIEW_ADDRESS::default(),
                len,
                _file: file,
            });
        }
        // SAFETY: the file is held read-only and was checked to have nonzero length.
        let mapping = unsafe {
            CreateFileMappingW(
                file.as_raw_handle(),
                std::ptr::null(),
                PAGE_READONLY,
                0,
                0,
                std::ptr::null(),
            )
        };
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, len) };
        let result = if view.Value.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self {
                view,
                len,
                _file: file,
            })
        };
        unsafe { CloseHandle(mapping) };
        result
    }
}

impl std::ops::Deref for Archive {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: the view is valid until Drop and this file excludes writers.
        unsafe { std::slice::from_raw_parts(self.view.Value.cast::<u8>(), self.len) }
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        if self.len != 0 {
            unsafe { UnmapViewOfFile(self.view) };
        }
    }
}
