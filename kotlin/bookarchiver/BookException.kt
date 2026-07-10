package bookarchiver

import java.io.IOException

/**
 * Exception dasar untuk semua kesalahan operasi arsip buku (Book Archiver).
 */
open class BookException(message: String, cause: Throwable? = null) : Exception(message, cause)

/**
 * Dilemparkan ketika inisialisasi native reader/writer gagal,
 * misalnya karena File Descriptor tidak valid atau tidak didukung.
 */
class BookInitializationException(message: String) : BookException(message)

/**
 * Dilemparkan ketika halaman tertentu tidak ditemukan di dalam arsip.
 */
class BookPageNotFoundException(message: String) : BookException(message)

/**
 * Dilemparkan ketika terjadi error input-output pada level filesystem/OS.
 */
class BookIOException(message: String, cause: Throwable? = null) : BookException(message, cause)
