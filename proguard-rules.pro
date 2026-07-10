# ProGuard / R8 Rules for BookArchiver Native Library

# Mencegah obfuscation dan stripping pada kelas JNI BookReader dan BookWriter beserta method native-nya
-keep class bookarchiver.BookReader {
    private native <methods>;
    *** nativePtr;
}

-keep class bookarchiver.BookWriter {
    private native <methods>;
    *** nativePtr;
}

# Mempertahankan enum BookFormat agar nilainya tidak berubah
-keep class bookarchiver.BookFormat {
    **[] $VALUES;
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

# Sangat penting: Pertahankan kelas-kelas eksepsi kustom.
# Rust JNI menggunakan pencarian nama kelas berbasis string (misal: "bookarchiver/BookPageNotFoundException").
# Jika kelas-kelas ini di-obfuscate oleh R8, Rust tidak akan dapat menemukan dan melempar eksepsi ini di runtime.
-keep class bookarchiver.BookException { *; }
-keep class bookarchiver.BookInitializationException { *; }
-keep class bookarchiver.BookPageNotFoundException { *; }
-keep class bookarchiver.BookIOException { *; }
