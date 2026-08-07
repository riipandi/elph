# Daftar Tools & Cara Penggunaan

Daftar lengkap tools yang tersedia dan cara menggunakannya.

---

## Daftar Isi

1. [create_dir](#create_dir)
2. [write_file](#write_file)
3. [read_file](#read_file)
4. [edit_file](#edit_file)
5. [delete_path](#delete_path)
6. [find_path](#find_path)
7. [grep](#grep)
8. [list_dir](#list_dir)
9. [copy_path](#copy_path)
10. [move_path](#move_path)
11. [shell_exec](#shell_exec)
12. [web_search](#web_search)
13. [web_fetch](#web_fetch)
14. [spawn_agent](#spawn_agent)

---

## create_dir

Membuat direktori baru beserta seluruh parent direktori yang diperlukan (seperti `mkdir -p`).

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                       |
| --------- | ------ | ----- | ------------------------------- |
| `path`    | string | ✅    | Path direktori yang akan dibuat |

**Contoh:**

Membuat direktori bertingkat:

```
create_dir path="src/components/ui"
```

Akan membuat direktori `src`, `src/components`, `src/components/ui` jika belum ada.

---

## write_file

Membuat file baru atau menimpa file yang sudah ada dengan konten baru. Parent direktori akan dibuat otomatis jika belum ada.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                                          |
| --------- | ------ | ----- | -------------------------------------------------- |
| `path`    | string | ✅    | Path file yang akan ditulis                        |
| `content` | string | ✅    | Konten file (bisa berupa teks biasa atau markdown) |

**Contoh:**

```
write_file path="src/main.rs" content="fn main() {
    println!(\"Hello, world!\");
}"
```

**Catatan:** Tool ini **menimpa** seluruh isi file. Gunakan `edit_file` jika hanya ingin mengubah sebagian kecil konten.

---

## read_file

Membaca isi file dan menampilkannya. Output dibatasi hingga 2000 baris atau 50 KB.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                                                                  |
| --------- | ------ | ----- | -------------------------------------------------------------------------- |
| `path`    | string | ✅    | Path file yang akan dibaca                                                 |
| `offset`  | number | ❌    | Baris awal pembacaan (1-indexed). Lewati untuk mulai dari awal.            |
| `limit`   | number | ❌    | Jumlah maksimal baris yang dibaca. Lewati untuk membaca semua (maks 2000). |

**Contoh:**

Baca file dari awal:

```
read_file path="Cargo.toml"
```

Baca file mulai baris 10, ambil 20 baris:

```
read_file path="src/main.rs" offset=10 limit=20
```

---

## edit_file

Mengubah konten file dengan mencari teks tertentu dan menggantinya. Berguna untuk perubahan kecil tanpa menimpa seluruh file.

**Parameter:**

| Parameter    | Tipe   | Wajib | Deskripsi                                                         |
| ------------ | ------ | ----- | ----------------------------------------------------------------- |
| `path`       | string | ✅    | Path file yang akan diedit                                        |
| `old_string` | string | ✅    | Teks yang akan diganti (**harus cocok persis satu kali** di file) |
| `new_string` | string | ✅    | Teks pengganti                                                    |

**Aturan penting:**

- `old_string` harus cocok **persis** (termasuk spasi, indentasi, newline).
- `old_string` hanya boleh muncul **tepat satu kali** di file — jika ada duplikat, tool akan gagal.
- Untuk perubahan besar, lebih baik baca file dulu dengan `read_file`, lalu tulis ulang dengan `write_file`.

**Contoh:**

Ganti satu baris:

```
edit_file path="src/main.rs"
    old_string="let x = 1;"
    new_string="let x = 2;"
```

Ganti blok multi-baris:

```
edit_file path="src/lib.rs"
    old_string="fn old_function() {
    // old code
}"
    new_string="fn new_function() {
    // new code
}"
```

---

## delete_path

Menghapus file atau direktori beserta seluruh isinya secara rekursif.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                                  |
| --------- | ------ | ----- | ------------------------------------------ |
| `path`    | string | ✅    | Path file atau direktori yang akan dihapus |

**Contoh:**

Hapus file:

```
delete_path path="src/old_file.rs"
```

Hapus direktori beserta isinya:

```
delete_path path="dist/"
```

**Peringatan:** Tool ini **tidak reversible** — file/direktori akan langsung terhapus.

---

## find_path

Mencari file dengan pola glob dan mengembalikan daftar path yang cocok secara alfabetis.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                                             |
| --------- | ------ | ----- | ----------------------------------------------------- |
| `pattern` | string | ✅    | Pola glob (misal: `*.rs`, `**/*.toml`, `src/**/*.ts`) |
| `path`    | string | ❌    | Direktori pencarian (default: working directory)      |
| `limit`   | number | ❌    | Jumlah maksimal hasil                                 |

**Pola glob umum:**

| Pola          | Keterangan                                           |
| ------------- | ---------------------------------------------------- |
| `*.rs`        | Semua file `.rs` di direktori saat ini               |
| `**/*.rs`     | Semua file `.rs` di semua subdirektori               |
| `src/**/*.ts` | Semua file `.ts` di dalam `src/` dan subdirektorinya |
| `*.{rs,toml}` | File dengan ekstensi `.rs` atau `.toml`              |

**Contoh:**

Cari semua file Rust:

```
find_path pattern="**/*.rs"
```

Cari file TOML di direktori tertentu:

```
find_path pattern="*.toml" path="src/config"
```

---

## grep

Mencari teks atau pola regex di dalam file-file di suatu direktori.

**Parameter:**

| Parameter    | Tipe    | Wajib | Deskripsi                                          |
| ------------ | ------- | ----- | -------------------------------------------------- |
| `pattern`    | string  | ✅    | Pola pencarian (regex atau teks literal)           |
| `path`       | string  | ✅    | Direktori atau file yang akan dicari               |
| `ignoreCase` | boolean | ❌    | Abaikan perbedaan huruf besar/kecil                |
| `literal`    | boolean | ❌    | Perlakukan pola sebagai teks literal (bukan regex) |
| `limit`      | number  | ❌    | Jumlah maksimal hasil                              |

**Contoh:**

Cari teks literal (case-sensitive):

```
grep pattern="TODO" path="src"
```

Cari dengan regex (abaikan case):

```
grep pattern="fn [a-z_]+" path="src/lib.rs"
```

Cari literal — escape regex characters:

```
grep pattern="some.nested.field" path="src" literal=true
```

**Tips:** Jika pola mengandung karakter regex khusus (seperti `.`, `*`, `+`), set `literal=true` agar dicari sebagai teks biasa.

---

## list_dir

Menampilkan daftar file dan direktori di dalam suatu path.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                             |
| --------- | ------ | ----- | ------------------------------------- |
| `path`    | string | ✅    | Path direktori yang akan didaftar     |
| `limit`   | number | ❌    | Jumlah maksimal item yang ditampilkan |

**Contoh:**

```
list_dir path="src"
```

```
list_dir path="." limit=10
```

---

## copy_path

Menyalin file atau direktori beserta seluruh isinya secara rekursif.

**Parameter:**

| Parameter     | Tipe   | Wajib | Deskripsi   |
| ------------- | ------ | ----- | ----------- |
| `source`      | string | ✅    | Path sumber |
| `destination` | string | ✅    | Path tujuan |

**Contoh:**

Salin file:

```
copy_path source="src/main.rs" destination="src/main_backup.rs"
```

Salin direktori:

```
copy_path source="src/" destination="src_copy/"
```

---

## move_path

Memindahkan atau mengganti nama file/direktori. Jika hanya filename yang berbeda, tool akan melakukan rename.

**Parameter:**

| Parameter     | Tipe   | Wajib | Deskripsi   |
| ------------- | ------ | ----- | ----------- |
| `source`      | string | ✅    | Path sumber |
| `destination` | string | ✅    | Path tujuan |

**Contoh:**

Renama file:

```
move_path source="src/old_name.rs" destination="src/new_name.rs"
```

Pindahkan file ke direktori lain:

```
move_path source="src/temp.rs" destination="src/utils/helper.rs"
```

Pindahkan direktori:

```
move_path source="old_dir/" destination="new_dir/"
```

---

## shell_exec

Menjalankan perintah shell di working directory. Output dibatasi hingga 2000 baris atau 50 KB.

**Parameter:**

| Parameter           | Tipe    | Wajib | Deskripsi                                                                                  |
| ------------------- | ------- | ----- | ------------------------------------------------------------------------------------------ |
| `command`           | string  | ✅    | Perintah shell yang akan dijalankan                                                        |
| `timeout`           | number  | ❌    | Timeout dalam detik                                                                        |
| `run_in_background` | boolean | ❌    | Jalankan sebagai task latar belakang; kembali langsung dengan task ID dan path file output |
| `disable_timeout`   | boolean | ❌    | Hapus batas timeout (berlaku untuk foreground maupun background)                           |
| `description`       | string  | ❌*   | Deskripsi task; **wajib** bila `run_in_background=true`                                    |

\* `description` wajib ketika `run_in_background=true`.

**Contoh:**

```
shell_exec command="cargo build"
```

```
shell_exec command="ls -la" timeout=10
```

```
shell_exec command="cargo test -- --nocapture" timeout=120
```

```
shell_exec command="cargo watch -x build" run_in_background=true description="build watcher"
```

**Background & timeout:**

- `run_in_background=true` menjalankan perintah secara terlepas (detached) dan mengembalikan
  task ID beserta path file log output; agen dapat membaca file tersebut nanti via `read_file`.
- Task background secara default memiliki timeout **10 menit** (600 detik) di mode interaktif,
  namun **tanpa batas** di mode headless `elph run`.
- `disable_timeout=true` menghapus batas timeout untuk foreground maupun background.
- `timeout` eksplisit (detik) tetap menang atas default di atas.

**Persistensi output:**

Setiap eksekusi `shell_exec` (foreground maupun background) menyimpan output mentahnya ke
file di direktori terminal sesi: `~/.local/share/elph/sessions/<SESSION_ID>/terminals/*.txt`
— foreground memakai nama `shell-<toolCallId>.txt`, background `shell-<taskId>.txt`. Path file
tersebut dikembalikan lewat `details.outputPath` dan direferensikan pula di `tool_outputs.jsonl`
(transkrip sesi) agar output tetap tersedia saat sesi dilanjutkan. Tanpa sesi persisten (mis. di
tes), output hanya disimpan ke file temporer dan `outputPath` tidak diisi.

**Catatan:** Tool ini berguna untuk menjalankan build, test, linting, git operations, dan perintah shell lainnya.

---

## web_search

Mencari informasi di web, memberikan hasil berupa snippet dan link. Mendukung multiple search engines.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                                                                                                             |
| --------- | ------ | ----- | --------------------------------------------------------------------------------------------------------------------- |
| `query`   | string | ✅    | Kata kunci pencarian                                                                                                  |
| `engine`  | string | ❌    | Mesin pencari: `auto` (default), `duckduckgo`, `brave`, `exa`, `firecrawl`, `jina`, `perplexity`, `tavily`, `serpapi` |
| `limit`   | number | ❌    | Jumlah maksimal hasil (default: 5, maks: 20)                                                                          |

**Contoh:**

Pencarian sederhana:

```
web_search query="rust async await tutorial"
```

Gunakan engine spesifik:

```
web_search query="latest tailwind css version" engine="duckduckgo"
```

---

## web_fetch

Mengambil konten dari URL dan mengembalikannya dalam format Markdown (HTML dikonversi ke teks biasa). Berguna untuk membaca dokumentasi.

**Parameter:**

| Parameter | Tipe   | Wajib | Deskripsi                           |
| --------- | ------ | ----- | ----------------------------------- |
| `url`     | string | ✅    | URL (HTTP/HTTPS) yang akan di-fetch |

**Contoh:**

```
web_fetch url="https://doc.rust-lang.org/book/"
```

```
web_fetch url="https://crates.io/api/v1/crates/serde"
```

**Catatan:** HTML dikonversi ke Markdown dengan `htmd`; halaman yang bergantung pada JavaScript dieksekusi (JS-rendered) dikembalikan apa adanya, tidak melalui browser in-process.

---

## spawn_agent

Membuat sub-agent baru untuk menangani tugas yang terfokus dalam konteks yang terisolasi. Berguna untuk parallel task atau delegasi.

**Parameter:**

| Parameter   | Tipe   | Wajib | Deskripsi                          |
| ----------- | ------ | ----- | ---------------------------------- |
| `task_name` | string | ✅    | Label singkat untuk task sub-agent |
| `message`   | string | ❌    | Instruksi awal opsional            |

**Contoh:**

```
spawn_agent task_name="check-types"
spawn_agent task_name="refactor-auth" message="Refactor auth module to use new middleware pattern"
```

**Catatan:** Tool ini hanya membuat agent — gunakan `followup_task` untuk mengirim instruksi dan menjalankan turn sub-agent, `wait_agent` untuk menunggu sub-agent selesai, `send_message` untuk mengirim pesan tanpa turn, dan `list_agents` untuk melihat daftar sub-agent aktif.

---

## Ringkasan Cepat

| Tool          | Fungsi Utama                          |
| ------------- | ------------------------------------- |
| `create_dir`  | Buat direktori (mkdir -p)             |
| `write_file`  | Buat/timpa file dengan konten         |
| `read_file`   | Baca isi file                         |
| `edit_file`   | Edit sebagian file (cari & ganti)     |
| `delete_path` | Hapus file/direktori                  |
| `find_path`   | Cari file dengan glob pattern         |
| `grep`        | Cari teks/regex di dalam file         |
| `list_dir`    | Lihat isi direktori                   |
| `copy_path`   | Salin file/direktori                  |
| `move_path`   | Pindahkan/renama file/direktori       |
| `shell_exec`  | Jalankan perintah shell               |
| `web_search`  | Cari informasi di web                 |
| `web_fetch`   | Ambil konten URL                      |
| `spawn_agent` | Buat sub-agent untuk tugas terisolasi |
