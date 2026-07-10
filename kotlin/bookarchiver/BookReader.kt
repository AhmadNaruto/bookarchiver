package bookarchiver

import android.os.ParcelFileDescriptor

class BookReader(pfd: ParcelFileDescriptor) : AutoCloseable {
    // Menyimpan alamat pointer memori native Rust
    private var nativePtr: Long = 0

    init {
        // Mengirimkan file descriptor (fd) ke Rust
        nativePtr = nativeInit(pfd.fd)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookReader")
        }
    }

    fun getPages(): Array<String> {
        check(nativePtr != 0L) { "Reader sudah ditutup" }
        return nativeGetPages(nativePtr)
    }

    fun readPage(pageName: String): ByteArray {
        check(nativePtr != 0L) { "Reader sudah ditutup" }
        return nativeReadPage(nativePtr, pageName)
            ?: throw BookPageNotFoundException("Gagal membaca halaman: $pageName")
    }

    override fun close() {
        if (nativePtr != 0L) {
            nativeClose(nativePtr)
            nativePtr = 0L
        }
    }

    // --- Native Link ---
    private native fun nativeInit(fd: Int): Long
    private native fun nativeGetPages(ptr: Long): Array<String>
    private native fun nativeReadPage(ptr: Long, pageName: String): ByteArray?
    private native fun nativeClose(ptr: Long)

    companion object {
        init {
            System.loadLibrary("bookarchiver")
        }
    }
}
