package bookarchiver

import android.os.ParcelFileDescriptor

class BookWriter : AutoCloseable {
    // Menyimpan alamat pointer memori native Rust
    private var nativePtr: Long = 0

    // Constructor untuk menulis berkas via Android File Descriptor (Scoped Storage)
    constructor(pfd: ParcelFileDescriptor, format: BookFormat) {
        nativePtr = nativeInit(pfd.fd, format.ordinal)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookWriter untuk format $format")
        }
    }

    // Constructor untuk menulis berkas arsip atau direktori folder lokal secara langsung
    constructor(path: String, format: BookFormat) {
        nativePtr = nativeInitPath(path, format.ordinal)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookWriter dengan path $path untuk format $format")
        }
    }

    fun writePage(pageName: String, data: ByteArray) {
        check(nativePtr != 0L) { "Writer sudah ditutup" }
        if (!nativeWritePage(nativePtr, pageName, data)) {
            throw BookIOException("Gagal menulis halaman: $pageName")
        }
    }

    override fun close() {
        if (nativePtr != 0L) {
            nativeClose(nativePtr)
            nativePtr = 0L
        }
    }

    // --- Native Link ---
    private native fun nativeInit(fd: Int, formatOrdinal: Int): Long
    private native fun nativeInitPath(path: String, formatOrdinal: Int): Long
    private native fun nativeWritePage(ptr: Long, pageName: String, data: ByteArray): Boolean
    private native fun nativeClose(ptr: Long)

    companion object {
        init {
            System.loadLibrary("cbz_native")
        }
    }
}
