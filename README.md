# Local Server

A tiny "Cloudflare" for every computer. It keeps your sites and apps and opens them
**without internet**, and can also serve phones and computers on the same Wi-Fi, still
without internet. Each person keeps what matters to them.

It also has a **site archive**: paste a website address and the server keeps a copy you
can view offline. It checks from time to time whether the site changed and stores a new
dated version, without deleting the old ones (like a private Wayback Machine).

- Written in Rust using **only the standard library** (no third-party code).
- HTTPS uses the `curl` that already ships with the system (Windows 10/11, Mac, Linux).
- Keeps no record of who visits. The program is under 1 MB and runs on old computers.
- Interface in **20 languages**, picked from your browser and system language.
- Free software (GPL-3.0-or-later).

## Download

Get the ready-made program from the **Releases** section of this repository:

| Computer | File |
|---|---|
| Windows 10 or 11 | `local-server-windows.exe` |
| Windows 7 / 8 (64-bit) | `local-server-windows7-64.exe` |
| Windows 7 / 8 (32-bit, very old) | `local-server-windows7-32.exe` |
| Mac with Apple chip (M1, M2, M3…) | `local-server-mac-apple` |
| Mac with Intel | `local-server-mac-intel` |
| Linux (64-bit), any distribution | `local-server-linux-64` |
| Linux (32-bit, very old) | `local-server-linux-32` |
| Raspberry Pi 3/4/5, Android (Termux) | `local-server-linux-arm64` |
| Older Raspberry Pi (32-bit) | `local-server-linux-arm32` |

**Windows:** double-click. The first time you may see "Windows protected your PC": click
**More info → Run anyway** (the program has no paid signature). On Windows 7, if you get
an "api-ms-win-crt" error, install update KB2999226 once.

**Mac:** in Terminal, inside the folder: `chmod +x local-server-mac-apple`, then
`./local-server-mac-apple`. If the Mac blocks it ("unidentified developer"), go to
**System Settings → Privacy & Security → Open Anyway**.

**Linux:** `chmod +x local-server-linux-64`, then `./local-server-linux-64`.

A window opens (the running server) and your browser opens the home page at
`http://127.0.0.1:8080`. To stop the server, close the window. Once downloaded, it
**works without internet**; the internet is only used to save new websites.

**Verify the download:** compare the SHA-256 fingerprint with the one published next to
the files (`certutil -hashfile file SHA256` on Windows, `shasum -a 256 file` on Mac/Linux).

> Tested on real hardware: Linux 64-bit. The other builds compile but have not yet been
> tested on real devices. Reports are welcome.

## Your own sites (folders)

Put each site in a folder inside `sites`, with an `index.html`.
Example: `sites/books/index.html` opens at `http://127.0.0.1:8080/books/`.
Videos, audio and PDFs are served too (with seeking support for video).

## Site archive

1. On the home page, paste an address (e.g. `https://example.com`) and click
   **save site**. It downloads gradually and the page refreshes by itself.
2. Each site gets a card with a thumbnail. **enlarge** opens it full screen with 100% to
   300% zoom (good for people with low vision). **open** opens the copy directly.
   **versions** lists older copies by date.
3. Every 24 hours (configurable) the server checks whether the site changed. If it did, it
   stores a new version; if not, it takes no extra space.

**Subscription articles and pages behind a login:** the server never uses your password.
Open the article, save it with **Ctrl+S** ("Web page, complete") or as **PDF**, and put the
file (plus its `_files` folder) in `arquivo/importar`. It shows up in the list within
seconds. The originals are moved to `arquivo/importar/importados`.

**How it stores things:** each file is stored once, even if it repeats across versions or
sites. Text is compressed losslessly (gzip). Photos and videos are already compressed and
stay as they are.

**Photos:** hidden photo data (camera model, date, GPS location, software, comments) is
always removed, without changing a single pixel. Only the color profile and rotation stay,
because they change how the photo looks.

**Economy mode (optional):** with `arquivo_economizar_imagens = sim` in `config.txt`, large
photos (JPEG and PNG over 60 KB) become WebP, usually 30% to 70% smaller, with an almost
invisible difference. The conversion uses the browser already on the computer (Edge,
Chrome or Chromium), with no internet at all. It only replaces a photo when the result is
at least 20% smaller; screenshots with text usually stay original so letters don't blur.
It is off by default because it slightly reduces sharpness, which matters for zoom users.

**Pages that need JavaScript:** the server uses the browser already on the computer (Edge,
Chrome, Chromium or Brave), invisible. It is not an extension and installs nothing.
- A new empty profile for each page, deleted at the end (none of your logins or history).
- All of that browser's traffic goes through a local filter that blocks the local network,
  the browser's own services (accounts, updates, telemetry) and common trackers.
- Still not working: logins, infinite scroll and interactive buttons (the copy is a
  "browsable snapshot").

To turn it off: `arquivo_navegador = nunca` in `config.txt`.

**Limits:**
- By default up to 200 pages and 300 MB per site (configurable).
- Respects `robots.txt` (when a site asks not to be copied) and downloads slowly so as not
  to overload anyone.
- Only the computer itself can add, update or remove. Over Wi-Fi, other people can only
  **view** what was saved.
- Phones (Android, iPhone, others) see everything over Wi-Fi but cannot save sites by
  themselves: phone browsers don't allow it.
- Windows 7 has no curl: it shows and imports, but does not download sites.

**Responsibility:** the copy stays only on your computer and nobody can delete it from
outside. But the law still applies to whoever keeps it (copyright, personal data). Keeping
copies for personal use is usually accepted; sharing someone else's site may count as
redistribution.

## Sharing over Wi-Fi

1. Open `config.txt` (created the first time the program runs).
2. Change to `compartilhar_na_rede = sim` (or `share_on_network = yes`), save, close and
   reopen the program.
3. The window shows the address to type on the phone, e.g. `http://192.168.0.15:8080/`.
   The phone must be on the same Wi-Fi.
4. If Windows asks about the firewall, allow it on **private networks**.

The router does not need internet, it only needs to be on.

## Settings (`config.txt`)

Option names are in Portuguese, and each one also accepts an English name:

| Portuguese | English | Default | Meaning |
|---|---|---|---|
| `porta` | `port` | `8080` | Port (1024–65535) |
| `compartilhar_na_rede` | `share_on_network` | `nao` | `sim`/`yes` = devices on the same Wi-Fi can open it |
| `arquivo_paginas_max` | `archive_max_pages` | `200` | Max pages per saved site |
| `arquivo_mb_max` | `archive_max_mb` | `300` | Max size per saved site (MB) |
| `arquivo_atualizar_horas` | `archive_update_hours` | `24` | Hours between checks for changes |
| `arquivo_navegador` | `archive_browser` | `auto` | Invisible browser: `auto`, `sempre`/`always`, `nunca`/`never` |
| `arquivo_economizar_imagens` | `archive_save_images` | `nao` | `sim`/`yes` = economy mode for photos (WebP) |

## Languages

Pages follow the browser's language and the window follows the system language, all
decided on the computer itself. The 🌐 menu at the top of the page switches language (the
choice stays only on that computer); the ◐ menu switches between automatic, light and dark.

Available: English, Português, Español, Français, Deutsch, Italiano, Nederlands, Polski,
Türkçe, Русский, Українська, العربية, हिन्दी, বাংলা, 中文, 日本語, 한국어, Bahasa Indonesia,
Tiếng Việt, Kiswahili.

Texts live in `src/idiomas/`, one plain-text file per language (`key = value`).
Corrections from native speakers are very welcome; no programming needed.

## Security

- With `compartilhar_na_rede = nao` (default), only this computer has access.
- Serves only files from `sites` and the archive. Hidden files and folders, shortcuts
  (symlinks) pointing outside the folder and path tricks (`../`) are blocked.
- Writes only inside the `arquivo` folder (and `config.txt`).
- The buttons (save, update, remove) only work on the computer itself, from the server's
  own page, with a one-time password that changes every time the program starts. Other
  sites open in the browser cannot use them (CSRF protection).
- Protection against DNS rebinding: only requests addressed by IP or `localhost` are accepted.
- When downloading, the server **never** accesses local network addresses (router, other
  devices, the computer itself), neither through curl nor through the invisible browser, so
  nobody can use it to spy on devices at home (SSRF protection).
- Copies open in a sandbox: no scripts and nothing fetched from the internet (no trackers).
- Use sharing only on trusted networks: anyone on the same Wi-Fi can view what is saved.

Found a security problem? Please open an *issue* in this repository.

## In case of a blackout

- Files are not lost when the power goes out: they are written to disk.
- The risk is damage to the device when power comes back: use a surge protector or a UPS.
- To keep it running during a blackout: a laptop with a battery, a UPS, a battery with an
  inverter, or solar power. The router also needs power for Wi-Fi.
- Copy the whole folder to other computers, USB sticks and drives. Each copy is a complete
  server, archive included.

## Building

You need [Rust](https://rustup.rs). The standard library is the only dependency.

```
cargo test
cargo build --release
```

The program is at `target/release/local-server`.

A note on the code: comments are in English, but function and variable names are in
Portuguese, because the project started in Brazil (`guardar` = save, `baixar` = download,
`arquivo` = archive, `pagina` = page, `textos` = texts, `imagens` = images,
`invisivel` = invisible browser).

## License

Free software under the **GNU General Public License v3.0 or later** (see `LICENSE`).
You may use, copy, study, modify and distribute it for free, including in companies. If
you distribute a modified version, its source code must also be released under the same
license.
