// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal, defensive HTTP: GET/HEAD to read; POST only for the archive
//! buttons, and only from the computer itself (with a one-time password).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::pagina;
use crate::Servidor;

const MAX_CABECALHO: usize = 16 * 1024;
const MAX_SEGMENTOS: usize = 32;

const CABECALHOS_COMUNS: &str = "X-Content-Type-Options: nosniff\r\n\
Referrer-Policy: no-referrer\r\n\
X-Frame-Options: SAMEORIGIN\r\n\
Cross-Origin-Opener-Policy: same-origin\r\n\
Cache-Control: no-cache\r\n";

pub struct Pedido {
    pub metodo: String,
    pub alvo: String,
    pub host: Option<String>,
    pub range: Option<String>,
    pub origem: Option<String>,
    pub sec_fetch_site: Option<String>,
    pub tamanho: Option<usize>,
    pub idioma: Option<String>,
    pub cookie: Option<String>,
}

const MAX_CORPO: usize = 8 * 1024;

pub fn atender(mut c: TcpStream, s: &Servidor) -> io::Result<()> {
    c.set_read_timeout(Some(Duration::from_secs(15)))?;
    c.set_write_timeout(Some(Duration::from_secs(60)))?;
    let (cabecalho, sobra) = match ler_cabecalho(&mut c)? {
        Some(t) => t,
        None => return Ok(()),
    };
    let do_proprio_pc = c.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
    let Some(p) = analisar(&cabecalho) else {
        return simples(&mut c, 400, "pedido inválido");
    };
    // page language: what the browser already sends (or the choice made in the language menu)
    crate::textos::definir(crate::textos::do_pedido(p.idioma.as_deref(), p.cookie.as_deref()));
    // light/dark theme chosen with the button (or the system's)
    crate::pagina::definir_tema(p.cookie.as_deref());
    if !host_permitido(p.host.as_deref(), s.config.porta, s.config.compartilhar) {
        return simples(&mut c, 403, "proibido");
    }
    if p.metodo == "POST" {
        return acao(&mut c, s, &p, sobra, do_proprio_pc);
    }
    let so_cabecalho = match p.metodo.as_str() {
        "GET" => false,
        "HEAD" => true,
        _ => return simples(&mut c, 405, "método não permitido"),
    };
    let (caminho, consulta) = match p.alvo.split('#').next().unwrap_or("").split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (p.alvo.split('#').next().unwrap_or(""), None),
    };
    if !caminho.starts_with('/') {
        return simples(&mut c, 400, "pedido inválido");
    }
    let html = |c: &mut TcpStream, codigo: u16, corpo: String| {
        enviar(c, codigo, "text/html; charset=utf-8", Some(pagina::CSP_PAGINAS), corpo.as_bytes(), so_cabecalho)
    };
    if caminho == "/" {
        return html(&mut c, 200, pagina::inicial(s, do_proprio_pc, consulta));
    }
    let partes: Vec<&str> = caminho[1..].split('/').collect();
    match partes.as_slice() {
        ["idioma", l] if crate::textos::LINGUAS.iter().any(|x| x.0 == *l) => {
            // stores the choice in a cookie for this server only (it never leaves the computer)
            let cab = format!(
                "HTTP/1.1 303 See Other\r\nLocation: /\r\nSet-Cookie: lingua={l}; Path=/; Max-Age=31536000; SameSite=Strict; HttpOnly\r\nContent-Length: 0\r\nConnection: close\r\n{CABECALHOS_COMUNS}\r\n"
            );
            return c.write_all(cab.as_bytes());
        }
        ["tema", tema @ ("auto" | "claro" | "escuro")] => {
            let cab = format!(
                "HTTP/1.1 303 See Other\r\nLocation: /\r\nSet-Cookie: tema={tema}; Path=/; Max-Age=31536000; SameSite=Strict; HttpOnly\r\nContent-Length: 0\r\nConnection: close\r\n{CABECALHOS_COMUNS}\r\n"
            );
            return c.write_all(cab.as_bytes());
        }
        ["arquivo", id, ts, resto @ ..] if !resto.is_empty() => {
            let chave = resto.join("/");
            let chave = match consulta {
                Some(q) => format!("{chave}?{q}"),
                None => chave,
            };
            return servir_guardado(&mut c, s, id, ts, &chave, so_cabecalho);
        }
        ["ver", id] | ["ver", id, ""] => return html(&mut c, 200, pagina::ver(s, id, None, consulta)),
        ["ver", id, ts] => return html(&mut c, 200, pagina::ver(s, id, Some(ts), consulta)),
        ["versoes", id] => return html(&mut c, 200, pagina::versoes(s, id)),
        ["remover", id] if do_proprio_pc => return html(&mut c, 200, pagina::confirmar_remocao(s, id)),
        _ => {}
    }
    let Some(texto) = decodificar(caminho) else {
        return simples(&mut c, 400, "endereço inválido");
    };
    let Some(segmentos) = segmentos_seguros(&texto) else {
        return simples(&mut c, 404, "não encontrado");
    };
    let mut alvo = s.raiz.clone();
    for seg in &segmentos {
        alvo.push(seg);
    }
    if alvo.is_dir() {
        if !texto.ends_with('/') {
            return redirecionar(&mut c, &format!("{caminho}/"));
        }
        alvo.push("index.html");
    }
    let Some(arquivo) = dentro_da_raiz(&alvo, &s.raiz) else {
        return simples(&mut c, 404, "não encontrado");
    };
    servir_arquivo(&mut c, &arquivo, p.range.as_deref(), so_cabecalho)
}

fn ler_cabecalho(c: &mut TcpStream) -> io::Result<Option<(String, Vec<u8>)>> {
    let mut buf = Vec::with_capacity(2048);
    let mut pedaco = [0u8; 2048];
    while !buf.windows(4).any(|j| j == b"\r\n\r\n") {
        let n = c.read(&mut pedaco)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&pedaco[..n]);
        if buf.len() > MAX_CABECALHO {
            simples(c, 431, "pedido grande demais")?;
            return Ok(None);
        }
    }
    let fim = buf.windows(4).position(|j| j == b"\r\n\r\n").unwrap() + 4;
    let sobra = buf.split_off(fim);
    Ok(Some((String::from_utf8_lossy(&buf).into_owned(), sobra)))
}

pub fn analisar(texto: &str) -> Option<Pedido> {
    let mut linhas = texto.split("\r\n");
    let mut partes = linhas.next()?.split(' ');
    let (metodo, alvo, versao) = (partes.next()?, partes.next()?, partes.next()?);
    if partes.next().is_some() || !versao.starts_with("HTTP/1.") {
        return None;
    }
    let mut p = Pedido {
        metodo: metodo.to_string(),
        alvo: alvo.to_string(),
        host: None,
        range: None,
        origem: None,
        sec_fetch_site: None,
        tamanho: None,
        idioma: None,
        cookie: None,
    };
    for l in linhas.take_while(|l| !l.is_empty()) {
        let Some((k, v)) = l.split_once(':') else { continue };
        if k.eq_ignore_ascii_case("host") {
            p.host = Some(v.trim().to_string());
        } else if k.eq_ignore_ascii_case("range") {
            p.range = Some(v.trim().to_string());
        } else if k.eq_ignore_ascii_case("origin") {
            p.origem = Some(v.trim().to_string());
        } else if k.eq_ignore_ascii_case("sec-fetch-site") {
            p.sec_fetch_site = Some(v.trim().to_ascii_lowercase());
        } else if k.eq_ignore_ascii_case("accept-language") {
            p.idioma = Some(v.trim().chars().take(200).collect());
        } else if k.eq_ignore_ascii_case("cookie") {
            p.cookie = Some(v.trim().chars().take(500).collect());
        } else if k.eq_ignore_ascii_case("content-length") {
            p.tamanho = v.trim().parse().ok();
        }
    }
    Some(p)
}

/// Protection against DNS rebinding: only accepts requests addressed by IP or "localhost".
pub fn host_permitido(host: Option<&str>, porta: u16, rede: bool) -> bool {
    let Some(host) = host else { return false };
    let (nome, p) = match host.rsplit_once(':') {
        Some((n, p)) if !n.ends_with(']') || n.starts_with('[') => (n, p.parse::<u16>().ok()),
        _ => (host, None),
    };
    if p.is_some_and(|p| p != porta) {
        return false;
    }
    let nome = nome.trim_start_matches('[').trim_end_matches(']');
    if nome.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match nome.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() => true,
        Ok(_) => rede,
        Err(_) => false,
    }
}

/// Decodes %XX. Refuses null bytes and text that is not UTF-8.
pub fn decodificar(caminho: &str) -> Option<String> {
    let b = caminho.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            saida.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            saida.push(b[i]);
            i += 1;
        }
    }
    if saida.contains(&0) {
        return None;
    }
    String::from_utf8(saida).ok()
}

/// Splits the path into parts and refuses any trick to escape the folder:
/// "..", ".", hidden files, backslashes, ":" (Windows) and reserved names.
pub fn segmentos_seguros(caminho: &str) -> Option<Vec<String>> {
    // Only one leading slash: "//site" could become a redirect to another domain.
    let partes: Vec<&str> = caminho.strip_prefix('/')?.split('/').collect();
    let ultimo = partes.len().saturating_sub(1);
    let mut ok = Vec::new();
    for (i, p) in partes.iter().enumerate() {
        if p.is_empty() {
            if i == ultimo {
                continue; // trailing slash
            }
            return None;
        }
        let base = p.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reservado = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((base.starts_with("COM") || base.starts_with("LPT")) && base.len() == 4);
        if p.starts_with('.') || p.contains(['\\', ':', '*', '?', '"', '<', '>', '|']) || p.ends_with([' ', '.']) || reservado {
            return None;
        }
        ok.push(p.to_string());
    }
    if ok.is_empty() || ok.len() > MAX_SEGMENTOS {
        return None;
    }
    Some(ok)
}

/// Checks, after resolving shortcuts (symlinks), that the file really is inside sites/.
fn dentro_da_raiz(alvo: &Path, raiz: &Path) -> Option<PathBuf> {
    let real = alvo.canonicalize().ok()?;
    (real.starts_with(raiz) && real.is_file()).then_some(real)
}

/// Reads "bytes=a-b". Returns (start, end) inclusive, or Err if outside the file.
pub fn faixa(range: &str, tamanho: u64) -> Option<Result<(u64, u64), ()>> {
    let r = range.strip_prefix("bytes=")?;
    if r.contains(',') || tamanho == 0 {
        return None; // several ranges: returns the whole file
    }
    let (a, b) = r.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    let resultado = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return Some(Err(()));
        }
        (tamanho.saturating_sub(n), tamanho - 1)
    } else {
        let ini: u64 = a.parse().ok()?;
        let fim: u64 = if b.is_empty() { tamanho - 1 } else { b.parse::<u64>().ok()?.min(tamanho - 1) };
        if ini > fim || ini >= tamanho {
            return Some(Err(()));
        }
        (ini, fim)
    };
    Some(Ok(resultado))
}

fn servir_arquivo(c: &mut TcpStream, caminho: &Path, range: Option<&str>, so_cabecalho: bool) -> io::Result<()> {
    let Ok(mut f) = File::open(caminho) else {
        return simples(c, 404, "não encontrado");
    };
    let tamanho = f.metadata()?.len();
    let tipo = pagina::tipo_do_arquivo(caminho);
    let (codigo, ini, fim) = match range.and_then(|r| faixa(r, tamanho)) {
        Some(Ok((a, b))) => (206, a, b),
        Some(Err(())) => {
            let cab = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{tamanho}\r\nContent-Length: 0\r\nConnection: close\r\n{CABECALHOS_COMUNS}\r\n"
            );
            return c.write_all(cab.as_bytes());
        }
        None => (200, 0, tamanho.saturating_sub(1)),
    };
    let comprimento = if tamanho == 0 { 0 } else { fim - ini + 1 };
    let mut cab = format!(
        "HTTP/1.1 {codigo} {}\r\nContent-Type: {tipo}\r\nContent-Length: {comprimento}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n{CABECALHOS_COMUNS}",
        if codigo == 206 { "Partial Content" } else { "OK" }
    );
    if codigo == 206 {
        cab.push_str(&format!("Content-Range: bytes {ini}-{fim}/{tamanho}\r\n"));
    }
    cab.push_str("\r\n");
    c.write_all(cab.as_bytes())?;
    if so_cabecalho || comprimento == 0 {
        return c.flush();
    }
    f.seek(SeekFrom::Start(ini))?;
    io::copy(&mut f.take(comprimento), c)?; // sends in chunks, without loading everything into memory
    c.flush()
}

// ---------------- Site archive ----------------

/// Serves a stored file. Copies open in a "sandbox": no
/// scripts and no access to anything outside this server (no trackers).
fn servir_guardado(c: &mut TcpStream, s: &Servidor, id: &str, ts: &str, chave: &str, so_cabecalho: bool) -> io::Result<()> {
    let arq = &s.gerente.arq;
    let (Some(site), Some(versao)) = (arq.site(id), arq.versao(id, ts)) else {
        return simples(c, 404, "não encontrado");
    };
    let item = versao
        .itens
        .get(chave)
        .or_else(|| decodificar(&format!("/{chave}")).and_then(|d| versao.itens.get(&d[1..])))
        .or_else(|| versao.itens.iter().find(|(k, _)| decodificar(&format!("/{k}")).is_some_and(|d| d[1..] == *chave)).map(|(_, v)| v))
        .cloned();
    let Some(item) = item else {
        let corpo = pagina::nao_guardado(&site, chave, id, ts);
        return enviar(c, 404, "text/html; charset=utf-8", Some(pagina::CSP_PAGINAS), corpo.as_bytes(), so_cabecalho);
    };
    let Ok(dados) = arq.ler_objeto(&item) else {
        return simples(c, 404, "não encontrado");
    };
    let tipo = if item.tipo.is_empty() { "application/octet-stream" } else { &item.tipo };
    let mut cab = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Security-Policy: {}\r\n{CABECALHOS_COMUNS}",
        dados.len(),
        if tipo.starts_with("application/pdf") { pagina::CSP_PDF } else { pagina::CSP_GUARDADO }
    );
    if item.gz {
        cab.push_str("Content-Encoding: gzip\r\n");
    }
    cab.push_str("\r\n");
    c.write_all(cab.as_bytes())?;
    if !so_cabecalho {
        c.write_all(&dados)?;
    }
    c.flush()
}

/// Reads "a=1&b=2" (form).
pub fn ler_formulario(corpo: &str) -> Vec<(String, String)> {
    corpo
        .split('&')
        .filter_map(|par| {
            let (k, v) = par.split_once('=')?;
            let d = |t: &str| decodificar(&format!("/{}", t.replace('+', " "))).map(|x| x[1..].to_string());
            Some((d(k)?, d(v)?))
        })
        .collect()
}

/// Archive buttons (add, update, remove). Only from the computer itself,
/// only from its own page (Origin/Sec-Fetch-Site) and only with the one-time password.
fn acao(c: &mut TcpStream, s: &Servidor, p: &Pedido, mut corpo: Vec<u8>, do_proprio_pc: bool) -> io::Result<()> {
    if !do_proprio_pc {
        return simples(c, 403, "só o próprio computador pode mudar o arquivo");
    }
    let mesma_pagina = p.sec_fetch_site.as_deref() == Some("same-origin");
    let origem_ok = match (&p.origem, &p.host) {
        (Some(o), Some(h)) if o == &format!("http://{h}") => true,
        // pages with "no-referrer" send Origin: null; then the browser's confirmation is required
        (Some(o), _) if o == "null" => mesma_pagina,
        (None, _) => true,
        _ => false,
    };
    let site_ok = p.sec_fetch_site.as_deref().is_none_or(|v| v == "same-origin" || v == "none");
    if !origem_ok || !site_ok {
        return simples(c, 403, "pedido de outro site bloqueado");
    }
    let Some(tamanho) = p.tamanho.filter(|&t| t <= MAX_CORPO) else {
        return simples(c, 413, "formulário grande demais");
    };
    while corpo.len() < tamanho {
        let mut b = [0u8; 1024];
        let n = c.read(&mut b)?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&b[..n]);
    }
    corpo.truncate(tamanho);
    let campos = ler_formulario(&String::from_utf8_lossy(&corpo));
    let campo = |k: &str| campos.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("");
    if campo("token") != s.token {
        return simples(c, 403, "página antiga: volte e recarregue");
    }
    let g = &s.gerente;
    let id = campo("id");
    let mensagem = match campo("acao") {
        "adicionar" => match g.adicionar(campo("url")) {
            Ok(_) => "ok",
            Err(_) => "invalido",
        },
        "atualizar" if g.arq.site(id).is_some() => {
            g.enfileirar(id);
            "ok"
        }
        "remover" if g.arq.site(id).is_some() => {
            let _ = g.arq.remover(id);
            "removido"
        }
        _ => "invalido",
    };
    let cab = format!("HTTP/1.1 303 See Other\r\nLocation: /?m={mensagem}\r\nContent-Length: 0\r\nConnection: close\r\n{CABECALHOS_COMUNS}\r\n");
    c.write_all(cab.as_bytes())
}

fn redirecionar(c: &mut TcpStream, destino: &str) -> io::Result<()> {
    let cab = format!(
        "HTTP/1.1 301 Moved Permanently\r\nLocation: {destino}\r\nContent-Length: 0\r\nConnection: close\r\n{CABECALHOS_COMUNS}\r\n"
    );
    c.write_all(cab.as_bytes())
}

fn simples(c: &mut TcpStream, codigo: u16, msg: &str) -> io::Result<()> {
    enviar(c, codigo, "text/plain; charset=utf-8", None, msg.as_bytes(), false)
}

fn enviar(c: &mut TcpStream, codigo: u16, tipo: &str, csp: Option<&str>, corpo: &[u8], so_cabecalho: bool) -> io::Result<()> {
    let motivo = match codigo {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Request Header Fields Too Large",
    };
    let csp = csp.map(|v| format!("Content-Security-Policy: {v}\r\n")).unwrap_or_default();
    let cab = format!(
        "HTTP/1.1 {codigo} {motivo}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nConnection: close\r\n{csp}{CABECALHOS_COMUNS}\r\n",
        corpo.len()
    );
    c.write_all(cab.as_bytes())?;
    if !so_cabecalho {
        c.write_all(corpo)?;
    }
    c.flush()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn bloqueia_fuga_da_pasta() {
        assert_eq!(segmentos_seguros("/livros/index.html"), Some(vec!["livros".into(), "index.html".into()]));
        assert_eq!(segmentos_seguros("/livros/"), Some(vec!["livros".into()]));
        for ruim in ["/../x", "/a/../../x", "/a/./b", "/.git/config", "/a\\..\\b", "/C:/Windows", "/a//b", "/CON", "/nul.txt", "/com1", "/a./b", "/", "//evil.com/"] {
            assert_eq!(segmentos_seguros(ruim), None, "{ruim}");
        }
        assert_eq!(decodificar("/%2e%2e/x").and_then(|t| segmentos_seguros(&t)), None);
        assert_eq!(decodificar("/a%00b"), None);
        assert_eq!(decodificar("/a%zz"), None);
        assert_eq!(decodificar("/m%C3%BAsica").as_deref(), Some("/música"));
    }

    #[test]
    fn host() {
        assert!(host_permitido(Some("127.0.0.1:8080"), 8080, false));
        assert!(host_permitido(Some("localhost:8080"), 8080, false));
        assert!(!host_permitido(Some("192.168.0.10:8080"), 8080, false));
        assert!(host_permitido(Some("192.168.0.10:8080"), 8080, true));
        assert!(host_permitido(Some("[::1]:8080"), 8080, false));
        assert!(!host_permitido(Some("site-malicioso.com:8080"), 8080, true));
        assert!(!host_permitido(Some("127.0.0.1:9999"), 8080, false));
        assert!(!host_permitido(None, 8080, true));
    }

    #[test]
    fn faixas_de_video() {
        assert_eq!(faixa("bytes=0-99", 1000), Some(Ok((0, 99))));
        assert_eq!(faixa("bytes=900-", 1000), Some(Ok((900, 999))));
        assert_eq!(faixa("bytes=-100", 1000), Some(Ok((900, 999))));
        assert_eq!(faixa("bytes=0-5000", 1000), Some(Ok((0, 999))));
        assert_eq!(faixa("bytes=2000-", 1000), Some(Err(())));
        assert_eq!(faixa("bytes=0-1,5-6", 1000), None);
        assert_eq!(faixa("lixo", 1000), None);
    }

    #[test]
    fn pedido() {
        let p = analisar("GET /a HTTP/1.1\r\nHost: x\r\nRange: bytes=0-1\r\n\r\n").unwrap();
        assert_eq!((p.metodo.as_str(), p.alvo.as_str()), ("GET", "/a"));
        assert_eq!(p.range.as_deref(), Some("bytes=0-1"));
        assert!(analisar("GET /a\r\n\r\n").is_none());
        assert!(analisar("GET /a HTTP/1.1 extra\r\n\r\n").is_none());
    }

    #[test]
    fn formulario() {
        let f = ler_formulario("acao=adicionar&url=https%3A%2F%2Fex.com%2F%3Fa%3D1+b&token=x");
        assert_eq!(f[1], ("url".to_string(), "https://ex.com/?a=1 b".to_string()));
        assert_eq!(ler_formulario("a=%zz").len(), 0);
    }
}
