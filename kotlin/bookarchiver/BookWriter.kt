package bookarchiver

import android.os.ParcelFileDescriptor

class BookWriter(pfd: ParcelFileDescriptor, format: BookFormat) : AutoCloseable {
    // Menyimpan alamat pointer memori native Rust
    private var nativePtr: Long = 0

    init {
        // Mengirimkan file descriptor (fd) dan format ordinal ke Rust
        nativePtr = nativeInit(pfd.fd, format.ordinal)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookWriter untuk format $format")
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
    private native fun nativeWritePage(ptr: Long, pageName: String, data: ByteArray): Boolean
    private native fun nativeClose(ptr: Long)

    companion object {
        init {
            System.loadLibrary("bookarchiver")
        }
    }
}
