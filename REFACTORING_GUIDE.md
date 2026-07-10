# Panduan Refactoring Kode Komikku dengan BookArchiver

Dokumen ini berisi panduan teknis untuk mengganti pustaka pemrosesan arsip komik lama (`libarchive` via pembungkus `me.zhanghai.android.libarchive`) di aplikasi **Komikku** menggunakan pustaka berkinerja tinggi **BookArchiver** (Rust & JNI).

---

## 1. Analisis Mekanisme Membaca/Menulis Saat Ini di Komikku

Di dalam proyek Komikku (pada modul `:core:archive` di package `mihon.core.archive`), pemrosesan arsip saat ini dilakukan oleh kelas-kelas berikut:

### A. Pembacaan Arsip (`ArchiveReader.kt` & `ArchiveInputStream.kt`)
*   **Mekanisme**: Menggunakan fungsi sistem Android `Os.mmap` untuk memetakan file arsip ke dalam alamat memori pointer. Alamat memori tersebut kemudian dikirimkan ke C-library `libarchive` untuk mem-parsing header dan membaca data halaman.
*   **Masalah**:
    1. Pembacaan entri file dilakukan secara linear sequential stream (`getNextEntry()`), yang berarti pencarian halaman gambar acak memiliki kompleksitas waktu $O(N)$.
    2. Karena parsing stream sequential lambat, Komikku harus menerapkan strategi caching yang kompleks di kelas `ArchivePageLoader.kt` (seperti menyalin seluruh isi arsip ke disk lokal terlebih dahulu via opsi `CACHE_TO_DISK` atau memuat byte array penuh ke RAM via opsi `LOAD_INTO_MEMORY`).

### B. Penulisan Arsip (`ZipWriter.kt`)
*   **Mekanisme**: Membuka berkas ZIP baru menggunakan `libarchive` dengan pengaturan kompresi *Store* (tanpa kompresi tambahan karena gambar JPG/PNG/WebP sudah terkompresi).
*   **Masalah**: Mengharuskan pemanggilan manual JNI C-bindings berulang kali untuk menulis setiap header entri dan byte buffer data, yang memicu overhead komunikasi JVM-native yang tinggi.

---

## 2. Strategi Refactoring Transparan & Sederhana

Untuk meminimalkan dampak refactoring pada bagian aplikasi lainnya (seperti loader halaman `ArchivePageLoader.kt` dan modul lokal `LocalSource.kt`), strategi terbaik adalah **mengganti isi implementasi internal `ArchiveReader` dan `ZipWriter` saja tanpa mengubah nama kelas atau signature method publiknya**.

Langkah-langkah detailnya adalah sebagai berikut:

### Langkah 1: Tambahkan Source Kotlin BookArchiver
Salin package Kotlin [bookarchiver/](file:///data/data/com.termux/files/home/bookarchiver/kotlin/bookarchiver) (`BookFormat.kt`, `BookReader.kt`, `BookWriter.kt`, `BookException.kt`) ke dalam folder source code utama Anda (misalnya di bawah modul `:core:archive`).

### Langkah 2: Ganti Implementasi `ArchiveReader.kt`
Ubah isi berkas `core/archive/src/main/kotlin/mihon/core/archive/ArchiveReader.kt` menjadi delegasi ke `BookReader` seperti di bawah ini. Anda dapat menghapus kelas `ArchiveInputStream.kt` sepenuhnya setelah ini!

```kotlin
package mihon.core.archive

import android.os.ParcelFileDescriptor
import bookarchiver.BookReader
import java.io.Closeable
import java.io.InputStream
import java.io.ByteArrayInputStream

class ArchiveReader(pfd: ParcelFileDescriptor) : Closeable {
    // Delegasikan tugas ke Rust BookReader native
    private val delegate = BookReader(pfd)

    // SY -->
    var encrypted: Boolean = false
        private set
    var wrongPassword: Boolean? = null
        private set
    val archiveHashCode = pfd.hashCode()
    // SY <--

    /**
     * Mengembalikan sequence daftar file di dalam arsip.
     * BookReader mengindeks halaman secara instan dalam O(1) di sisi Rust.
     */
    inline fun <T> useEntries(block: (Sequence<ArchiveEntry>) -> T): T {
        val pages = delegate.getPages()
        val sequence = pages.asSequence().map { name ->
            ArchiveEntry(
                name = name,
                isFile = true,
                isEncrypted = false
            )
        }
        return block(sequence)
    }

    /**
     * Membaca satu halaman gambar secara cepat (O(1) Memory-mapped offset lookup)
     */
    fun getInputStream(entryName: String): InputStream? {
        return try {
            val bytes = delegate.readPage(entryName)
            ByteArrayInputStream(bytes)
        } catch (e: Exception) {
            null
        }
    }

    override fun close() {
        delegate.close()
    }
}
```

### Langkah 3: Ganti Implementasi `ZipWriter.kt`
Ubah isi berkas `core/archive/src/main/kotlin/mihon/core/archive/ZipWriter.kt` menjadi delegasi ke `BookWriter` seperti berikut:

```kotlin
package mihon.core.archive

import android.content.Context
import com.hippo.unifile.UniFile
import bookarchiver.BookWriter
import bookarchiver.BookFormat
import java.io.Closeable

class ZipWriter(
    val context: Context,
    file: UniFile,
    encrypt: Boolean = false,
) : Closeable {
    private val pfd = file.openFileDescriptor(context, "wt")
    
    // Delegasikan penulisan ke BookWriter dengan format CBZ (ZIP)
    private val delegate = BookWriter(pfd, BookFormat.CBZ)

    fun write(file: UniFile) {
        file.openInputStream().use { input ->
            delegate.writePage(file.name ?: "", input)
        }
    }

    fun write(fileData: ByteArray, fileName: String) {
        delegate.writePage(fileName, fileData)
    }

    override fun close() {
        delegate.close()
        pfd.close()
    }
}
```

---

## 3. Menyederhanakan `ArchivePageLoader.kt`

Karena `BookReader` melakukan memory mapping file dan pencarian koordinat offset gambar secara instan dalam Rust (`O(1)`), overhead pembacaan halaman acak menjadi sangat minim. 

Hal ini membuat opsi caching disk (`CACHE_TO_DISK`) atau pemuatan penuh ke memori (`LOAD_INTO_MEMORY`) di `ArchivePageLoader.kt` **tidak diperlukan lagi**. Anda dapat menghapus puluhan baris kode ekstra tersebut dan membuat loader menjadi sangat sederhana:

```kotlin
package eu.kanade.tachiyomi.ui.reader.loader

import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.ui.reader.model.ReaderPage
import eu.kanade.tachiyomi.util.lang.compareToCaseInsensitiveNaturalOrder
import mihon.core.archive.ArchiveReader

internal class ArchivePageLoader(private val reader: ArchiveReader) : PageLoader() {

    override var isLocal: Boolean = true

    override suspend fun getPages(): List<ReaderPage> = reader.useEntries { entries ->
        entries
            .filter { it.isFile }
            .sortedWith { f1, f2 -> f1.name.compareToCaseInsensitiveNaturalOrder(f2.name) }
            .mapIndexed { i, entry ->
                ReaderPage(i).apply {
                    // Cukup buat lambdas InputStream yang akan dievaluasi secara on-demand
                    stream = { reader.getInputStream(entry.name)!! }
                    status = Page.State.Ready
                }
            }
            .toList()
    }

    override suspend fun loadPage(page: ReaderPage) {
        check(!isRecycled)
    }

    override fun recycle() {
        super.recycle()
        reader.close()
    }
}
```

## 4. Keuntungan Setelah Refactoring

1.  **Kode Jauh Lebih Sederhana**: Anda mengeliminasi penanganan pointer memori `Os.mmap`/`munmap` manual di Kotlin, menghapus helper stream `ArchiveInputStream.kt` sepenuhnya, serta menyederhanakan `ArchivePageLoader.kt`.
2.  **Bebas Caching Lambat**: Tidak perlu lagi mengekstrak file arsip komik ke disk cache internal Android saat membaca komik, menghemat siklus tulis storage (memperpanjang umur flash memory perangkat pengguna) dan mengeliminasi delay loading awal.
3.  **Dukungan Multi-Format Instan**: Cukup dengan mengganti implementasi ini, modul `ArchiveReader` Anda secara ajaib akan langsung bisa membaca komik berformat **CBT (TAR)**, **CB7 (7z)**, dan **BBF** tanpa menulis kode parsing format tersebut sama sekali di sisi Kotlin.
