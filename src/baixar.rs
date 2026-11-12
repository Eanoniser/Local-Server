// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Baixa sites usando o `curl` que já vem no sistema (Windows 10/11, Mac, Linux).
//! O nosso código não traz biblioteca de criptografia: o HTTPS é do sistema.
//!
//! Travas de segurança:
//! - só http/https; nunca endereços da rede interna (roteador, outros PCs);
//! - o IP é verificado e "travado" (--resolve) antes de cada pedido;
//! - redirecionamentos são seguidos à mão, um a um, com a mesma verificação;
//! - o curl ignora qualquer configuração do usuário (-q);
//! - limites de páginas, tamanho e tempo; pausa entre pedidos; respeita robots.txt.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::arquivo::{agora, titulo_html, Arquivo, Item, Site, Versao};
use crate::html::{ate_raiz, para_atributo, reescrever_css, reescrever_html, Mapa, Tipo};
use crate::url::Url;

const AGENTE: &str = "ServidorLocal/1.0 (arquivo pessoal offline)";
const MAX_RECURSO: u64 = 25 * 1024 * 1024;
const MAX_REDIRECIONAMENTOS: usize = 5;
const MAX_RECURSOS: usize = 3000;

#[derive(Clone, Debug)]
pub struct Limites {
    /// "auto" (só páginas que precisam), "sempre" ou "nunca"
    pub navegador_modo: String,
    pub paginas: usize,
    pub bytes: u64,
    pub atualizar_horas: u64,
    pub permitir_rede_interna: bool, // só para testes (variável de ambiente)
    /// modo econômico: fotos grandes viram WebP (pelo navegador do computador)
    pub economizar_imagens: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Progresso {
    pub id: String,
    pub paginas: usize,
    pub recursos: usize,
    pub bytes: u64,
}

// ---------------- curl do sistema ----------------

pub struct Curl {
    pub disponivel: bool,
    pub comprime: bool,
}

impl Curl {
    pub fn detectar() -> Curl {
        match Command::new("curl").arg("-V").output() {
            Ok(s) if s.status.success() => {
                let v = String::from_utf8_lossy(&s.stdout).to_lowercase();
                Curl { disponivel: true, comprime: v.contains("libz") || v.contains("zlib") }
            }
            _ => Curl { disponivel: false, comprime: false },
        }
    }
}

pub fn ip_publico(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_multicast()
                || v.is_unspecified()
                || v.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..20).contains(&o[1]))
                || o[0] >= 240)
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return ip_publico(IpAddr::V4(v4));
            }
            let s = v.segments();
            !(v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || (s[0] == 0x64 && s[1] == 0xff9b))
        }
    }
}

/// Descobre o IP do site e recusa endereços internos.
fn resolver(u: &Url, permitir_interna: bool) -> Result<IpAddr, String> {
    let host = u.host.trim_start_matches('[').trim_end_matches(']');
    let ips: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => (host, u.porta_real())
            .to_socket_addrs()
            .map_err(|_| format!("não encontrei o endereço {host}"))?
            .map(|a| a.ip())
            .collect(),
    };
    let primeiro = *ips.first().ok_or("endereço sem IP")?;
    if !permitir_interna && !ips.iter().all(|&ip| ip_publico(ip)) {
        return Err("endereço da rede interna bloqueado".into());
    }
    Ok(primeiro)
}

pub struct Resposta {
    pub status: u16,
    pub tipo: String,
    pub local: Option<String>,
    pub corpo: Vec<u8>,
}

static CONTADOR: AtomicU64 = AtomicU64::new(0);

fn buscar_uma(curl: &Curl, u: &Url, max: u64, tmp: &Path, permitir_interna: bool) -> Result<Resposta, String> {
    let ip = resolver(u, permitir_interna)?;
    let n = CONTADOR.fetch_add(1, Ordering::SeqCst);
    let cab = tmp.join(format!("c{}-{n}.txt", std::process::id()));
    let corpo = tmp.join(format!("b{}-{n}.bin", std::process::id()));
    let ip_txt = match ip {
        IpAddr::V6(v) => format!("[{v}]"),
        IpAddr::V4(v) => v.to_string(),
    };
    let mut c = Command::new("curl");
    c.arg("-q") // ignora .curlrc do usuário (precisa ser o primeiro)
        .args(["-sS", "--proto", "=http,https", "--max-time", "60", "--connect-timeout", "15"])
        .args(["--max-filesize", &max.to_string(), "-A", AGENTE])
        .args(["-H", "Accept-Language: pt-BR,pt;q=0.9,en;q=0.8"])
        .arg("--resolve")
        .arg(format!("{}:{}:{ip_txt}", u.host, u.porta_real()))
        .arg("-D")
        .arg(&cab)
        .arg("-o")
        .arg(&corpo)
        .args(["-w", "%{http_code}"]);
    if curl.comprime {
        c.arg("--compressed");
    }
    c.arg("--").arg(u.texto());
    let saida = c.output().map_err(|_| "curl não encontrado".to_string())?;
    let codigo = String::from_utf8_lossy(&saida.stdout).trim().parse::<u16>().unwrap_or(0);
    let cabecalho = fs::read_to_string(&cab).unwrap_or_default();
    let dados = fs::read(&corpo).unwrap_or_default();
    let _ = fs::remove_file(&cab);
    let _ = fs::remove_file(&corpo);
    if codigo == 0 {
        let erro = String::from_utf8_lossy(&saida.stderr);
        return Err(format!("falha ao baixar: {}", erro.trim().chars().take(160).collect::<String>()));
    }
    let mut r = Resposta { status: codigo, tipo: String::new(), local: None, corpo: dados };
    for l in cabecalho.lines() {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-type" => r.tipo = v.trim().to_string(),
                "location" => r.local = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    Ok(r)
}

/// Busca seguindo redirecionamentos um a um (cada destino é verificado).
fn buscar(curl: &Curl, u: &Url, max: u64, tmp: &Path, permitir_interna: bool) -> Result<(Url, Resposta), String> {
    let mut atual = u.clone();
    for _ in 0..=MAX_REDIRECIONAMENTOS {
        let r = buscar_uma(curl, &atual, max, tmp, permitir_interna)?;
        if (300..400).contains(&r.status) {
            let destino = r.local.as_deref().and_then(|l| atual.juntar(l)).ok_or("redirecionamento inválido")?;
            atual = destino;
            continue;
        }
        if r.status != 200 {
            return Err(format!("o site respondeu {}", r.status));
        }
        return Ok((atual, r));
    }
    Err("redirecionamentos demais".into())
}

// ---------------- robots.txt ----------------

#[derive(Default)]
pub struct Robots {
    regras: Vec<(bool, String)>, // (permitido, prefixo)
}

impl Robots {
    pub fn ler(texto: &str) -> Robots {
        let mut r = Robots::default();
        let mut grupo_nosso = false;
        let mut lendo_agentes = false;
        for l in texto.lines() {
            let l = l.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = l.split_once(':') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "user-agent" => {
                    if !lendo_agentes {
                        grupo_nosso = false;
                    }
                    lendo_agentes = true;
                    let a = v.to_ascii_lowercase();
                    if a == "*" || a.contains("servidorlocal") {
                        grupo_nosso = true;
                    }
                }
                "disallow" | "allow" => {
                    lendo_agentes = false;
                    // regras absurdas (muitos "*") são ignoradas para não travar a conferência
                    if grupo_nosso && !v.is_empty() && v.len() <= 300 && v.matches('*').count() <= 6 {
                        r.regras.push((k == "allow", v.to_string()));
                    }
                }
                _ => lendo_agentes = false,
            }
        }
        r
    }

    pub fn permite(&self, caminho: &str) -> bool {
        let mut melhor: Option<(usize, bool)> = None;
        for (ok, regra) in &self.regras {
            if casa_padrao(regra.as_bytes(), caminho.as_bytes()) && melhor.is_none_or(|(t, _)| regra.len() > t) {
                melhor = Some((regra.len(), *ok));
            }
        }
        melhor.map(|(_, ok)| ok).unwrap_or(true)
    }
}

/// Padrão do robots.txt: começa no início do caminho, "*" = qualquer coisa, "$" no fim = termina ali.
fn casa_padrao(p: &[u8], c: &[u8]) -> bool {
    match p.first() {
        None => true, // padrão acabou: é prefixo
        Some(b'$') if p.len() == 1 => c.is_empty(),
        Some(b'*') => (0..=c.len()).any(|i| casa_padrao(&p[1..], &c[i..])),
        Some(&x) => c.first() == Some(&x) && casa_padrao(&p[1..], &c[1..]),
    }
}

// ---------------- Arquivar um site ----------------

struct Coletor<'a> {
    origem: String,
    robots: &'a Robots,
    prefixo: String, // "../../" até a raiz da versão
    paginas: Vec<Url>,
    recursos: Vec<Url>,
}

impl Mapa for Coletor<'_> {
    fn mapear(&mut self, alvo: &Url, tipo: Tipo) -> String {
        let local = format!("{}{}", self.prefixo, para_atributo(&alvo.chave()));
        match tipo {
            Tipo::Recurso => {
                self.recursos.push(alvo.clone());
                local
            }
            // páginas do mesmo site sempre apontam para a cópia: se não foram guardadas
            // (limite ou robots.txt), aparece um aviso em vez de sair para a internet
            Tipo::Pagina if alvo.origem() == self.origem => {
                if self.robots.permite(&alvo.caminho) {
                    self.paginas.push(alvo.clone());
                }
                local
            }
            Tipo::Pagina => alvo.texto(),
        }
    }
}

fn eh_html(tipo: &str) -> bool {
    let t = tipo.to_ascii_lowercase();
    t.starts_with("text/html") || t.starts_with("application/xhtml")
}

fn eh_css(tipo: &str, u: &Url) -> bool {
    tipo.to_ascii_lowercase().starts_with("text/css") || u.caminho.to_ascii_lowercase().ends_with(".css")
}

fn eh_script(tipo: &str) -> bool {
    let t = tipo.to_ascii_lowercase();
    t.contains("javascript") || t.contains("ecmascript")
}

pub fn arquivar(
    arq: &Arquivo,
    curl: &Curl,
    site: &mut Site,
    lim: &Limites,
    progresso: &Mutex<Option<Progresso>>,
    navegador: Option<&Path>,
) -> Result<bool, String> {
    if !curl.disponivel {
        return Err("este sistema não tem curl (Windows 7?)".into());
    }
    let inicio = Url::ler(&site.url).ok_or("endereço inválido")?;
    let tmp = arq.tmp();
    let mut prog = Progresso { id: site.id.clone(), ..Default::default() };
    let atualizar = |p: &Progresso| *progresso.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());

    let navegador_imagens = navegador.filter(|_| lim.economizar_imagens);
    // a primeira página pode redirecionar (http -> https, www); a origem passa a ser a final
    let (final_inicio, resp_inicio) = buscar(curl, &inicio, MAX_RECURSO, &tmp, lim.permitir_rede_interna)?;
    let origem = final_inicio.origem();
    let robots_url = Url::ler(&format!("{origem}/robots.txt")).ok_or("endereço inválido")?;
    let robots = buscar(curl, &robots_url, 512 * 1024, &tmp, lim.permitir_rede_interna)
        .map(|(_, r)| Robots::ler(&String::from_utf8_lossy(&r.corpo)))
        .unwrap_or_default();
    if !robots.permite(&final_inicio.caminho) {
        return Err("o site pede (robots.txt) para não ser copiado automaticamente".into());
    }

    // navegador invisível (com filtro de privacidade) para páginas que dependem de JavaScript
    let navegador = navegador.filter(|_| lim.navegador_modo != "nunca");
    let filtro = match navegador {
        Some(_) => crate::invisivel::iniciar_filtro(lim.permitir_rede_interna).ok(),
        None => None,
    };
    let montar_pagina = |url: &Url, bruto: &[u8]| -> String {
        let texto = String::from_utf8_lossy(bruto).into_owned();
        if let (Some(nav), Some(f)) = (navegador, filtro.as_ref()) {
            if lim.navegador_modo == "sempre" || crate::invisivel::precisa_de_javascript(&texto) {
                if let Ok(montado) = crate::invisivel::renderizar(nav, &url.texto(), &tmp, f) {
                    return montado;
                }
            }
        }
        texto
    };

    let mut versao = Versao { inicio: final_inicio.chave(), ..Default::default() };
    let mut vistas: HashSet<String> = HashSet::new();
    let mut fila: VecDeque<(Url, Option<Resposta>)> = VecDeque::from([(final_inicio.clone(), Some(resp_inicio))]);
    vistas.insert(final_inicio.chave());
    vistas.insert(inicio.chave());
    let mut recursos: Vec<Url> = Vec::new();
    let mut apelidos: Vec<(String, String)> = vec![(inicio.chave(), final_inicio.chave())];

    while let Some((pedida, pronta)) = fila.pop_front() {
        if prog.paginas >= lim.paginas || prog.bytes >= lim.bytes {
            break;
        }
        let (url, resp) = match pronta {
            Some(r) => (pedida.clone(), r),
            None => {
                thread::sleep(Duration::from_millis(800)); // gentil com o site
                match buscar(curl, &pedida, MAX_RECURSO, &tmp, lim.permitir_rede_interna) {
                    Ok(x) => x,
                    Err(_) => continue,
                }
            }
        };
        if url.chave() != pedida.chave() {
            apelidos.push((pedida.chave(), url.chave()));
        }
        prog.paginas += 1;
        prog.bytes += resp.corpo.len() as u64;
        if eh_html(&resp.tipo) {
            let ajustar_titulo = site.titulo.is_empty() || site.titulo == site.url;
            let texto = montar_pagina(&url, &resp.corpo);
            let mut col = Coletor { origem: origem.clone(), robots: &robots, prefixo: ate_raiz(&url.chave()), paginas: vec![], recursos: vec![] };
            if ajustar_titulo {
                if let Some(t) = titulo_html(texto.as_bytes()) {
                    site.titulo = t;
                }
            }
            let novo = reescrever_html(&texto, &url, &mut col);
            versao.itens.insert(url.chave(), arq.guardar(novo.as_bytes(), "text/html; charset=utf-8").map_err(|e| e.to_string())?);
            for p in col.paginas {
                if vistas.insert(p.chave()) {
                    fila.push_back((p, None));
                }
            }
            recursos.extend(col.recursos);
        } else if !eh_script(&resp.tipo) {
            let tipo = if resp.tipo.is_empty() { "application/octet-stream".to_string() } else { resp.tipo.clone() };
            versao.itens.insert(url.chave(), arq.guardar(&resp.corpo, &tipo).map_err(|e| e.to_string())?);
        }
        atualizar(&prog);
    }

    // recursos (imagens, estilos, fontes), inclusive de outros endereços públicos
    let mut feitos: HashSet<String> = HashSet::new();
    // fotos à espera do modo econômico (guardadas num arquivo temporário até a conversão)
    let mut pendentes: Vec<(Vec<String>, std::path::PathBuf, String)> = Vec::new();
    let mut i = 0;
    while i < recursos.len() && feitos.len() < MAX_RECURSOS && prog.bytes < lim.bytes {
        let r = recursos[i].clone();
        i += 1;
        if !feitos.insert(r.chave()) || versao.itens.contains_key(&r.chave()) {
            continue;
        }
        thread::sleep(Duration::from_millis(150));
        let Ok((url, resp)) = buscar(curl, &r, MAX_RECURSO, &tmp, lim.permitir_rede_interna) else { continue };
        if eh_html(&resp.tipo) || eh_script(&resp.tipo) {
            continue;
        }
        prog.recursos += 1;
        prog.bytes += resp.corpo.len() as u64;
        let item: Item = if eh_css(&resp.tipo, &url) {
            let mut col = Coletor { origem: origem.clone(), robots: &robots, prefixo: ate_raiz(&r.chave()), paginas: vec![], recursos: vec![] };
            let novo = reescrever_css(&String::from_utf8_lossy(&resp.corpo), &url, &mut col);
            recursos.extend(col.recursos);
            arq.guardar(novo.as_bytes(), "text/css; charset=utf-8").map_err(|e| e.to_string())?
        } else {
            let tipo = if resp.tipo.is_empty() { crate::pagina::tipo_do_arquivo(Path::new(&url.caminho)).to_string() } else { resp.tipo.clone() };
            if navegador_imagens.is_some() && crate::imagens::candidata_webp(&resp.corpo, &tipo) {
                let temp = tmp.join(format!("foto-{}-{}", std::process::id(), pendentes.len()));
                if std::fs::write(&temp, &resp.corpo).is_ok() {
                    let mut chaves = vec![r.chave()];
                    if url.chave() != r.chave() {
                        chaves.push(url.chave());
                    }
                    pendentes.push((chaves, temp, tipo));
                    atualizar(&prog);
                    continue;
                }
            }
            arq.guardar(&resp.corpo, &tipo).map_err(|e| e.to_string())?
        };
        versao.itens.insert(r.chave(), item.clone());
        if url.chave() != r.chave() {
            versao.itens.insert(url.chave(), item);
        }
        atualizar(&prog);
    }
    // modo econômico: converte as fotos em lotes; se não ficar bem menor, guarda a original
    if let Some(nav) = navegador_imagens {
        let mut ini = 0;
        while ini < pendentes.len() {
            let mut fim = ini;
            let mut soma = 0u64;
            let mut lote: Vec<Vec<u8>> = Vec::new();
            while fim < pendentes.len() && (lote.is_empty() || soma < 24 * 1024 * 1024) && lote.len() < 40 {
                let d = std::fs::read(&pendentes[fim].1).unwrap_or_default();
                soma += d.len() as u64;
                lote.push(d);
                fim += 1;
            }
            let entrada: Vec<(&[u8], &str)> = lote.iter().zip(&pendentes[ini..fim]).map(|(d, p)| (d.as_slice(), p.2.as_str())).collect();
            let convertidas = crate::imagens::para_webp(nav, &tmp, &entrada);
            for ((d, (chaves, temp, tipo)), webp) in lote.iter().zip(&pendentes[ini..fim]).zip(convertidas) {
                let item = match webp {
                    Some(w) => arq.guardar(&w, "image/webp"),
                    None if !d.is_empty() => arq.guardar(d, tipo),
                    None => continue,
                }
                .map_err(|e| e.to_string())?;
                for k in chaves {
                    versao.itens.insert(k.clone(), item.clone());
                }
                let _ = std::fs::remove_file(temp);
            }
            ini = fim;
        }
    }
    for (de, para) in apelidos {
        if let Some(item) = versao.itens.get(&para).cloned() {
            versao.itens.entry(de).or_insert(item);
        }
    }
    if versao.itens.is_empty() {
        return Err("nada foi baixado".into());
    }
    arq.gravar_versao(&site.id, &versao).map_err(|e| e.to_string())
}

// ---------------- Gerente: fila, atualização automática e importação ----------------

pub struct Gerente {
    // (os Mutex abaixo continuam usáveis mesmo se um download falhar no meio)
    pub arq: Arquivo,
    pub curl: Curl,
    pub navegador: Option<std::path::PathBuf>,
    pub limites: Limites,
    pub fila: Mutex<VecDeque<String>>,
    pub progresso: Mutex<Option<Progresso>>,
}

pub fn id_para(u: &Url) -> String {
    let host: String = u.host.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').take(50).collect();
    format!("{host}-{}", &crate::sha256::sha256_hex(u.texto().as_bytes())[..8])
}

impl Gerente {
    /// Adiciona um site (ou pede atualização, se já existe).
    pub fn adicionar(&self, endereco: &str) -> Result<String, String> {
        let u = Url::ler(endereco).ok_or("endereço inválido: use http:// ou https://")?;
        let id = id_para(&u);
        if self.arq.site(&id).is_none() {
            let t = agora();
            let s = Site { id: id.clone(), url: u.texto(), titulo: u.texto(), tipo: "web".into(), criado: t, verificado: 0, erro: String::new() };
            self.arq.salvar_site(&s).map_err(|e| e.to_string())?;
        }
        self.enfileirar(&id);
        Ok(id)
    }

    pub fn enfileirar(&self, id: &str) {
        let mut f = self.fila.lock().unwrap_or_else(|e| e.into_inner());
        if !f.iter().any(|x| x == id) {
            f.push_back(id.to_string());
        }
    }

    pub fn na_fila(&self, id: &str) -> bool {
        self.fila.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|x| x == id) || self.progresso.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|p| p.id == id)
    }

    fn trabalhar(&self) {
        let proximo = self.fila.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
        if let Some(id) = proximo {
            if let Some(mut site) = self.arq.site(&id) {
                if site.tipo == "web" {
                    *self.progresso.lock().unwrap_or_else(|e| e.into_inner()) = Some(Progresso { id: id.clone(), ..Default::default() });
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        arquivar(&self.arq, &self.curl, &mut site, &self.limites, &self.progresso, self.navegador.as_deref())
                    }))
                    .unwrap_or_else(|_| Err("erro interno ao baixar este site".into()));
                    // o site pode ter sido removido enquanto baixava
                    if self.arq.site(&id).is_some() {
                        site.verificado = agora();
                        site.erro = match r {
                            Ok(_) => String::new(),
                            Err(e) => e,
                        };
                        let _ = self.arq.salvar_site(&site);
                    } else {
                        let _ = self.arq.remover(&id);
                    }
                    *self.progresso.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
            }
        }
    }

    fn verificar_atualizacoes(&self) {
        let prazo = self.limites.atualizar_horas.max(1) * 3600;
        for s in self.arq.sites() {
            if s.tipo == "web" && agora().saturating_sub(s.verificado) >= prazo {
                self.enfileirar(&s.id);
            }
        }
    }
}

pub fn iniciar(g: Arc<Gerente>) {
    thread::spawn(move || {
        let mut voltas: u64 = 0;
        loop {
            if voltas % 6 == 0 {
                g.arq.importar();
            }
            if voltas % 120 == 0 && g.curl.disponivel {
                g.verificar_atualizacoes();
            }
            g.trabalhar();
            voltas += 1;
            thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Monta o endereço original de uma chave guardada (para o link "ver online").
pub fn original(site: &Site, chave: &str) -> Option<String> {
    let esquema = Url::ler(&site.url).map(|u| u.esquema).unwrap_or_else(|| "https".into());
    (site.tipo == "web").then(|| format!("{esquema}://{chave}"))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn bloqueia_rede_interna() {
        for ip in ["127.0.0.1", "10.0.0.1", "192.168.1.1", "172.16.5.4", "169.254.169.254", "100.64.0.1", "0.0.0.0", "224.0.0.1"] {
            assert!(!ip_publico(ip.parse::<Ipv4Addr>().unwrap().into()), "{ip}");
        }
        for ip in ["::1", "fd00::1", "fe80::1", "::ffff:192.168.0.1", "64:ff9b::a00:1"] {
            assert!(!ip_publico(ip.parse::<Ipv6Addr>().unwrap().into()), "{ip}");
        }
        assert!(ip_publico("8.8.4.4".parse::<Ipv4Addr>().unwrap().into()));
        assert!(ip_publico("2606:4700::1111".parse::<Ipv6Addr>().unwrap().into()));
        let u = Url::ler("http://127.0.0.1:9/").unwrap();
        assert!(resolver(&u, false).is_err());
        let u = Url::ler("http://localhost/").unwrap();
        assert!(resolver(&u, false).is_err());
    }

    #[test]
    fn robots() {
        let r = Robots::ler("User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /privado\nAllow: /privado/livre\nDisallow: /*.pdf$\n");
        assert!(r.permite("/"));
        assert!(!r.permite("/privado/x"));
        assert!(r.permite("/privado/livre/y"));
        assert!(r.permite("/doc.pdf.html"));
        assert!(!r.permite("/docs/a.pdf"));
        assert!(Robots::ler("").permite("/qualquer"));
        let todo = Robots::ler("User-agent: *\nDisallow: /\n");
        assert!(!todo.permite("/"));
    }

    #[test]
    fn ids() {
        let id = id_para(&Url::ler("https://Exemplo.com.br/a").unwrap());
        assert!(id.starts_with("exemplo.com.br-") && crate::arquivo::id_valido(&id));
    }
}
