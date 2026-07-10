# BookArchiver Native Library (Android JNI)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Android-green.svg)](https://developer.android.com/)

**BookArchiver** adalah library native berkinerja tinggi yang ditulis menggunakan **Rust** dengan antarmuka **JNI (Java Native Interface)** khusus untuk platform **Android**. Library ini dirancang untuk membaca (**Reader**) dan menulis (**Writer**) berbagai format arsip buku komik secara instan menggunakan memori terpetakan (Memory-Mapped file) dan pengindeksan ter-cache.

Untuk mendukung kebijakan Android Scoped Storage modern, library ini bekerja menggunakan **File Descriptor (FD)** numerik dan juga mendukung **Path File/Direktori** lokal secara langsung.

---

## Fitur Utama

- **Pendeteksian Format Otomatis (Magic Bytes)**: Saat membaca berkas, library mendeteksi format berkas secara otomatis dari bytes header file.
- **Dukungan Format Luas**:
  - **CBZ** (Comic Book Zip): Membaca dan menulis secara cepat (deflate compression).
  - **CBT** (Comic Book Tar): Membaca dan menulis dengan performa tinggi (`O(1)` offset-cache lookup untuk random access).
  - **CB7** (Comic Book 7z): Membaca arsip 7z menggunakan library pure Rust `sevenz-rust`.
  - **BBF** (Bound Book Format): Membaca dan menulis format kontainer manga modern berkinerja tinggi (Zero-copy memory mapped parsing).
  - **DIRECTORY** (Folder Gambar): Membaca dan menulis halaman komik langsung dari/ke direktori folder lokal biasa (extracted folder).
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
│   ├── reader.rs             # Implementasi pembaca (Zip, Tar, 7z, BBF, Directory)
│   ├── writer.rs             # Implementasi penulis (Zip, Tar, BBF, Directory)
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
- `BookInitializationException`: Gagal melakukan inisialisasi berkas/FD/Path.
- `BookPageNotFoundException`: Halaman yang diminta tidak ada di arsip/folder.
- `BookIOException`: Kegagalan input-output pada level filesystem/OS.

### 2. Format Berkas (`BookFormat.kt`)
```kotlin
package bookarchiver

enum class BookFormat {
    CBZ,       // ZIP
    CBR,       // RAR (Read-Only)
    CB7,       // 7z (Read-Only)
    CBT,       // TAR
    BBF,       // Bound Book Format
    DIRECTORY  // Folder direktori lokal biasa
}
```

### 3. Membaca Komik (`BookReader.kt`)
Kelas `BookReader` secara otomatis mendeteksi format berkas ketika diinisialisasi.

```kotlin
package bookarchiver

import android.os.ParcelFileDescriptor

class BookReader : AutoCloseable {
    // Membuka berkas komik via Android File Descriptor (Scoped Storage)
    constructor(pfd: ParcelFileDescriptor)
    
    // Membuka berkas arsip atau folder direktori gambar secara langsung via Path lokal
    constructor(path: String)

    // Mendapatkan daftar nama halaman komik (terurut secara alfanumerik)
    fun getPages(): Array<String>
    
    // Membaca byte biner dari suatu halaman gambar
    fun readPage(pageName: String): ByteArray
    
    // Menutup instansi native dan membebaskan memori heap
    override fun close()
}
```

### 4. Menulis Komik (`BookWriter.kt`)
```kotlin
package bookarchiver

import android.os.ParcelFileDescriptor

class BookWriter : AutoCloseable {
    // Menulis berkas komik via Android File Descriptor (Scoped Storage)
    constructor(pfd: ParcelFileDescriptor, format: BookFormat)
    
    // Menulis berkas arsip atau folder direktori gambar secara langsung via Path lokal
    constructor(path: String, format: BookFormat)

    // Menulis byte gambar ke dalam arsip/folder
    fun writePage(pageName: String, data: ByteArray)
    
    // Menyelesaikan penulisan (menulis central directory/footer) dan membebaskan memori
    override fun close()
}
```

---

## Contoh Penggunaan (Kotlin)

### Membaca Folder/Direktori Gambar Sebagai Komik
Aplikasi cukup memberikan path folder gambar (misal folder cache komik hasil download yang sudah diekstrak).
```kotlin
import bookarchiver.BookReader
import bookarchiver.BookPageNotFoundException

fun readComicFromFolder(folderPath: String) {
    // Membuka langsung menggunakan string path lokal
    BookReader(folderPath).use { reader ->
        val pages = reader.getPages()
        println("Halaman terdeteksi di folder: ${pages.size}")
        
        if (pages.isNotEmpty()) {
            val pageBytes = reader.readPage(pages[0])
            // Tampilkan atau render gambar
        }
    }
}
```

### Menulis Komik ke Folder Gambar Lokal
```kotlin
import bookarchiver.BookWriter
import bookarchiver.BookFormat

fun extractOrWriteToFolder(outputPath: String) {
    // BookWriter akan membuat direktori secara otomatis jika belum ada
    BookWriter(outputPath, BookFormat.DIRECTORY).use { writer ->
        val imgData = byteArrayOf(...)
        
        // File 0001.jpg akan ditulis langsung ke folder outputPath
        writer.writePage("0001.jpg", imgData)
    }
}
```

---

## Panduan Kompilasi Silang (Cross-Compilation) ke Android

Library native ini dapat dikompilasi ke library `.so` menggunakan **`cargo-ndk`**.

### Perintah Kompilasi
Jalankan perintah berikut untuk mengompilasi library rilis yang dioptimalkan untuk seluruh arsitektur Android:

```bash
cargo ndk -t aarch64-linux-android -t armv7-linux-androideabi -t x86_64-linux-android -o ./jniLibs build --release
```

Library `.so` yang teroptimasi akan dihasilkan pada folder `./jniLibs/` yang dapat langsung disalin ke folder `src/main/jniLibs/` pada proyek Android Studio Anda.

---

## Aturan ProGuard / R8 (`proguard-rules.pro`)

Tambahkan aturan berikut ke file ProGuard proyek Android Anda agar kelas dan eksepsi tidak di-obfuscate atau di-strip:

```proguard
-keep class bookarchiver.BookReader {
    private native <methods>;
    *** nativePtr;
}

-keep class bookarchiver.BookWriter {
    private native <methods>;
    *** nativePtr;
}

-keep class bookarchiver.BookFormat {
    **[] $VALUES;
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

-keep class bookarchiver.BookException { *; }
-keep class bookarchiver.BookInitializationException { *; }
-keep class bookarchiver.BookPageNotFoundException { *; }
-keep class bookarchiver.BookIOException { *; }
```

---

## Lisensi

Proyek ini dilisensikan di bawah **[MIT License](LICENSE)**.
