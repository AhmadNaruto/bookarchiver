pub mod reader;
pub mod writer;
pub mod error;

use jni::JNIEnv;
use jni::objects::{JClass, JString, JByteArray};
use jni::sys::{jlong, jint, jbyteArray, jobjectArray, jboolean};
use reader::CbzReader;
use writer::{CbzWriter, BookFormat};
use std::os::fd::RawFd;

// Helper untuk mengubah pointer mentah kembali ke objek Rust
unsafe fn get_reader<'a>(ptr: jlong) -> &'a CbzReader {
    &*(ptr as *const CbzReader)
}

unsafe fn get_writer<'a>(ptr: jlong) -> &'a mut CbzWriter {
    &mut *(ptr as *mut CbzWriter)
}

// Helper untuk melemparkan exception ke JVM berdasarkan CbzError
fn throw_book_exception(env: &mut JNIEnv, err: &crate::error::CbzError) {
    let (class_name, msg) = match err {
        crate::error::CbzError::PageNotFound(page) => (
            "bookarchiver/BookPageNotFoundException",
            format!("Halaman tidak ditemukan: {}", page),
        ),
        crate::error::CbzError::InvalidFileDescriptor => (
            "bookarchiver/BookInitializationException",
            "File descriptor tidak valid atau gagal diduplikasi".to_string(),
        ),
        crate::error::CbzError::Io(e) => (
            "bookarchiver/BookIOException",
            e.to_string(),
        ),
        crate::error::CbzError::Zip(e) => (
            "bookarchiver/BookException",
            e.to_string(),
        ),
        crate::error::CbzError::UnsupportedFormat => (
            "bookarchiver/BookInitializationException",
            "Format berkas komik tidak didukung atau belum diimplementasikan".to_string(),
        ),
    };
    let _ = env.throw_new(class_name, msg);
}

/// Inisialisasi BookReader native dari File Descriptor
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookReader_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
) -> jlong {
    match CbzReader::from_fd(fd as RawFd) {
        Ok(reader) => {
            Box::into_raw(Box::new(reader)) as jlong
        }
        Err(e) => {
            throw_book_exception(&mut env, &e);
            0
        }
    }
}

/// Mengambil daftar halaman komik
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookReader_nativeGetPages(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jobjectArray {
    let reader = get_reader(ptr);
    
    match reader.get_pages() {
        Ok(pages) => {
            let string_class = match env.find_class("java/lang/String") {
                Ok(class) => class,
                Err(_) => return std::ptr::null_mut(),
            };

            // String inisialisasi awal untuk array
            let initial_element = match env.new_string("") {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let array = match env.new_object_array(pages.len() as jint, &string_class, &initial_element) {
                Ok(arr) => arr,
                Err(_) => return std::ptr::null_mut(),
            };

            // Isi array dengan string halaman dari Rust
            for (i, page) in pages.iter().enumerate() {
                let java_string = match env.new_string(page) {
                    Ok(s) => s,
                    Err(_) => return std::ptr::null_mut(),
                };
                if env.set_object_array_element(&array, i as jint, &java_string).is_err() {
                    return std::ptr::null_mut();
                }
                // Hapus local reference untuk mencegah table overflow (sangat penting untuk komik yang panjang)
                let _ = env.delete_local_ref(java_string);
            }

            array.into_raw()
        }
        Err(e) => {
            throw_book_exception(&mut env, &e);
            std::ptr::null_mut()
        }
    }
}

/// Membaca byte biner dari satu halaman gambar
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookReader_nativeReadPage(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    page_name: JString,
) -> jbyteArray {
    let reader = get_reader(ptr);
    
    let page_str: String = match env.get_string(&page_name) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    match reader.read_page(&page_str) {
        Ok(bytes) => {
            let byte_array = match env.new_byte_array(bytes.len() as jint) {
                Ok(arr) => arr,
                Err(_) => return std::ptr::null_mut(),
            };
            if env.set_byte_array_region(&byte_array, 0, bytemuck::cast_slice(&bytes)).is_err() {
                return std::ptr::null_mut();
            }
            byte_array.into_raw()
        }
        Err(e) => {
            throw_book_exception(&mut env, &e);
            std::ptr::null_mut()
        }
    }
}

/// Menghapus objek BookReader dan membebaskan memori native
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookReader_nativeClose(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        let _ = Box::from_raw(ptr as *mut CbzReader);
    }
}

/// Inisialisasi BookWriter native dari File Descriptor dengan pilihan format komik
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookWriter_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
    format_ordinal: jint,
) -> jlong {
    let format = match BookFormat::from_i32(format_ordinal) {
        Ok(f) => f,
        Err(e) => {
            throw_book_exception(&mut env, &e);
            return 0;
        }
    };
    match CbzWriter::from_fd(fd as RawFd, format) {
        Ok(writer) => {
            Box::into_raw(Box::new(writer)) as jlong
        }
        Err(e) => {
            throw_book_exception(&mut env, &e);
            0
        }
    }
}

/// Menulis halaman ke file zip/tar/bbf
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookWriter_nativeWritePage(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    page_name: JString,
    data: jbyteArray,
) -> jboolean {
    let writer = get_writer(ptr);

    let page_str: String = match env.get_string(&page_name) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let data_array = unsafe { JByteArray::from_raw(data) };
    let bytes = match env.convert_byte_array(&data_array) {
        Ok(b) => b,
        Err(_) => return 0,
    };

    match writer.write_page(&page_str, &bytes) {
        Ok(_) => 1,
        Err(e) => {
            throw_book_exception(&mut env, &e);
            0
        }
    }
}

/// Menutup objek BookWriter dan membebaskan memori native
#[no_mangle]
pub unsafe extern "system" fn Java_bookarchiver_BookWriter_nativeClose(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        let writer = Box::from_raw(ptr as *mut CbzWriter);
        if let Err(e) = writer.finish() {
            throw_book_exception(&mut env, &e);
        }
    }
}
