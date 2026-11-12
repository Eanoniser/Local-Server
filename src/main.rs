// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Servidor Local — uma "Cloudflare" pequena que roda em qualquer computador,
//! inclusive antigo, sem internet. Serve os sites e apps da pasta `sites/`.
//! Opcional: compartilhar pelo Wi-Fi com celulares e computadores por perto.
//!
//! Também guarda cópias de sites (arquivo pessoal, com versões datadas).
//!
//! Usa apenas a biblioteca padrão do Rust (sem dependências). Para HTTPS,
//! usa o `curl` que já vem no sistema. Não guarda registro de quem acessa.

#![forbid(unsafe_code)]

mod arquivo;
mod baixar;
mod gzip;
mod html;
mod http;
mod imagens;
mod invisivel;
mod sha256;
mod textos;
mod url;
mod pagina;

use std::fs;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use textos::t;

const MAX_CONEXOES: usize = 128;

const CONFIG_PADRAO: &str = "\
# Local Server — configuração / settings
# Depois de mudar, feche e abra o programa de novo. / After changing, close and reopen the program.

# Porta (número entre 1024 e 65535)
porta = 8080

# compartilhar_na_rede:
#   nao = só este computador abre os sites (mais seguro) / this computer only (safer)
#   sim = celulares e computadores no mesmo Wi-Fi/cabo também abrem / devices on the same Wi-Fi too
compartilhar_na_rede = nao

# Arquivo de sites (guardar cópias):
# máximo de páginas por site, tamanho máximo por site (MB) e de quantas em
# quantas horas conferir se o site mudou.
arquivo_paginas_max = 200
arquivo_mb_max = 300
arquivo_atualizar_horas = 24

# Páginas que só aparecem com JavaScript: o servidor usa o navegador que já
# está no computador (Edge, Chrome...), invisível, com perfil vazio e filtro de
# privacidade. auto = só quando precisa, sempre, ou nunca.
arquivo_navegador = auto

# Imagens / images:
# Os dados escondidos das fotos (câmera, data, localização GPS) são sempre
# apagados, sem mudar nenhum pixel. / Hidden photo data (camera, date, GPS) is
# always removed, without changing any pixel.
# arquivo_economizar_imagens = sim: fotos grandes viram WebP (30% a 70% menores,
# diferença quase invisível), usando o navegador do computador, sem internet.
# Desligado por padrão. / sim = big photos become WebP (much smaller, tiny quality loss).
arquivo_economizar_imagens = nao
";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub porta: u16,
    pub compartilhar: bool,
    pub paginas: usize,
    pub mb: u64,
    pub horas: u64,
    pub navegador: String,
    pub economizar_imagens: bool,
}

pub fn ler_config(texto: &str) -> Config {
    let mut c = Config { porta: 8080, compartilhar: false, paginas: 200, mb: 300, horas: 24, navegador: "auto".into(), economizar_imagens: false };
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.starts_with('#') {
            continue;
        }
        let Some((k, v)) = linha.split_once('=') else { continue };
        let (k, v) = (k.trim().to_lowercase(), v.trim().to_lowercase());
        match k.as_str() {
            "porta" => {
                if let Ok(p) = v.parse::<u16>() {
                    if p >= 1024 {
                        c.porta = p;
                    }
                }
            }
            "compartilhar_na_rede" => c.compartilhar = matches!(v.as_str(), "sim" | "s" | "yes" | "true" | "1"),
            "arquivo_paginas_max" => c.paginas = v.parse::<usize>().unwrap_or(200).clamp(1, 100_000),
            "arquivo_mb_max" => c.mb = v.parse::<u64>().unwrap_or(300).clamp(1, 1_000_000),
            "arquivo_navegador" if matches!(v.as_str(), "auto" | "sempre" | "nunca") => c.navegador = v.clone(),
            "arquivo_economizar_imagens" => c.economizar_imagens = matches!(v.as_str(), "sim" | "s" | "yes" | "true" | "1"),
            "arquivo_atualizar_horas" => c.horas = v.parse::<u64>().unwrap_or(24).clamp(1, 24 * 365),
            _ => {}
        }
    }
    c
}

/// Estado compartilhado entre as conexões.
pub struct Servidor {
    pub raiz: PathBuf,
    pub config: Config,
    pub enderecos: Vec<String>,
    pub gerente: Arc<baixar::Gerente>,
    /// Senha de uso único desta execução: protege os botões contra outros sites.
    pub token: String,
}

/// Número aleatório da biblioteca padrão (semente do sistema operacional).
fn aleatorio() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) + i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

fn main() {
    textos::definir(textos::do_sistema());
    println!("{}", t("j-titulo"));
    println!("--------------------------------------");
    let pasta = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));

    let sites = pasta.join("sites");
    if let Err(e) = fs::create_dir_all(&sites) {
        return erro_fatal(&format!("não consegui criar a pasta {}: {e}", sites.display()));
    }
    let raiz = match sites.canonicalize() {
        Ok(r) => r,
        Err(e) => return erro_fatal(&format!("pasta sites inválida: {e}")),
    };

    let arquivo_config = pasta.join("config.txt");
    let texto = match fs::read_to_string(&arquivo_config) {
        Ok(t) => t,
        Err(_) => {
            let _ = fs::write(&arquivo_config, CONFIG_PADRAO);
            CONFIG_PADRAO.to_string()
        }
    };
    let config = ler_config(&texto);

    let ip = if config.compartilhar { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::LOCALHOST) };
    let ouvinte = match TcpListener::bind(SocketAddr::new(ip, config.porta)) {
        Ok(o) => o,
        Err(e) => {
            return erro_fatal(&format!("{} ({e})", t("j-porta").replace("{p}", &config.porta.to_string())))
        }
    };

    let arq = match arquivo::Arquivo::novo(pasta.join("arquivo")) {
        Ok(a) => a,
        Err(e) => return erro_fatal(&format!("não consegui criar a pasta arquivo: {e}")),
    };
    let curl = baixar::Curl::detectar();
    let navegador = if config.navegador == "nunca" && !config.economizar_imagens { None } else { invisivel::encontrar() };
    let limites = baixar::Limites {
        navegador_modo: config.navegador.clone(),
        paginas: config.paginas,
        bytes: config.mb * 1024 * 1024,
        atualizar_horas: config.horas,
        permitir_rede_interna: std::env::var("SERVIDOR_LOCAL_TESTE_REDE_INTERNA").is_ok_and(|v| v == "1"),
        economizar_imagens: config.economizar_imagens,
    };
    let curl_ok = curl.disponivel;
    let gerente = Arc::new(baixar::Gerente {
        arq,
        curl,
        navegador: navegador.clone(),
        limites,
        fila: std::sync::Mutex::new(Default::default()),
        progresso: std::sync::Mutex::new(None),
    });
    baixar::iniciar(Arc::clone(&gerente));

    let local = format!("http://127.0.0.1:{}/", config.porta);
    let enderecos = if config.compartilhar { ips_da_rede() } else { Vec::new() };
    println!("{}: {}", t("j-pasta"), raiz.display());
    println!("{}: {local}", t("j-aqui"));
    if config.compartilhar {
        if enderecos.is_empty() {
            println!("{}", t("j-sem-ip"));
            println!("{}", t("j-veja-ip").replace("{p}", &config.porta.to_string()));
        }
        for e in &enderecos {
            println!("{}: http://{e}:{}/", t("j-celular"), config.porta);
        }
        println!("{}", t("j-firewall"));
    } else {
        println!("{}", t("j-so-aqui"));
    }
    if curl_ok {
        println!("{}", t("j-curl-ok"));
    } else {
        println!("{}", t("j-curl-nao"));
    }
    if config.economizar_imagens {
        println!("{}", if navegador.is_some() { t("j-img-eco") } else { t("j-img-eco-sem") });
    }
    match &navegador {
        Some(_) if config.navegador == "nunca" => println!("{}", t("j-nav-off")),
        Some(n) => println!("{}", t("j-nav-usa").replace("{n}", &n.file_name().unwrap_or_default().to_string_lossy())),
        None if config.navegador == "nunca" => println!("{}", t("j-nav-off")),
        None => println!("{}", t("j-nav-nenhum")),
    }
    println!("{}", t("j-fechar"));
    abrir_navegador(&local);

    let servidor = Arc::new(Servidor { raiz, config, enderecos, gerente, token: aleatorio() });
    let ativas = Arc::new(AtomicUsize::new(0));
    for conexao in ouvinte.incoming() {
        let Ok(conexao) = conexao else { continue };
        if ativas.load(Ordering::SeqCst) >= MAX_CONEXOES {
            continue; // descarta excesso (proteção contra sobrecarga)
        }
        ativas.fetch_add(1, Ordering::SeqCst);
        let (s, a) = (Arc::clone(&servidor), Arc::clone(&ativas));
        thread::spawn(move || {
            let _ = http::atender(conexao, &s);
            a.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Descobre o IP deste computador na rede local, sem enviar nenhum pacote
/// (um "connect" UDP só consulta a tabela de rotas do sistema).
fn ips_da_rede() -> Vec<String> {
    let mut ips = Vec::new();
    for destino in ["10.254.254.254:9", "192.168.254.254:9", "172.31.254.254:9"] {
        let Ok(s) = UdpSocket::bind("0.0.0.0:0") else { continue };
        if s.connect(destino).is_err() {
            continue;
        }
        if let Ok(a) = s.local_addr() {
            let ip = a.ip();
            if !ip.is_loopback() && !ip.is_unspecified() {
                let t = ip.to_string();
                if !ips.contains(&t) {
                    ips.push(t);
                }
            }
        }
    }
    ips
}

fn abrir_navegador(url: &str) {
    let r = if cfg!(target_os = "windows") {
        Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if r.is_err() {
        println!("{}: {url}", t("j-abra"));
    }
}

fn erro_fatal(msg: &str) {
    eprintln!("{}: {msg}", t("j-erro"));
    println!("{}", t("j-enter"));
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn config() {
        assert_eq!(ler_config(CONFIG_PADRAO), Config { porta: 8080, compartilhar: false, paginas: 200, mb: 300, horas: 24, navegador: "auto".into(), economizar_imagens: false });
        assert_eq!(ler_config("arquivo_navegador = sempre").navegador, "sempre");
        assert_eq!(ler_config("arquivo_navegador = qualquer").navegador, "auto");
        let c = ler_config("porta = 9000\ncompartilhar_na_rede = SIM\narquivo_paginas_max = 0\n");
        assert_eq!((c.porta, c.compartilhar, c.paginas), (9000, true, 1));
        assert_eq!(ler_config("porta = 80").porta, 8080); // porta baixa é recusada
        assert_eq!(ler_config("porta = abc").porta, 8080);
        assert!(!ler_config("# compartilhar_na_rede = sim").compartilhar);
    }
}
