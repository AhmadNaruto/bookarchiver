use thiserror::Error;

#[derive(Error, Debug)]
pub enum CbzError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Halaman tidak ditemukan: {0}")]
    PageNotFound(String),

    #[error("File descriptor tidak valid")]
    InvalidFileDescriptor,

    #[error("Format file tidak didukung")]
    UnsupportedFormat,
}
