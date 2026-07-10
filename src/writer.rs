use std::fs::File;
use std::io::{Write, Seek, SeekFrom};
use std::os::fd::{FromRawFd, RawFd};
use std::path::PathBuf;
use zip::{ZipWriter, write::FileOptions as ZipFileOptions, CompressionMethod};
use tar::Builder as TarBuilder;
use crate::error::CbzError;

pub trait ComicWriter: Send + Sync {
    fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError>;
    fn finish(self: Box<Self>) -> Result<(), CbzError>;
}

pub struct CbzWriter {
    inner: Option<Box<dyn ComicWriter>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookFormat {
    Cbz = 0,
    Cbr = 1,
    Cb7 = 2,
    Cbt = 3,
    Bbf = 4,
    Directory = 5,
}

impl BookFormat {
    pub fn from_i32(val: i32) -> Result<Self, CbzError> {
        match val {
            0 => Ok(BookFormat::Cbz),
            1 => Ok(BookFormat::Cbr),
            2 => Ok(BookFormat::Cb7),
            3 => Ok(BookFormat::Cbt),
            4 => Ok(BookFormat::Bbf),
            5 => Ok(BookFormat::Directory),
            _ => Err(CbzError::UnsupportedFormat),
        }
    }
}

impl CbzWriter {
    /// Inisialisasi Writer menggunakan Android File Descriptor secara aman
    pub fn from_fd(fd: RawFd, format: BookFormat) -> Result<Self, CbzError> {
        let native_fd = unsafe { libc::dup(fd) };
        if native_fd < 0 {
            return Err(CbzError::InvalidFileDescriptor);
        }

        let file = unsafe { File::from_raw_fd(native_fd) };
        
        let inner: Box<dyn ComicWriter> = match format {
            BookFormat::Cbz => Box::new(ZipComicWriter::new(file)),
            BookFormat::Cbt => Box::new(TarComicWriter::new(file)),
            BookFormat::Bbf => Box::new(BbfComicWriter::new(file)),
            _ => return Err(CbzError::UnsupportedFormat),
        };

        Ok(Self { inner: Some(inner) })
    }

    /// Inisialisasi Writer menggunakan path local (berkas arsip atau folder direktori)
    pub fn from_path(path: &str, format: BookFormat) -> Result<Self, CbzError> {
        let inner: Box<dyn ComicWriter> = match format {
            BookFormat::Directory => Box::new(DirectoryComicWriter::new(path)),
            _ => {
                let file = File::create(path)?;
                match format {
                    BookFormat::Cbz => Box::new(ZipComicWriter::new(file)),
                    BookFormat::Cbt => Box::new(TarComicWriter::new(file)),
                    BookFormat::Bbf => Box::new(BbfComicWriter::new(file)),
                    _ => return Err(CbzError::UnsupportedFormat),
                }
            }
        };

        Ok(Self { inner: Some(inner) })
    }

    pub fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError> {
        if let Some(ref mut writer) = self.inner {
            writer.write_page(page_name, data)
        } else {
            Err(CbzError::Io(std::io::Error::new(std::io::ErrorKind::Other, "Writer sudah ditutup")))
        }
    }

    pub fn finish(mut self) -> Result<(), CbzError> {
        if let Some(writer) = self.inner.take() {
            writer.finish()
        } else {
            Ok(())
        }
    }
}

// ---------------- ZIP Comic Writer ----------------
struct ZipComicWriter {
    zip: ZipWriter<File>,
}

impl ZipComicWriter {
    fn new(file: File) -> Self {
        Self { zip: ZipWriter::new(file) }
    }
}

impl ComicWriter for ZipComicWriter {
    fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError> {
        let options = ZipFileOptions::default().compression_method(CompressionMethod::Stored);
        self.zip.start_file(page_name, options)?;
        self.zip.write_all(data)?;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), CbzError> {
        self.zip.finish()?;
        Ok(())
    }
}

// ---------------- TAR Comic Writer ----------------
struct TarComicWriter {
    builder: TarBuilder<File>,
}

impl TarComicWriter {
    fn new(file: File) -> Self {
        Self { builder: TarBuilder::new(file) }
    }
}

impl ComicWriter for TarComicWriter {
    fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError> {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        self.builder.append_data(&mut header, page_name, data)?;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), CbzError> {
        self.builder.finish()?;
        Ok(())
    }
}

// ---------------- BBF Comic Writer ----------------
struct BbfComicWriter {
    file: File,
    pages: Vec<(String, Vec<u8>)>,
}

impl BbfComicWriter {
    fn new(file: File) -> Self {
        Self { file, pages: Vec::new() }
    }
}

impl ComicWriter for BbfComicWriter {
    fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError> {
        self.pages.push((page_name.to_string(), data.to_vec()));
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), CbzError> {
        let alignment = 8u64;

        // 1. Tulis header dummy terlebih dahulu
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 64];
        self.file.write_all(&header)?;

        // 2. Tulis data aset dan bangun entri tabel aset
        let mut asset_entries = Vec::new();
        let mut current_offset = 64u64;

        for (name, data) in &self.pages {
            let pad = (alignment - (current_offset % alignment)) % alignment;
            if pad > 0 {
                self.file.write_all(&vec![0u8; pad as usize])?;
                current_offset += pad;
            }

            let start_offset = current_offset;
            self.file.write_all(data)?;
            current_offset += data.len() as u64;

            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            let asset_type = match ext.as_str() {
                "avif" => 0x01,
                "png" => 0x02,
                "webp" => 0x03,
                "jxl" => 0x04,
                "bmp" => 0x05,
                "gif" => 0x07,
                "tiff" => 0x08,
                "jpg" | "jpeg" => 0x09,
                _ => 0x00,
            };

            let hash_bytes = [0u8; 16];

            let mut entry = [0u8; 48];
            entry[0..8].copy_from_slice(&start_offset.to_le_bytes());
            entry[8..24].copy_from_slice(&hash_bytes);
            entry[24..32].copy_from_slice(&(data.len() as u64).to_le_bytes());
            entry[38] = asset_type;
            asset_entries.push(entry);
        }

        // 3. Tulis Asset Table
        let asset_table_offset = current_offset;
        for entry in &asset_entries {
            self.file.write_all(entry)?;
            current_offset += 48;
        }

        // 4. Tulis Page Table
        let page_table_offset = current_offset;
        for i in 0..self.pages.len() {
            let mut entry = [0u8; 16];
            entry[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            self.file.write_all(&entry)?;
            current_offset += 16;
        }

        // 5. Tulis String Pool (minimal 1 null byte)
        let string_pool_offset = current_offset;
        self.file.write_all(&[0u8])?;
        current_offset += 1;

        // 6. Tulis Footer
        let footer_offset = current_offset;
        let mut footer = [0u8; 256];
        footer[0..8].copy_from_slice(&asset_table_offset.to_le_bytes());
        footer[8..16].copy_from_slice(&page_table_offset.to_le_bytes());
        footer[40..48].copy_from_slice(&string_pool_offset.to_le_bytes());
        footer[48..56].copy_from_slice(&1u64.to_le_bytes());
        footer[56..64].copy_from_slice(&(self.pages.len() as u64).to_le_bytes());
        footer[64..72].copy_from_slice(&(self.pages.len() as u64).to_le_bytes());
        footer[100..104].copy_from_slice(&256u32.to_le_bytes());

        self.file.write_all(&footer)?;

        // 7. Tulis ulang header asli di offset 0
        self.file.seek(SeekFrom::Start(0))?;
        header[0..4].copy_from_slice(b"BBF3");
        header[4..6].copy_from_slice(&3u16.to_le_bytes());
        header[6..8].copy_from_slice(&64u16.to_le_bytes());
        header[12] = 3; // alignment 2^3 = 8
        header[16..24].copy_from_slice(&footer_offset.to_le_bytes());
        self.file.write_all(&header)?;

        Ok(())
    }
}

// ---------------- Directory Comic Writer (Extracted Folder) ----------------
struct DirectoryComicWriter {
    dir_path: PathBuf,
}

impl DirectoryComicWriter {
    fn new(path: &str) -> Self {
        let dir_path = PathBuf::from(path);
        let _ = std::fs::create_dir_all(&dir_path);
        Self { dir_path }
    }
}

impl ComicWriter for DirectoryComicWriter {
    fn write_page(&mut self, page_name: &str, data: &[u8]) -> Result<(), CbzError> {
        let file_path = self.dir_path.join(page_name);
        let mut file = File::create(file_path)?;
        file.write_all(data)?;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), CbzError> {
        Ok(())
    }
}

// ---------------- Integration Tests ----------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::CbzReader;

    #[test]
    fn test_all_formats() {
        std::fs::create_dir_all("target").unwrap();
        
        let formats = vec![
            (BookFormat::Cbz, "target/test_comic.cbz"),
            (BookFormat::Cbt, "target/test_comic.cbt"),
            (BookFormat::Bbf, "target/test_comic.bbf"),
            (BookFormat::Directory, "target/test_comic_dir"),
        ];

        for (format, path) in formats {
            // Write
            {
                let mut writer = CbzWriter::from_path(path, format).unwrap();
                writer.write_page("0001.jpg", b"page1data").unwrap();
                writer.write_page("0002.png", b"page2data").unwrap();
                writer.finish().unwrap();
            }

            // Read & Verify
            {
                let reader = CbzReader::from_path(path).unwrap();
                let pages = reader.get_pages().unwrap();
                
                assert_eq!(pages.len(), 2);
                assert!(pages[0].contains("0001"));
                assert!(pages[1].contains("0002"));

                let data1 = reader.read_page(&pages[0]).unwrap();
                assert_eq!(data1, b"page1data");

                let data2 = reader.read_page(&pages[1]).unwrap();
                assert_eq!(data2, b"page2data");
            }

            // Clean up
            if format == BookFormat::Directory {
                std::fs::remove_dir_all(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
    }
}
