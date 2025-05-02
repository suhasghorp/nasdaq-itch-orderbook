use memmap2::{Mmap, MmapOptions};
use std::fs::File;
use std::io;
use std::path::Path;

pub fn map_file(path: &Path) -> io::Result<Mmap> {
    let file = File::open(path)?;
    unsafe { MmapOptions::new().map(&file) }
}