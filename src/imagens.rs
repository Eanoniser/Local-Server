// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Imagens do arquivo de sites.
//!
//! 1. Limpeza sem perda (sempre ligada): tira das fotos os dados escondidos
//!    (câmera, data, localização GPS, programa usado, comentários). Nenhum pixel
//!    muda. Mantém o que afeta a aparência: perfil de cor e a rotação da foto.
//!    Funciona com JPEG, PNG e WebP. Se o arquivo tiver algo estranho, fica como está.
//!
//! 2. Modo econômico (opcional, desligado por padrão): fotos grandes viram WebP,
//!    usando o navegador que já está no computador, sem internet nenhuma.
//!    Só troca quando o resultado fica bem menor.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::gzip::crc32;

fn tipo_base(tipo: &str) -> String {
    tipo.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Limpa os dados escondidos, sem mudar a imagem. Devolve o original se não souber limpar.
pub fn limpar(dados: &[u8], tipo: &str) -> Vec<u8> {
    let r = match tipo_base(tipo).as_str() {
        "image/jpeg" | "image/jpg" | "image/pjpeg" => limpar_jpeg(dados),
        "image/png" | "image/apng" => limpar_png(dados),
        "image/webp" => limpar_webp(dados),
        _ => None,
    };
    match r {
        Some(v) if v.len() <= dados.len() => v,
        _ => dados.to_vec(),
    }
}

// ---------------- rotação (o único dado do EXIF que muda a aparência) ----------------

/// Lê a orientação (1 a 8) de um bloco TIFF/EXIF (começando em "II" ou "MM").
fn orientacao_tiff(t: &[u8]) -> Option<u16> {
    if t.len() < 8 {
        return None;
    }
    let le = match &t[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_ = |i: usize| -> Option<u16> {
        let b = t.get(i..i + 2)?;
        Some(if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_ = |i: usize| -> Option<u32> {
        let b = t.get(i..i + 4)?;
        Some(if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };
    let ifd = u32_(4)? as usize;
    let n = u16_(ifd)? as usize;
    for k in 0..n.min(500) {
        let e = ifd + 2 + k * 12;
        if u16_(e)? == 0x0112 {
            let v = u16_(e + 8)?;
            return (1..=8).contains(&v).then_some(v);
        }
    }
    None
}

/// Bloco TIFF mínimo, só com a orientação.
fn tiff_so_orientacao(o: u16) -> Vec<u8> {
    let mut t = b"MM\0\x2a\0\0\0\x08".to_vec();
    t.extend([0, 1]); // uma entrada
    t.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1]); // Orientation, SHORT, 1 valor
    t.extend(o.to_be_bytes());
    t.extend([0, 0]);
    t.extend([0, 0, 0, 0]); // sem próximo IFD
    t
}

// ---------------- JPEG ----------------

fn limpar_jpeg(d: &[u8]) -> Option<Vec<u8>> {
    if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
        return None;
    }
    let mut saida = vec![0xFF, 0xD8];
    let mut orientacao = None;
    let mut onde_exif = 2; // onde pôr a rotação: logo depois do APP0, se houver
    let mut i = 2;
    loop {
        // pula bytes de preenchimento 0xFF
        if *d.get(i)? != 0xFF {
            return None;
        }
        while *d.get(i + 1)? == 0xFF {
            i += 1;
        }
        let m = d[i + 1];
        if m == 0xD9 {
            saida.extend(&d[i..]);
            break;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            saida.extend(&d[i..i + 2]);
            i += 2;
            continue;
        }
        let tam = u16::from_be_bytes([*d.get(i + 2)?, *d.get(i + 3)?]) as usize;
        if tam < 2 {
            return None;
        }
        let fim = i + 2 + tam;
        let corpo = d.get(i + 4..fim)?;
        let manter = match m {
            0xE0 => true, // JFIF
            0xE1 => {
                if corpo.starts_with(b"Exif\0\0") && orientacao.is_none() {
                    orientacao = orientacao_tiff(&corpo[6..]);
                }
                false // EXIF (câmera, GPS, data) e XMP
            }
            0xE2 => corpo.starts_with(b"ICC_PROFILE\0"), // perfil de cor: muda as cores, fica
            0xEE => corpo.starts_with(b"Adobe"),         // necessário para decodificar certas cores
            0xE3..=0xED | 0xEF => false,                  // outros dados de programas (IPTC, etc.)
            0xFE => false,                                // comentário
            _ => true,                                    // tabelas e cabeçalho da imagem
        };
        if manter {
            saida.extend(&d[i..fim]);
            if m == 0xE0 && onde_exif == 2 {
                onde_exif = saida.len();
            }
        }
        i = fim;
        if m == 0xDA {
            // começo da imagem em si: o resto vai inteiro, sem mexer
            saida.extend(d.get(i..)?);
            break;
        }
    }
    if let Some(o) = orientacao.filter(|&o| o != 1) {
        let mut seg = b"Exif\0\0".to_vec();
        seg.extend(tiff_so_orientacao(o));
        let mut bloco = vec![0xFF, 0xE1];
        bloco.extend(((seg.len() + 2) as u16).to_be_bytes());
        bloco.extend(seg);
        saida.splice(onde_exif..onde_exif, bloco);
    }
    Some(saida)
}

// ---------------- PNG ----------------

fn limpar_png(d: &[u8]) -> Option<Vec<u8>> {
    const ASSINATURA: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !d.starts_with(ASSINATURA) {
        return None;
    }
    let mut saida = ASSINATURA.to_vec();
    let mut i = 8;
    let mut viu_fim = false;
    while i < d.len() {
        let tam = u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?) as usize;
        let tipo: [u8; 4] = d.get(i + 4..i + 8)?.try_into().ok()?;
        let fim = i.checked_add(12)?.checked_add(tam)?;
        let bloco = d.get(i..fim)?;
        match &tipo {
            b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" => {} // textos, data e programa: fora
            b"eXIf" => {
                // pode ter GPS; fica só a rotação, se houver
                if let Some(o) = orientacao_tiff(&bloco[8..8 + tam]).filter(|&o| o != 1) {
                    let t = tiff_so_orientacao(o);
                    saida.extend((t.len() as u32).to_be_bytes());
                    let mut tc = b"eXIf".to_vec();
                    tc.extend(&t);
                    saida.extend(&tc);
                    saida.extend(crc32(&tc).to_be_bytes());
                }
            }
            _ => saida.extend(bloco),
        }
        i = fim;
        if &tipo == b"IEND" {
            viu_fim = true;
            break;
        }
    }
    viu_fim.then_some(saida)
}

// ---------------- WebP ----------------

fn limpar_webp(d: &[u8]) -> Option<Vec<u8>> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WEBP" {
        return None;
    }
    let total = (u32::from_le_bytes(d[4..8].try_into().ok()?) as usize).checked_add(8)?;
    let d = d.get(..total)?;
    let mut blocos: Vec<Vec<u8>> = Vec::new();
    let mut tirou_exif = false;
    let mut tirou_xmp = false;
    let mut orientacao = None;
    let mut i = 12;
    while i + 8 <= d.len() {
        let tipo: [u8; 4] = d[i..i + 4].try_into().ok()?;
        let tam = u32::from_le_bytes(d[i + 4..i + 8].try_into().ok()?) as usize;
        let fim = i.checked_add(8)?.checked_add(tam)?.checked_add(tam & 1)?;
        let bloco = d.get(i..fim.min(d.len()))?;
        match &tipo {
            b"EXIF" => {
                let mut c = &bloco[8..8 + tam.min(bloco.len() - 8)];
                if c.starts_with(b"Exif\0\0") {
                    c = &c[6..];
                }
                orientacao = orientacao_tiff(c).filter(|&o| o != 1);
                tirou_exif = true;
            }
            b"XMP " => tirou_xmp = true,
            _ => blocos.push(bloco.to_vec()),
        }
        i = fim;
    }
    if !tirou_exif && !tirou_xmp {
        return None; // nada para tirar
    }
    if let Some(o) = orientacao {
        let t = tiff_so_orientacao(o);
        let mut b = b"EXIF".to_vec();
        b.extend((t.len() as u32).to_le_bytes());
        b.extend(&t);
        if t.len() % 2 == 1 {
            b.push(0);
        }
        blocos.push(b);
    }
    // avisa no cabeçalho VP8X o que ainda existe
    if let Some(vp8x) = blocos.iter_mut().find(|b| b.starts_with(b"VP8X") && b.len() >= 9) {
        if orientacao.is_none() {
            vp8x[8] &= !0x08;
        }
        vp8x[8] &= !0x04;
    }
    let corpo: Vec<u8> = blocos.concat();
    let mut saida = b"RIFF".to_vec();
    saida.extend(((corpo.len() + 4) as u32).to_le_bytes());
    saida.extend(b"WEBP");
    saida.extend(corpo);
    Some(saida)
}

// ---------------- modo econômico: WebP pelo navegador do computador ----------------

/// Vale a pena tentar converter esta imagem?
pub fn candidata_webp(dados: &[u8], tipo: &str) -> bool {
    let t = tipo_base(tipo);
    let formato_ok = match t.as_str() {
        "image/jpeg" | "image/jpg" | "image/pjpeg" => dados.starts_with(&[0xFF, 0xD8]),
        // PNG animado (APNG) fica como está
        "image/png" => dados.starts_with(b"\x89PNG") && !dados.windows(4).take(4096).any(|w| w == b"acTL"),
        _ => false,
    };
    formato_ok && dados.len() >= 60 * 1024 && dados.len() <= 15 * 1024 * 1024
}

const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len() / 3 * 4 + 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(ALFABETO[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn de_base64(s: &str) -> Option<Vec<u8>> {
    let mut v = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut n = 0;
    for b in s.bytes() {
        let x = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = acc << 6 | x;
        n += 1;
        if n == 4 {
            v.extend([(acc >> 16) as u8, (acc >> 8) as u8, acc as u8]);
            acc = 0;
            n = 0;
        }
    }
    match n {
        2 => v.push((acc >> 4) as u8),
        3 => v.extend([(acc >> 10) as u8, (acc >> 2) as u8]),
        _ => {}
    }
    Some(v)
}

/// Qualidade do WebP (0 a 1). 0,88: diferença quase invisível, mesmo ampliando.
const QUALIDADE: &str = "0.88";
const QUALIDADE_PNG: &str = "0.95";

/// Endereço file:// com acentos e espaços codificados (pastas como "C:\\Users\\José Silva").
fn url_de_arquivo(p: &Path) -> String {
    let caminho = p.to_string_lossy().replace('\\', "/");
    let mut u = String::from(if caminho.starts_with('/') { "file://" } else { "file:///" });
    for b in caminho.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/:".contains(&b) {
            u.push(b as char);
        } else {
            u.push_str(&format!("%{b:02X}"));
        }
    }
    u
}

/// Converte um lote de imagens para WebP no navegador do computador, sem rede.
/// Para cada imagem devolve o WebP, ou None se não valeu a pena (ou falhou).
pub fn para_webp(navegador: &Path, tmp: &Path, imagens: &[(&[u8], &str)]) -> Vec<Option<Vec<u8>>> {
    let mut res = vec![None; imagens.len()];
    if imagens.is_empty() {
        return res;
    }
    static LOTE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = LOTE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let pasta = tmp.join(format!("webp-{}-{n}", std::process::id()));
    if std::fs::create_dir_all(&pasta).is_err() {
        return res;
    }
    // página que só desenha as imagens (embutidas) e devolve o WebP; nada de rede
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; script-src 'unsafe-inline'\"><body>",
    );
    for (i, (dados, tipo)) in imagens.iter().enumerate() {
        // PNG costuma ser print de tela ou desenho com letras: mais qualidade para não borrar
        let q = if tipo_base(tipo) == "image/png" { QUALIDADE_PNG } else { QUALIDADE };
        html.push_str(&format!("<img id=i{i} data-q={q} src=\"data:{};base64,{}\">", tipo_base(tipo), base64(dados)));
    }
    html.push_str(&format!(
        "<pre id=r></pre><script>onload=function(){{var s='';for(var i=0;i<{};i++){{var m=document.getElementById('i'+i),o='';\
try{{var c=document.createElement('canvas');c.width=m.naturalWidth;c.height=m.naturalHeight;\
if(c.width>0&&c.height>0){{c.getContext('2d').drawImage(m,0,0);var u=c.toDataURL('image/webp',+m.getAttribute('data-q'));\
if(u.indexOf('data:image/webp;base64,')===0)o=u.slice(23);}}}}catch(e){{}}s+='#'+i+':'+o+'\\n';}}\
document.getElementById('r').textContent=s;}};</script>",
        imagens.len()
    ));
    let pagina = pasta.join("converter.html");
    let perfil = pasta.join("perfil");
    if std::fs::write(&pagina, html).is_err() {
        let _ = std::fs::remove_dir_all(&pasta);
        return res;
    }
    let mut cmd = Command::new(navegador);
    cmd.arg("--headless")
        .args([
            "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
            "--disable-component-update", "--disable-sync", "--disable-extensions", "--disable-default-apps",
            "--disable-breakpad", "--no-pings", "--mute-audio", "--virtual-time-budget=20000",
            // sem rede nenhuma: tudo vai para um endereço que não responde
            "--proxy-server=http://127.0.0.1:9", "--proxy-bypass-list=<-loopback>",
        ])
        .arg(format!("--user-data-dir={}", perfil.display()));
    if std::env::var("SERVIDOR_LOCAL_NAVEGADOR_SEM_SANDBOX").is_ok_and(|v| v == "1") {
        cmd.arg("--no-sandbox");
    }
    cmd.arg("--dump-dom").arg(url_de_arquivo(&pagina)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let saida = match cmd.spawn() {
        Ok(mut filho) => {
            let mut out = filho.stdout.take().unwrap();
            let leitor = std::thread::spawn(move || {
                let mut v = Vec::new();
                let _ = (&mut out).take(200 * 1024 * 1024).read_to_end(&mut v);
                v
            });
            let inicio = Instant::now();
            loop {
                match filho.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if inicio.elapsed() > Duration::from_secs(120) => {
                        let _ = filho.kill();
                        let _ = filho.wait();
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                    Err(_) => break,
                }
            }
            leitor.join().unwrap_or_default()
        }
        Err(_) => Vec::new(),
    };
    for _ in 0..10 {
        if std::fs::remove_dir_all(&pasta).is_ok() || !pasta.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let texto = String::from_utf8_lossy(&saida);
    let Some(ini) = texto.find("<pre id=\"r\">") else { return res };
    let corpo = &texto[ini + 12..];
    let corpo = &corpo[..corpo.find("</pre>").unwrap_or(corpo.len())];
    for linha in corpo.lines() {
        let Some(resto) = linha.strip_prefix('#') else { continue };
        let Some((i, b64)) = resto.split_once(':') else { continue };
        let Ok(i) = i.parse::<usize>() else { continue };
        if i >= imagens.len() || b64.is_empty() {
            continue;
        }
        if let Some(webp) = de_base64(b64) {
            // só troca se ficou bem menor (pelo menos 20%) e é um WebP de verdade
            if webp.len() >= 12 && &webp[..4] == b"RIFF" && &webp[8..12] == b"WEBP" && webp.len() * 10 <= imagens[i].0.len() * 8 {
                res[i] = Some(webp);
            }
        }
    }
    res
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exif_com(orientacao: u16, com_gps: bool) -> Vec<u8> {
        // TIFF little-endian: IFD0 com Make, Orientation e (talvez) um ponteiro de GPS
        let mut t = b"II\x2a\0\x08\0\0\0".to_vec();
        let n: u16 = if com_gps { 3 } else { 2 };
        t.extend(n.to_le_bytes());
        t.extend([0x0F, 0x01, 2, 0, 4, 0, 0, 0]); // Make, ASCII, 4
        t.extend(*b"Cam\0");
        t.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0]);
        t.extend(orientacao.to_le_bytes());
        t.extend([0, 0]);
        if com_gps {
            t.extend([0x25, 0x88, 4, 0, 1, 0, 0, 0, 0x40, 0, 0, 0]);
        }
        t.extend([0, 0, 0, 0]);
        t.extend(b"GPS -23.5505 -46.6333 (secreto)");
        t
    }

    fn jpeg_falso(exif: &[u8]) -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8];
        j.extend([0xFF, 0xE0, 0, 16]);
        j.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(exif);
        j.extend([0xFF, 0xE1]);
        j.extend(((app1.len() + 2) as u16).to_be_bytes());
        j.extend(app1);
        j.extend([0xFF, 0xE2, 0, 18]);
        j.extend(b"ICC_PROFILE\0\x01\x01ab");
        j.extend([0xFF, 0xFE, 0, 9]);
        j.extend(b"segredo");
        j.extend([0xFF, 0xDB, 0, 4, 1, 2]); // tabela (fica)
        j.extend([0xFF, 0xDA, 0, 4, 9, 9]); // começo da imagem
        j.extend([1, 2, 3, 0xFF, 0x00, 4, 5]);
        j.extend([0xFF, 0xD9]);
        j
    }

    #[test]
    fn jpeg_perde_gps_mantem_cor_e_rotacao() {
        let j = jpeg_falso(&exif_com(6, true));
        let l = limpar(&j, "image/jpeg");
        let achar = |h: &[u8], a: &[u8]| h.windows(a.len()).any(|w| w == a);
        assert!(!achar(&l, b"GPS"), "GPS saiu");
        assert!(!achar(&l, b"Cam"), "câmera saiu");
        assert!(!achar(&l, b"segredo"), "comentário saiu");
        assert!(achar(&l, b"ICC_PROFILE"), "perfil de cor ficou");
        assert!(achar(&l, b"JFIF"));
        assert!(l.ends_with(&[0xFF, 0xDA, 0, 4, 9, 9, 1, 2, 3, 0xFF, 0x00, 4, 5, 0xFF, 0xD9]), "imagem intacta");
        // rotação continua valendo
        let p = l.windows(6).position(|w| w == b"Exif\0\0").unwrap();
        assert_eq!(orientacao_tiff(&l[p + 6..]), Some(6));
        // sem rotação: nenhum EXIF
        let l1 = limpar(&jpeg_falso(&exif_com(1, true)), "image/jpeg");
        assert!(!achar(&l1, b"Exif"));
        assert!(l1.len() < j.len());
    }

    #[test]
    fn arquivos_estranhos_ficam_como_estao() {
        for lixo in [&b""[..], b"\xFF\xD8", b"\xFF\xD8\xFF\xE1\xFF\xFF", b"\x89PNG\r\n\x1a\n\0\0\0\xFFIHDR", b"RIFF\x10\0\0\0WEBPVP8X", b"abc"] {
            assert_eq!(limpar(lixo, "image/jpeg"), lixo);
            assert_eq!(limpar(lixo, "image/png"), lixo);
            assert_eq!(limpar(lixo, "image/webp"), lixo);
        }
        assert_eq!(limpar(b"<svg/>", "image/svg+xml"), b"<svg/>");
    }

    fn bloco_png(tipo: &[u8; 4], d: &[u8]) -> Vec<u8> {
        let mut b = (d.len() as u32).to_be_bytes().to_vec();
        let mut tc = tipo.to_vec();
        tc.extend(d);
        b.extend(&tc);
        b.extend(crc32(&tc).to_be_bytes());
        b
    }

    #[test]
    fn png_perde_textos() {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend(bloco_png(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]));
        p.extend(bloco_png(b"tEXt", b"Author\0Fulano"));
        p.extend(bloco_png(b"eXIf", &exif_com(3, true)));
        p.extend(bloco_png(b"iCCP", b"perfil"));
        p.extend(bloco_png(b"IDAT", b"pixels"));
        p.extend(bloco_png(b"IEND", b""));
        let l = limpar(&p, "image/png");
        let achar = |h: &[u8], a: &[u8]| h.windows(a.len()).any(|w| w == a);
        assert!(!achar(&l, b"Fulano") && !achar(&l, b"GPS"));
        assert!(achar(&l, b"iCCP") && achar(&l, b"pixels") && achar(&l, b"IEND"));
        let pe = l.windows(4).position(|w| w == b"eXIf").unwrap();
        assert_eq!(orientacao_tiff(&l[pe + 4..]), Some(3));
    }

    #[test]
    fn webp_perde_exif_e_xmp() {
        let mut corpo = Vec::new();
        corpo.extend(b"VP8X");
        corpo.extend(10u32.to_le_bytes());
        corpo.extend([0x2C, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // ICC + EXIF + XMP
        corpo.extend(b"ICCP");
        corpo.extend(2u32.to_le_bytes());
        corpo.extend(b"cc");
        corpo.extend(b"VP8L");
        corpo.extend(3u32.to_le_bytes());
        corpo.extend(b"img\0");
        let ex = exif_com(1, true);
        corpo.extend(b"EXIF");
        corpo.extend((ex.len() as u32).to_le_bytes());
        corpo.extend(&ex);
        if ex.len() % 2 == 1 {
            corpo.push(0);
        }
        corpo.extend(b"XMP ");
        corpo.extend(4u32.to_le_bytes());
        corpo.extend(b"<x/>");
        let mut w = b"RIFF".to_vec();
        w.extend(((corpo.len() + 4) as u32).to_le_bytes());
        w.extend(b"WEBP");
        w.extend(corpo);
        let l = limpar(&w, "image/webp");
        let achar = |h: &[u8], a: &[u8]| h.windows(a.len()).any(|w| w == a);
        assert!(!achar(&l, b"GPS") && !achar(&l, b"<x/>"));
        assert!(achar(&l, b"ICCP") && achar(&l, b"img"));
        assert_eq!(l[20], 0x20, "só a marca de ICC fica no VP8X");
        assert_eq!(u32::from_le_bytes(l[4..8].try_into().unwrap()) as usize + 8, l.len());
    }

    #[test]
    fn endereco_de_arquivo() {
        assert_eq!(url_de_arquivo(Path::new("/tmp/a b/ção.html")), "file:///tmp/a%20b/%C3%A7%C3%A3o.html");
        assert_eq!(url_de_arquivo(Path::new("C:\\Users\\José\\x.html")), "file:///C:/Users/Jos%C3%A9/x.html");
    }

    #[test]
    fn base64_ida_e_volta() {
        for n in 0..50usize {
            let d: Vec<u8> = (0..n).map(|i| (i * 37 % 256) as u8).collect();
            assert_eq!(de_base64(&base64(&d)).unwrap(), d);
        }
        assert_eq!(base64(b"Man"), "TWFu");
    }

    /// Com imagens reais: SERVIDOR_LOCAL_IMAGENS_TESTE=pasta (cada arquivo é limpo e
    /// gravado como <nome>.limpo para conferir fora que os pixels são idênticos).
    #[test]
    fn imagens_reais() {
        let Ok(pasta) = std::env::var("SERVIDOR_LOCAL_IMAGENS_TESTE") else { return };
        for e in std::fs::read_dir(&pasta).unwrap().flatten() {
            let p = e.path();
            let nome = p.to_string_lossy().to_string();
            if nome.ends_with(".limpo") || nome.ends_with(".webp-conv") {
                continue;
            }
            let tipo = crate::pagina::tipo_do_arquivo(&p);
            let d = std::fs::read(&p).unwrap();
            std::fs::write(format!("{nome}.limpo"), limpar(&d, tipo)).unwrap();
        }
        if let Ok(nav) = std::env::var("SERVIDOR_LOCAL_NAVEGADOR") {
            let mut lista = Vec::new();
            for e in std::fs::read_dir(&pasta).unwrap().flatten() {
                let p = e.path();
                let tipo = crate::pagina::tipo_do_arquivo(&p);
                let d = std::fs::read(&p).unwrap();
                if candidata_webp(&d, tipo) {
                    lista.push((p, d, tipo));
                }
            }
            let entrada: Vec<(&[u8], &str)> = lista.iter().map(|(_, d, t)| (d.as_slice(), *t)).collect();
            let r = para_webp(Path::new(&nav), Path::new(&pasta), &entrada);
            for ((p, _, _), w) in lista.iter().zip(r) {
                if let Some(w) = w {
                    std::fs::write(format!("{}.webp-conv", p.display()), w).unwrap();
                }
            }
        }
    }
}
