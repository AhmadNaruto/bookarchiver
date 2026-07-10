package bookarchiver

import android.os.ParcelFileDescriptor

class BookReader : AutoCloseable {
    // Menyimpan alamat pointer memori native Rust
    private var nativePtr: Long = 0

    // Constructor untuk membuka berkas via Android File Descriptor (Scoped Storage)
    constructor(pfd: ParcelFileDescriptor) {
        nativePtr = nativeInit(pfd.fd)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookReader")
        }
    }

    // Constructor untuk membuka direktori lokal atau path berkas arsip secara langsung
    constructor(path: String) {
        nativePtr = nativeInitPath(path)
        if (nativePtr == 0L) {
            throw BookInitializationException("Gagal menginisialisasi Native BookReader dengan path: $path")
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
    private native fun nativeInitPath(path: String): Long
    private native fun nativeGetPages(ptr: Long): Array<String>
    private native fun nativeReadPage(ptr: Long, pageName: String): ByteArray?
    private native fun nativeClose(ptr: Long)

    companion object {
        init {
            System.loadLibrary("cbz_native")
        }
    }
}
