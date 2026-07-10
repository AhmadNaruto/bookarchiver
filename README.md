# BookArchiver Native Library (Android JNI)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Android-green.svg)](https://developer.android.com/)

**BookArchiver** adalah library native berkinerja tinggi yang ditulis menggunakan **Rust** dengan antarmuka **JNI (Java Native Interface)** khusus untuk platform **Android**. Library ini dirancang untuk membaca (**Reader**) dan menulis (**Writer**) berbagai format arsip buku komik secara instan menggunakan memori terpetakan (Memory-Mapped file) dan pengindeksan ter-cache.

Untuk mendukung kebijakan Android Scoped Storage modern, library ini bekerja sepenuhnya menggunakan **File Descriptor (FD)** numerik, bukan direct file path.

---

## Fitur Utama

- **Pendeteksian Format Otomatis (Magic Bytes)**: Saat membaca berkas, library mendeteksi format berkas secara otomatis dari bytes header file.
- **Dukungan Format Luas**:
  - **CBZ** (Comic Book Zip): Membaca dan menulis secara cepat (deflate compression).
  - **CBT** (Comic Book Tar): Membaca dan menulis dengan performa tinggi (`O(1)` offset-cache lookup untuk random access).
  - **CB7** (Comic Book 7z): Membaca arsip 7z menggunakan library pure Rust `sevenz-rust`.
  - **BBF** (Bound Book Format): Membaca dan menulis format kontainer manga modern berkinerja tinggi (Zero-copy memory mapped parsing).
  - **CBR** (Comic Book RAR): Dukungan placeholder/stub untuk pembacaan berkas RAR.
- **Keamanan Memori & Stabilitas**:
  - Manajemen referensi lokal JNI secara otomatis untuk mencegah kegagalan JVM (`Local Reference Table Overflow`) saat memproses komik dengan ratusan halaman.
  - JNI bridge yang aman dari crash (panic-free), menerjemahkan kesalahan native menjadi eksepsi Kotlin terstruktur.

---

## Struktur File Project

```text
bookarchiver/
├── Cargo.toml                # Konfigurasi Rust dependencies & release profile
├── LICENSE                   # Lisensi MIT
├── proguard-rules.pro        # Aturan ProGuard/R8 untuk Android
├── src/                      # Source code Rust
│   ├── lib.rs                # JNI Bridge entrypoints & exception mapping
│   ├── reader.rs             # Implementasi pembaca (Zip, Tar, 7z, BBF)
│   ├── writer.rs             # Implementasi penulis (Zip, Tar, BBF)
│   └── error.rs              # Representasi error native (CbzError)
└── kotlin/bookarchiver/      # Berkas kelas Kotlin
    ├── BookFormat.kt         # Enum format komik
    ├── BookReader.kt         # Wrapper pembaca komik
    ├── BookWriter.kt         # Wrapper penulis komik
    └── BookException.kt      # Hierarki exception terstruktur
```

---

## Dokumentasi API Kotlin

### 1. Eksepsi Kustom (`BookException.kt`)
Semua operasi native yang gagal akan dilemparkan sebagai sub-kelas dari `BookException`:
- `BookInitializationException`: Dilemparkan jika gagal melakukan inisialisasi berkas/FD.
- `BookPageNotFoundException`: Dilemparkan jika halaman yang diminta tidak ada di arsip.
- `BookIOException`: Dilemparkan jika terjadi kegagalan input-output pada level filesystem/OS.

### 2. Format Berkas (`BookFormat.kt`)
```kotlin
package bookarchiver

enum class BookFormat {
    CBZ, // ZIP
    CBR, // RAR (Read-Only)
    CB7, // 7z (Read-Only)
    CBT, // TAR
    BBF  // Bound Book Format (Manga container)
}
```

### 3. Membaca Komik (`BookReader.kt`)
Kelas `BookReader` secara otomatis mendeteksi format berkas (CBZ, CBT, CB7, BBF) ketika diinisialisasi.

```kotlin
package bookarchiver

import android.os.ParcelFileDescriptor

class BookReader(pfd: ParcelFileDescriptor) : AutoCloseable {
    // Mendapatkan daftar nama halaman komik (terurut secara alfanumerik)
    fun getPages(): Array<String>
    
    // Membaca byte biner dari suatu halaman gambar
    fun readPage(pageName: String): ByteArray
    
    // Menutup instansi native dan membebaskan memori heap
    override fun close()
}
```

### 4. Menulis Komik (`BookWriter.kt`)
Kelas `BookWriter` digunakan untuk mengemas berkas gambar menjadi format komik tertentu.

```kotlin
package bookarchiver

import android.os.ParcelFileDescriptor

class BookWriter(pfd: ParcelFileDescriptor, format: BookFormat) : AutoCloseable {
    // Menulis byte gambar ke dalam arsip
    fun writePage(pageName: String, data: ByteArray)
    
    // Menyelesaikan penulisan (menulis central directory/footer) dan membebaskan memori
    override fun close()
}
```

---

## Contoh Penggunaan (Kotlin)

### Membaca File Komik
```kotlin
import android.content.Context
import android.net.Uri
import bookarchiver.BookReader
import bookarchiver.BookPageNotFoundException

fun readComic(context: Context, uri: Uri) {
    // Membuka file descriptor dari ContentResolver Android
    val pfd = context.contentResolver.openFileDescriptor(uri, "r") ?: return
    
    pfd.use { fd ->
        BookReader(fd).use { reader ->
            // Mengambil daftar semua halaman
            val pages = reader.getPages()
            println("Total Halaman: ${pages.size}")
            
            // Membaca isi halaman pertama
            if (pages.isNotEmpty()) {
                try {
                    val pageBytes = reader.readPage(pages[0])
                    // Proses data gambar (misal decode ke Bitmap)
                } catch (e: BookPageNotFoundException) {
                    e.printStackTrace()
                }
            }
        }
    }
}
```

### Membuat/Menulis File Komik Baru (Format BBF atau CBZ)
```kotlin
import android.content.Context
import android.net.Uri
import bookarchiver.BookWriter
import bookarchiver.BookFormat

fun createComicBook(context: Context, outputUri: Uri) {
    val pfd = context.contentResolver.openFileDescriptor(outputUri, "w") ?: return
    
    pfd.use { fd ->
        // Membuat arsip komik baru dengan format Bound Book Format (BBF)
        BookWriter(fd, BookFormat.BBF).use { writer ->
            val page1Data = byteArrayOf(...) // Byte data gambar 1
            val page2Data = byteArrayOf(...) // Byte data gambar 2
            
            writer.writePage("0001.jpg", page1Data)
            writer.writePage("0002.png", page2Data)
            
            // Saat blok `use` berakhir, `close()` dipanggil secara otomatis
            // dan metadata serta footer ZIP/BBF ditulis dengan aman.
        }
    }
}
```

---

## Panduan Kompilasi Silang (Cross-Compilation) ke Android

Library native ini dapat dikompilasi ke library `.so` menggunakan **`cargo-ndk`**.

### Prasyarat
1. Install Rust melalui rustup.
2. Install Android NDK (melalui SDK Manager di Android Studio).
3. Install target arsitektur Android:
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
   ```
4. Install `cargo-ndk` tool:
   ```bash
   cargo install cargo-ndk
   ```

### Perintah Kompilasi
Jalankan perintah berikut untuk mengompilasi library rilis yang dioptimalkan untuk seluruh arsitektur Android:

```bash
cargo ndk -t aarch64-linux-android -t armv7-linux-androideabi -t x86_64-linux-android -o ./jniLibs build --release
```

Library `.so` yang teroptimasi akan dihasilkan pada folder `./jniLibs/` yang dapat langsung disalin ke folder `src/main/jniLibs/` pada proyek Android Studio Anda.

---

## Aturan ProGuard / R8 (`proguard-rules.pro`)

Karena komunikasi JNI dilakukan secara dinamis menggunakan reflection, tambahkan aturan berikut ke file ProGuard proyek Android Anda agar kelas dan eksepsi tidak di-obfuscate atau di-strip:

```proguard
# Mempertahankan kelas dan native method BookArchiver
-keep class bookarchiver.BookReader {
    private native <methods>;
    *** nativePtr;
}

-keep class bookarchiver.BookWriter {
    private native <methods>;
    *** nativePtr;
}

# Mempertahankan enum BookFormat
-keep class bookarchiver.BookFormat {
    **[] $VALUES;
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

# Mempertahankan kelas-kelas eksepsi agar dapat dicari dan dilemparkan oleh Rust JNI
-keep class bookarchiver.BookException { *; }
-keep class bookarchiver.BookInitializationException { *; }
-keep class bookarchiver.BookPageNotFoundException { *; }
-keep class bookarchiver.BookIOException { *; }
```

---

## Lisensi

Proyek ini dilisensikan di bawah **[MIT License](LICENSE)**.
