// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Navegador invisível: para páginas que só aparecem com JavaScript, o servidor
//! abre o navegador que JÁ está no computador (Edge, Chrome, Chromium, Brave),
//! sem janela, com um perfil novo e vazio (sem seus logins nem histórico), que é
//! apagado no fim. Não é extensão e não instala nada.
//!
//! Todo o tráfego desse navegador passa por um filtro nosso (proxy local):
//! - bloqueia a rede interna (roteador, outros computadores, o próprio PC);
//! - bloqueia serviços do próprio navegador (contas, atualização, telemetria);
//! - bloqueia rastreadores comuns de sites (análise de visitas, anúncios).

use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::baixar::ip_publico;

/// Endereços que o navegador ou os sites usam para contas, atualização,
/// telemetria e rastreamento. Nada disso é necessário para mostrar a página.
const BLOQUEADOS: &[&str] = &[
    // navegador (Google/Chrome)
    "accounts.google.com", "clients1.google.com", "clients2.google.com", "clients3.google.com", "clients4.google.com",
    "clients5.google.com", "clients6.google.com", "android.clients.google.com", "update.googleapis.com",
    "safebrowsing.googleapis.com", "optimizationguide-pa.googleapis.com", "content-autofill.googleapis.com",
    "www.googleapis.com", "gvt1.com", "gvt2.com", "redirector.gvt1.com",
    // navegador (Microsoft Edge)
    "edge.microsoft.com", "msedge.api.cdp.microsoft.com", "config.edge.skype.com", "edge.activity.windows.com",
    "events.data.microsoft.com", "smartscreen.microsoft.com", "smartscreen-prod.microsoft.com", "msedge.net",
    "data.msn.com", "assets.msn.com", "ntp.msn.com",
    // navegador (Brave)
    "brave.com", "bravesoftware.com",
    // rastreadores e anúncios comuns
    "google-analytics.com", "googletagmanager.com", "googlesyndication.com", "doubleclick.net", "googleadservices.com",
    "connect.facebook.net", "facebook.com", "hotjar.com", "clarity.ms", "scorecardresearch.com", "quantserve.com",
    "taboola.com", "outbrain.com", "criteo.com", "adnxs.com", "amazon-adsystem.com", "tiktok.com", "analytics.tiktok.com",
    "segment.io", "segment.com", "mixpanel.com", "amplitude.com", "newrelic.com", "nr-data.net", "sentry.io",
];

pub fn bloqueado(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    BLOQUEADOS.iter().any(|b| h == *b || h.ends_with(&format!(".{b}")))
}

// ---------------- achar o navegador do sistema ----------------

pub fn encontrar() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SERVIDOR_LOCAL_NAVEGADOR") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut candidatos: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "windows") {
        let pastas: Vec<String> = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var(v).ok()).collect();
        for base in &pastas {
            for rel in [
                r"Microsoft\Edge\Application\msedge.exe",
                r"Google\Chrome\Application\chrome.exe",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
                r"Chromium\Application\chrome.exe",
            ] {
                candidatos.push(Path::new(base).join(rel));
            }
        }
    } else if cfg!(target_os = "macos") {
        for app in ["Google Chrome", "Microsoft Edge", "Chromium", "Brave Browser"] {
            candidatos.push(PathBuf::from(format!("/Applications/{app}.app/Contents/MacOS/{app}")));
        }
    } else {
        let caminho = std::env::var("PATH").unwrap_or_default();
        for nome in ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "microsoft-edge", "microsoft-edge-stable", "brave-browser"] {
            for pasta in caminho.split(':') {
                candidatos.push(Path::new(pasta).join(nome));
            }
        }
    }
    candidatos.into_iter().find(|p| p.is_file())
}

// ---------------- filtro (proxy local) ----------------

pub struct Filtro {
    pub porta: u16,
    parar: Arc<AtomicBool>,
}

impl Drop for Filtro {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.porta)); // acorda o accept para encerrar
    }
}

fn resolver_publico(host: &str, porta: u16, permitir_interna: bool) -> io::Result<SocketAddr> {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if bloqueado(h) {
        return Err(io::Error::other("bloqueado"));
    }
    let enderecos: Vec<SocketAddr> = match h.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, porta)],
        Err(_) => (h, porta).to_socket_addrs()?.collect(),
    };
    let primeiro = *enderecos.first().ok_or_else(|| io::Error::other("sem endereço"))?;
    if !permitir_interna && !enderecos.iter().all(|a| ip_publico(a.ip())) {
        return Err(io::Error::other("rede interna"));
    }
    Ok(primeiro)
}

fn ligar(a: TcpStream, b: TcpStream) {
    let (a2, b2) = match (a.try_clone(), b.try_clone()) {
        (Ok(x), Ok(y)) => (x, y),
        _ => return,
    };
    let t = thread::spawn(move || {
        let (mut de, mut para) = (a2, b2);
        let _ = io::copy(&mut de, &mut para);
        let _ = para.shutdown(Shutdown::Write);
    });
    let (mut de, mut para) = (b, a);
    let _ = io::copy(&mut de, &mut para);
    let _ = para.shutdown(Shutdown::Write);
    let _ = t.join();
}

fn atender_filtro(mut c: TcpStream, permitir_interna: bool) -> io::Result<()> {
    c.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut buf = Vec::new();
    let mut b = [0u8; 4096];
    while !buf.windows(4).any(|j| j == b"\r\n\r\n") {
        let n = c.read(&mut b)?;
        if n == 0 || buf.len() > 32 * 1024 {
            return Ok(());
        }
        buf.extend_from_slice(&b[..n]);
    }
    let fim = buf.windows(4).position(|j| j == b"\r\n\r\n").unwrap() + 4;
    let resto = buf.split_off(fim);
    let cab = String::from_utf8_lossy(&buf).to_string();
    let primeira = cab.lines().next().unwrap_or("");
    let partes: Vec<&str> = primeira.split(' ').collect();
    if partes.len() != 3 {
        return c.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
    }
    let recusar = |c: &mut TcpStream| c.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    if partes[0] == "CONNECT" {
        // HTTPS: o filtro só vê o nome do site, nunca o conteúdo (criptografado)
        let Some((host, porta)) = partes[1].rsplit_once(':') else { return recusar(&mut c) };
        let Ok(porta) = porta.parse::<u16>() else { return recusar(&mut c) };
        let Ok(destino) = resolver_publico(host, porta, permitir_interna) else { return recusar(&mut c) };
        let Ok(s) = TcpStream::connect_timeout(&destino, Duration::from_secs(10)) else { return recusar(&mut c) };
        c.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
        c.set_read_timeout(None)?;
        ligar(c, s);
        return Ok(());
    }
    // HTTP comum: "GET http://site/caminho HTTP/1.1"
    let Some(u) = crate::url::Url::ler(partes[1]).filter(|u| u.esquema == "http") else { return recusar(&mut c) };
    let Ok(destino) = resolver_publico(&u.host, u.porta_real(), permitir_interna) else { return recusar(&mut c) };
    let Ok(mut s) = TcpStream::connect_timeout(&destino, Duration::from_secs(10)) else { return recusar(&mut c) };
    let alvo = match &u.consulta {
        Some(q) => format!("{}?{q}", u.caminho),
        None => u.caminho.clone(),
    };
    let mut novo = format!("{} {alvo} {}\r\n", partes[0], partes[2]);
    for l in cab.lines().skip(1).filter(|l| !l.is_empty()) {
        let k = l.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
        if k != "proxy-connection" && k != "connection" && k != "keep-alive" {
            novo.push_str(l);
            novo.push_str("\r\n");
        }
    }
    novo.push_str("Connection: close\r\n\r\n"); // um pedido por conexão: nada vaza para outro site
    s.write_all(novo.as_bytes())?;
    s.write_all(&resto)?;
    c.set_read_timeout(None)?;
    ligar(c, s);
    Ok(())
}

pub fn iniciar_filtro(permitir_interna: bool) -> io::Result<Filtro> {
    let ouvinte = TcpListener::bind(("127.0.0.1", 0))?;
    let porta = ouvinte.local_addr()?.port();
    let parar = Arc::new(AtomicBool::new(false));
    let p = Arc::clone(&parar);
    let ativas = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for c in ouvinte.incoming() {
            if p.load(Ordering::SeqCst) {
                break;
            }
            let Ok(c) = c else { continue };
            // só o próprio computador usa o filtro
            if !c.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false) || ativas.load(Ordering::SeqCst) > 96 {
                continue;
            }
            ativas.fetch_add(1, Ordering::SeqCst);
            let a = Arc::clone(&ativas);
            thread::spawn(move || {
                let _ = atender_filtro(c, permitir_interna);
                a.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Ok(Filtro { porta, parar })
}

// ---------------- renderizar uma página ----------------

static PERFIS: AtomicU64 = AtomicU64::new(0);

/// Abre a página no navegador invisível e devolve o HTML já montado pelo JavaScript.
pub fn renderizar(navegador: &Path, url: &str, tmp: &Path, filtro: &Filtro) -> Result<String, String> {
    let perfil = tmp.join(format!("perfil-{}-{}", std::process::id(), PERFIS.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::create_dir_all(&perfil);
    let mut cmd = Command::new(navegador);
    cmd.arg("--headless")
        .args([
            "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
            "--disable-component-update", "--disable-sync", "--disable-extensions", "--disable-default-apps",
            "--disable-domain-reliability", "--disable-client-side-phishing-detection", "--disable-breakpad",
            "--no-pings", "--metrics-recording-only", "--mute-audio", "--hide-scrollbars", "--window-size=1280,2000",
            "--disable-features=Translate,OptimizationHints,MediaRouter,AutofillServerCommunication,CertificateTransparencyComponentUpdater,InterestFeedContentSuggestions,PrivacySandboxSettings4,msEdgeShopping,msEdgeCollections",
            "--proxy-bypass-list=<-loopback>",
            "--virtual-time-budget=8000",
        ])
        .arg(format!("--proxy-server=http://127.0.0.1:{}", filtro.porta))
        .arg(format!("--user-data-dir={}", perfil.display()));
    if std::env::var("SERVIDOR_LOCAL_NAVEGADOR_SEM_SANDBOX").is_ok_and(|v| v == "1") {
        cmd.arg("--no-sandbox"); // só para testes como administrador no Linux
    }
    cmd.arg("--dump-dom").arg("--").arg(url).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut filho = cmd.spawn().map_err(|_| "não consegui abrir o navegador".to_string())?;
    let mut saida = filho.stdout.take().unwrap();
    let leitor = thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut saida).take(30 * 1024 * 1024).read_to_end(&mut v);
        v
    });
    let inicio = Instant::now();
    let terminou = loop {
        match filho.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if inicio.elapsed() > Duration::from_secs(45) => break false,
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(_) => break false,
        }
    };
    if !terminou {
        let _ = filho.kill();
        let _ = filho.wait();
    }
    let html = leitor.join().unwrap_or_default();
    for _ in 0..10 {
        if std::fs::remove_dir_all(&perfil).is_ok() || !perfil.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(300)); // o Windows às vezes demora a liberar
    }
    if html.len() < 50 {
        return Err(if terminou { "o navegador não devolveu a página".into() } else { "o navegador demorou demais".into() });
    }
    Ok(String::from_utf8_lossy(&html).into_owned())
}

/// A página parece depender de JavaScript? (pouco texto visível ou marcas de apps)
pub fn precisa_de_javascript(html: &str) -> bool {
    let minus = html.to_ascii_lowercase();
    let marcas = ["id=\"root\"></div>", "id=\"app\"></div>", "id=\"__next\"", "id=\"__nuxt\"", "ng-version", "data-reactroot", "enable javascript", "habilite o javascript", "ative o javascript"];
    if marcas.iter().any(|m| minus.contains(m)) {
        return true;
    }
    // conta o texto visível (fora de tags, scripts e estilos), byte a byte
    // (sem fatiar o texto: assim letras acentuadas nunca são cortadas ao meio)
    fn achar(b: &[u8], de: usize, agulha: &[u8]) -> Option<usize> {
        b.get(de..)?.windows(agulha.len()).position(|j| j == agulha).map(|p| de + p)
    }
    let b = minus.as_bytes();
    let mut visivel = 0usize;
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<script") || b[i..].starts_with(b"<style") {
            let fecho: &[u8] = if b[i..].starts_with(b"<script") { b"</script" } else { b"</style" };
            i = achar(b, i + 1, fecho).map(|f| f + 1).unwrap_or(b.len());
            continue;
        }
        if b[i] == b'<' {
            i = achar(b, i, b">").map(|f| f + 1).unwrap_or(b.len());
            continue;
        }
        if !b[i].is_ascii_whitespace() && (b[i] & 0xC0) != 0x80 {
            visivel += 1; // conta letras, não bytes de continuação do UTF-8
        }
        i += 1;
    }
    visivel < 300
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn bloqueios() {
        assert!(bloqueado("accounts.google.com"));
        assert!(bloqueado("www.google-analytics.com"));
        assert!(bloqueado("config.edge.skype.com"));
        assert!(!bloqueado("fonts.googleapis.com"));
        assert!(!bloqueado("exemplo.com.br"));
        assert!(!bloqueado("notgoogle-analytics.com"));
    }

    #[test]
    fn detecta_paginas_de_javascript() {
        assert!(precisa_de_javascript("<html><body><div id=\"root\"></div><script src=app.js></script></body></html>"));
        assert!(precisa_de_javascript("<html><body><script>var x = 'muito texto '.repeat(1000)</script></body></html>"));
        let normal = format!("<html><body><p>{}</p></body></html>", "texto de verdade ".repeat(40));
        assert!(!precisa_de_javascript(&normal));
        // acentos e emojis em qualquer posição não podem derrubar o servidor
        for t in ["<p>á</p>", "á<", "<á", "<scriptá", "ção<style>é</style>ü", "🙂<🙂>🙂"] {
            let _ = precisa_de_javascript(t);
        }
    }

    #[test]
    fn filtro_recusa_rede_interna() {
        let f = iniciar_filtro(false).unwrap();
        for pedido in ["CONNECT 127.0.0.1:22 HTTP/1.1\r\n\r\n", "CONNECT 192.168.0.1:443 HTTP/1.1\r\n\r\n", "CONNECT accounts.google.com:443 HTTP/1.1\r\n\r\n", "GET http://10.0.0.1/ HTTP/1.1\r\nHost: 10.0.0.1\r\n\r\n"] {
            let mut c = TcpStream::connect(("127.0.0.1", f.porta)).unwrap();
            c.write_all(pedido.as_bytes()).unwrap();
            let mut r = String::new();
            let _ = c.read_to_string(&mut r);
            assert!(r.starts_with("HTTP/1.1 403"), "{pedido} -> {r}");
        }
    }
}
