use std::fs::File;
use std::io::{Read, Cursor};
use std::os::fd::{FromRawFd, RawFd};
use std::sync::Arc;
use memmap2::Mmap;
use zip::ZipArchive;
use tar::Archive as TarArchive;
use sevenz_rust::{SevenZReader, Password};
use crate::error::CbzError;

// Wrapper untuk memampukan Arc<Mmap> mengimplementasikan AsRef<[u8]> secara aman
#[derive(Clone)]
struct ArcMmap(Arc<Mmap>);

impl AsRef<[u8]> for ArcMmap {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

pub trait ComicReader: Send + Sync {
    fn get_pages(&self) -> Result<Vec<String>, CbzError>;
    fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError>;
}

pub struct CbzReader {
    _file: File,
    inner: Box<dyn ComicReader>,
}

impl CbzReader {
    /// Inisialisasi Reader menggunakan Android File Descriptor secara aman
    pub fn from_fd(fd: RawFd) -> Result<Self, CbzError> {
        let native_fd = unsafe { libc::dup(fd) };
        if native_fd < 0 {
            return Err(CbzError::InvalidFileDescriptor);
        }

        let file = unsafe { File::from_raw_fd(native_fd) };
        
        // Auto-detect format dari bytes header
        let mut check_file = file.try_clone()?;
        let mut header_buf = [0u8; 512];
        let bytes_read = match check_file.read(&mut header_buf) {
            Ok(n) => n,
            Err(_) => 0,
        };
        
        let inner: Box<dyn ComicReader> = if bytes_read >= 4 && &header_buf[0..4] == b"PK\x03\x04" {
            Box::new(ZipComicReader::new(&file)?)
        } else if bytes_read >= 6 && &header_buf[0..6] == b"7z\xbc\xaf\x27\x1c" {
            Box::new(SevenzComicReader::new(&file)?)
        } else if bytes_read >= 4 && &header_buf[0..4] == b"BBF3" {
            Box::new(BbfComicReader::new(&file)?)
        } else if bytes_read >= 262 && &header_buf[257..262] == b"ustar" {
            Box::new(TarComicReader::new(&file)?)
        } else if bytes_read >= 4 && &header_buf[0..4] == b"Rar!" {
            Box::new(RarComicReader::new(&file)?)
        } else {
            // Coba parsing sebagai ZIP default jika ragu
            Box::new(ZipComicReader::new(&file)?)
        };

        Ok(Self { _file: file, inner })
    }

    pub fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        self.inner.get_pages()
    }

    pub fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError> {
        self.inner.read_page(page_name)
    }
}

// ---------------- ZIP Comic Reader (Cached Index) ----------------
struct ZipComicReader {
    _mmap: Arc<Mmap>,
    archive: std::sync::Mutex<ZipArchive<Cursor<ArcMmap>>>,
    pages: Vec<String>,
}

impl ZipComicReader {
    fn new(file: &File) -> Result<Self, CbzError> {
        let mmap = Arc::new(unsafe { Mmap::map(file)? });
        let cursor = Cursor::new(ArcMmap(mmap.clone()));
        let mut archive = ZipArchive::new(cursor)?;
        
        let mut pages = Vec::new();
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            if file.is_file() {
                let name = file.name().to_string();
                if is_image_file(&name) {
                    pages.push(name);
                }
            }
        }
        
        pages.sort_by(|a, b| alphanumeric_sort::compare_str(a, b));
        Ok(Self {
            _mmap: mmap,
            archive: std::sync::Mutex::new(archive),
            pages,
        })
    }
}

impl ComicReader for ZipComicReader {
    fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        Ok(self.pages.clone())
    }

    fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError> {
        let mut archive = self.archive.lock().unwrap();
        let mut file = match archive.by_name(page_name) {
            Ok(file) => file,
            Err(zip::result::ZipError::FileNotFound) => {
                return Err(CbzError::PageNotFound(page_name.to_string()));
            }
            Err(e) => return Err(CbzError::Zip(e)),
        };
        
        let mut buffer = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}

// ---------------- TAR Comic Reader (Zero-Copy Offset Cache) ----------------
struct TarComicReader {
    mmap: Arc<Mmap>,
    pages: Vec<String>,
    entry_map: std::collections::HashMap<String, (u64, u64)>, // page_name -> (offset, size)
}

impl TarComicReader {
    fn new(file: &File) -> Result<Self, CbzError> {
        let mmap = Arc::new(unsafe { Mmap::map(file)? });
        let mut archive = TarArchive::new(&mmap[..]);
        
        let mut pages = Vec::new();
        let mut entry_map = std::collections::HashMap::new();
        
        for entry_res in archive.entries()? {
            let entry = entry_res?;
            let path = entry.path()?.to_string_lossy().to_string();
            if is_image_file(&path) {
                pages.push(path.clone());
                let offset = entry.raw_file_position();
                let size = entry.header().size()?;
                entry_map.insert(path, (offset, size));
            }
        }
        pages.sort_by(|a, b| alphanumeric_sort::compare_str(a, b));
        
        Ok(Self {
            mmap,
            pages,
            entry_map,
        })
    }
}

impl ComicReader for TarComicReader {
    fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        Ok(self.pages.clone())
    }

    fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError> {
        let &(offset, size) = self.entry_map.get(page_name)
            .ok_or_else(|| CbzError::PageNotFound(page_name.to_string()))?;
            
        let start = offset as usize;
        let end = (offset + size) as usize;
        
        if end > self.mmap.len() {
            return Err(CbzError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Tar entry offset out of bounds",
            )));
        }
        
        Ok(self.mmap[start..end].to_vec())
    }
}

// ---------------- 7Z Comic Reader (Cached Index) ----------------
struct SevenzComicReader {
    mmap: Arc<Mmap>,
    pages: Vec<String>,
}

impl SevenzComicReader {
    fn new(file: &File) -> Result<Self, CbzError> {
        let mmap = Arc::new(unsafe { Mmap::map(file)? });
        let cursor = Cursor::new(ArcMmap(mmap.clone()));
        let mut reader = SevenZReader::new(cursor, mmap.len() as u64, Password::from(""))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let mut pages = Vec::new();
        reader.for_each_entries(|entry, _| {
            if !entry.is_directory() {
                let name = entry.name.to_string();
                if is_image_file(&name) {
                    pages.push(name);
                }
            }
            Ok(true)
        }).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        pages.sort_by(|a, b| alphanumeric_sort::compare_str(a, b));

        Ok(Self { mmap, pages })
    }
}

impl ComicReader for SevenzComicReader {
    fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        Ok(self.pages.clone())
    }

    fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError> {
        let cursor = Cursor::new(ArcMmap(self.mmap.clone()));
        let mut reader = SevenZReader::new(cursor, self.mmap.len() as u64, Password::from(""))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let mut result = None;
        reader.for_each_entries(|entry, read_stream| {
            if entry.name == page_name {
                let mut buffer = Vec::new();
                read_stream.read_to_end(&mut buffer)?;
                result = Some(buffer);
                return Ok(false); // Stop iteration
            }
            Ok(true)
        }).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        result.ok_or_else(|| CbzError::PageNotFound(page_name.to_string()))
    }
}

// ---------------- RAR Comic Reader (Stub) ----------------
struct RarComicReader;

impl RarComicReader {
    fn new(_file: &File) -> Result<Self, CbzError> {
        Err(CbzError::UnsupportedFormat)
    }
}

impl ComicReader for RarComicReader {
    fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        Err(CbzError::UnsupportedFormat)
    }

    fn read_page(&self, _page_name: &str) -> Result<Vec<u8>, CbzError> {
        Err(CbzError::UnsupportedFormat)
    }
}

// ---------------- BBF Comic Reader (Zero-Copy Mmap & Cached Index) ----------------
struct BbfComicReader {
    mmap: Mmap,
    pages: Vec<String>,
    page_data: Vec<(u64, u64)>, // (offset, size)
}

impl BbfComicReader {
    fn new(file: &File) -> Result<Self, CbzError> {
        let mmap = unsafe { Mmap::map(file)? };
        if mmap.len() < 64 {
            return Err(CbzError::UnsupportedFormat);
        }

        // Validate header magic
        if &mmap[0..4] != b"BBF3" {
            return Err(CbzError::UnsupportedFormat);
        }

        let footer_offset = u64::from_le_bytes(mmap[16..24].try_into().unwrap());
        if footer_offset + 256 > mmap.len() as u64 {
            return Err(CbzError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "BBF Footer offset out of bounds",
            )));
        }

        let footer_start = footer_offset as usize;
        let asset_table_offset = u64::from_le_bytes(mmap[footer_start..footer_start + 8].try_into().unwrap());
        let page_table_offset = u64::from_le_bytes(mmap[footer_start + 8..footer_start + 16].try_into().unwrap());
        let asset_count = u64::from_le_bytes(mmap[footer_start + 56..footer_start + 64].try_into().unwrap());
        let page_count = u64::from_le_bytes(mmap[footer_start + 64..footer_start + 72].try_into().unwrap());

        // Pre-parse all pages
        let mut pages = Vec::with_capacity(page_count as usize);
        let mut page_data = Vec::with_capacity(page_count as usize);

        for i in 0..page_count {
            let page_offset = (page_table_offset + i * 16) as usize;
            if page_offset + 16 > mmap.len() {
                return Err(CbzError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "BBF Page table out of bounds",
                )));
            }
            let asset_index = u64::from_le_bytes(mmap[page_offset..page_offset + 8].try_into().unwrap());
            if asset_index >= asset_count {
                return Err(CbzError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "BBF Asset index out of bounds",
                )));
            }

            let asset_offset = (asset_table_offset + asset_index * 48) as usize;
            if asset_offset + 48 > mmap.len() {
                return Err(CbzError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "BBF Asset table out of bounds",
                )));
            }

            let file_data_offset = u64::from_le_bytes(mmap[asset_offset..asset_offset + 8].try_into().unwrap());
            let file_size = u64::from_le_bytes(mmap[asset_offset + 24..asset_offset + 32].try_into().unwrap());
            let asset_type = mmap[asset_offset + 38];

            if file_data_offset + file_size > mmap.len() as u64 {
                return Err(CbzError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "BBF Asset data position out of bounds",
                )));
            }

            let ext = match asset_type {
                0x01 => "avif",
                0x02 => "png",
                0x03 => "webp",
                0x04 => "jxl",
                0x05 => "bmp",
                0x07 => "gif",
                0x08 => "tiff",
                0x09 => "jpg",
                _ => "bin",
            };

            pages.push(format!("{:04}.{}", i + 1, ext));
            page_data.push((file_data_offset, file_size));
        }

        Ok(Self {
            mmap,
            pages,
            page_data,
        })
    }
}

impl ComicReader for BbfComicReader {
    fn get_pages(&self) -> Result<Vec<String>, CbzError> {
        Ok(self.pages.clone())
    }

    fn read_page(&self, page_name: &str) -> Result<Vec<u8>, CbzError> {
        let idx = self.pages.binary_search_by(|p| p.as_str().cmp(page_name))
            .map_err(|_| CbzError::PageNotFound(page_name.to_string()))?;
            
        let (offset, size) = self.page_data[idx];
        let start = offset as usize;
        let end = (offset + size) as usize;
        
        Ok(self.mmap[start..end].to_vec())
    }
}

// ---------------- Helper ----------------
fn is_image_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") || 
    lower.ends_with(".png") || lower.ends_with(".webp") || 
    lower.ends_with(".avif")
}
