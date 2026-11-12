// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Site archive: content-addressed storage (each file is
//! stored only once, even if it appears in many versions), dated
//! versions, and import of saved pages (Ctrl+S) and PDFs.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::gzip;
use crate::sha256::sha256_hex;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Site {
    pub id: String,
    pub url: String,
    pub titulo: String,
    pub tipo: String, // "web", "pagina", "documento"
    pub criado: u64,
    pub verificado: u64,
    pub erro: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub hash: String,
    pub tipo: String,
    pub gz: bool,
    pub tamanho: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Versao {
    pub inicio: String,
    pub itens: HashMap<String, Item>,
}

pub struct Arquivo {
    pub raiz: PathBuf,
}

/// Text types that shrink well without loss.
pub fn comprimivel(tipo: &str) -> bool {
    let t = tipo.split(';').next().unwrap_or("").trim();
    t.starts_with("text/") || matches!(t, "application/javascript" | "application/json" | "image/svg+xml" | "application/xml" | "application/xhtml+xml" | "application/manifest+json")
}

pub fn id_valido(t: &str) -> bool {
    !t.is_empty() && t.len() <= 80 && !t.starts_with('.') && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
}

fn limpar_campo(t: &str) -> String {
    t.chars().map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }).collect::<String>().trim().chars().take(500).collect()
}

impl Arquivo {
    pub fn novo(raiz: PathBuf) -> io::Result<Arquivo> {
        for sub in ["objetos", "sites", "importar", "tmp"] {
            fs::create_dir_all(raiz.join(sub))?;
        }
        Ok(Arquivo { raiz })
    }

    pub fn tmp(&self) -> PathBuf {
        self.raiz.join("tmp")
    }

    fn caminho_objeto(&self, hash: &str, gz: bool) -> PathBuf {
        let mut p = self.raiz.join("objetos").join(&hash[..2]);
        p.push(if gz { format!("{hash}.gz") } else { hash.to_string() });
        p
    }

    /// Stores the content and returns the item. If an identical one exists, it is not written again.
    pub fn guardar(&self, dados: &[u8], tipo: &str) -> io::Result<Item> {
        // photos: without hidden data (camera, GPS, date); no pixel changes
        let limpo;
        let dados = if tipo.trim_start().to_ascii_lowercase().starts_with("image/") {
            limpo = crate::imagens::limpar(dados, tipo);
            &limpo[..]
        } else {
            dados
        };
        let hash = sha256_hex(dados);
        let gz = comprimivel(tipo) && dados.len() > 256;
        let destino = self.caminho_objeto(&hash, gz);
        if !destino.exists() {
            fs::create_dir_all(destino.parent().unwrap())?;
            let conteudo = if gz { gzip::gzip(dados) } else { dados.to_vec() };
            let temporario = destino.with_extension("parcial");
            fs::write(&temporario, conteudo)?;
            fs::rename(&temporario, &destino)?;
        }
        Ok(Item { hash, tipo: limpar_campo(tipo), gz, tamanho: dados.len() as u64 })
    }

    pub fn ler_objeto(&self, item: &Item) -> io::Result<Vec<u8>> {
        if item.hash.len() != 64 || !item.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::other("hash inválido"));
        }
        fs::read(self.caminho_objeto(&item.hash, item.gz))
    }

    fn pasta_site(&self, id: &str) -> Option<PathBuf> {
        id_valido(id).then(|| self.raiz.join("sites").join(id))
    }

    pub fn salvar_site(&self, s: &Site) -> io::Result<()> {
        let pasta = self.pasta_site(&s.id).ok_or_else(|| io::Error::other("id inválido"))?;
        fs::create_dir_all(&pasta)?;
        let texto = format!(
            "url={}\ntitulo={}\ntipo={}\ncriado={}\nverificado={}\nerro={}\n",
            limpar_campo(&s.url),
            limpar_campo(&s.titulo),
            limpar_campo(&s.tipo),
            s.criado,
            s.verificado,
            limpar_campo(&s.erro)
        );
        let tmp = pasta.join("info.parcial");
        fs::write(&tmp, texto)?;
        fs::rename(tmp, pasta.join("info.txt"))
    }

    pub fn site(&self, id: &str) -> Option<Site> {
        let texto = fs::read_to_string(self.pasta_site(id)?.join("info.txt")).ok()?;
        let mut s = Site { id: id.to_string(), ..Default::default() };
        for l in texto.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            match k {
                "url" => s.url = v.to_string(),
                "titulo" => s.titulo = v.to_string(),
                "tipo" => s.tipo = v.to_string(),
                "criado" => s.criado = v.parse().unwrap_or(0),
                "verificado" => s.verificado = v.parse().unwrap_or(0),
                "erro" => s.erro = v.to_string(),
                _ => {}
            }
        }
        Some(s)
    }

    pub fn sites(&self) -> Vec<Site> {
        let mut v: Vec<Site> = fs::read_dir(self.raiz.join("sites"))
            .map(|it| it.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).filter_map(|n| self.site(&n)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| b.criado.cmp(&a.criado));
        v
    }

    /// Site versions, newest first.
    pub fn versoes(&self, id: &str) -> Vec<String> {
        let Some(p) = self.pasta_site(id) else { return Vec::new() };
        let mut v: Vec<String> = fs::read_dir(p)
            .map(|it| {
                it.filter_map(Result::ok)
                    .filter(|e| e.path().join("manifesto.txt").is_file())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.len() == 15 && n.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.cmp(a));
        v
    }

    pub fn versao(&self, id: &str, ts: &str) -> Option<Versao> {
        if ts.len() != 15 || !ts.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
            return None;
        }
        let texto = fs::read_to_string(self.pasta_site(id)?.join(ts).join("manifesto.txt")).ok()?;
        let mut v = Versao::default();
        for l in texto.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() == 2 && c[0] == "inicio" {
                v.inicio = c[1].to_string();
            } else if c.len() == 5 {
                v.itens.insert(
                    c[0].to_string(),
                    Item { hash: c[1].to_string(), tipo: c[2].to_string(), gz: c[3] == "1", tamanho: c[4].parse().unwrap_or(0) },
                );
            }
        }
        Some(v)
    }

    /// Writes a new version unless it is identical to the last one. Returns whether it wrote.
    pub fn gravar_versao(&self, id: &str, versao: &Versao) -> io::Result<bool> {
        if let Some(ultima) = self.versoes(id).first().and_then(|ts| self.versao(id, ts)) {
            let iguais = ultima.inicio == versao.inicio
                && ultima.itens.len() == versao.itens.len()
                && ultima.itens.iter().all(|(k, i)| versao.itens.get(k).is_some_and(|j| j.hash == i.hash));
            if iguais {
                return Ok(false);
            }
        }
        let pasta = self.pasta_site(id).ok_or_else(|| io::Error::other("id inválido"))?.join(carimbo(agora()));
        fs::create_dir_all(&pasta)?;
        let mut texto = format!("inicio\t{}\n", limpar_campo(&versao.inicio));
        let mut chaves: Vec<&String> = versao.itens.keys().collect();
        chaves.sort();
        for k in chaves {
            let i = &versao.itens[k];
            texto.push_str(&format!("{}\t{}\t{}\t{}\t{}\n", limpar_campo(k), i.hash, i.tipo, if i.gz { 1 } else { 0 }, i.tamanho));
        }
        let tmp = pasta.join("manifesto.parcial");
        fs::write(&tmp, texto)?;
        fs::rename(tmp, pasta.join("manifesto.txt"))?;
        Ok(true)
    }

    /// Deletes the site and the files no other site uses any more.
    pub fn remover(&self, id: &str) -> io::Result<()> {
        if let Some(p) = self.pasta_site(id) {
            if p.is_dir() {
                fs::remove_dir_all(p)?;
            }
        }
        self.limpar_orfaos()
    }

    pub fn limpar_orfaos(&self) -> io::Result<()> {
        let mut usados = HashSet::new();
        for s in self.sites() {
            for ts in self.versoes(&s.id) {
                if let Some(v) = self.versao(&s.id, &ts) {
                    usados.extend(v.itens.values().map(|i| i.hash.clone()));
                }
            }
        }
        for sub in fs::read_dir(self.raiz.join("objetos"))?.filter_map(Result::ok) {
            for f in fs::read_dir(sub.path())?.filter_map(Result::ok) {
                let nome = f.file_name().to_string_lossy().to_string();
                let hash = nome.trim_end_matches(".gz");
                if !usados.contains(hash) {
                    let _ = fs::remove_file(f.path());
                }
            }
        }
        Ok(())
    }

    pub fn tamanho_total(&self) -> u64 {
        fn somar(p: &Path) -> u64 {
            fs::read_dir(p)
                .map(|it| {
                    it.filter_map(Result::ok)
                        .map(|e| {
                            let m = e.metadata();
                            match m {
                                Ok(m) if m.is_dir() => somar(&e.path()),
                                Ok(m) => m.len(),
                                Err(_) => 0,
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        somar(&self.raiz.join("objetos"))
    }

    // ---------------- Import (Ctrl+S and PDF) ----------------

    /// Reads the importar/ folder. Each .pdf or .html (with its files folder)
    /// becomes a list item. The originals go to importar/importados/.
    pub fn importar(&self) -> Vec<String> {
        let pasta = self.raiz.join("importar");
        let mut novos = Vec::new();
        let Ok(it) = fs::read_dir(&pasta) else { return novos };
        let entradas: Vec<PathBuf> = it.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_file()).collect();
        for arq in entradas {
            let nome = arq.file_name().unwrap_or_default().to_string_lossy().to_string();
            let ext = arq.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let r = match ext.as_str() {
                "pdf" => self.importar_um(&arq, &nome, "documento", "application/pdf", &[]),
                "html" | "htm" => {
                    let raiz_nome = arq.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let pastas: Vec<PathBuf> = fs::read_dir(&pasta)
                        .map(|it| {
                            it.filter_map(Result::ok)
                                .map(|e| e.path())
                                .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{raiz_nome}_"))))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.importar_um(&arq, &nome, "pagina", "text/html; charset=utf-8", &pastas)
                }
                _ => continue,
            };
            if let Ok(id) = r {
                novos.push(id);
            }
        }
        novos
    }

    fn importar_um(&self, arq: &Path, nome: &str, tipo_site: &str, tipo: &str, pastas: &[PathBuf]) -> io::Result<String> {
        let dados = fs::read(arq)?;
        let mut versao = Versao { inicio: format!("importado/{nome}"), ..Default::default() };
        let titulo = if tipo_site == "pagina" { titulo_html(&dados).unwrap_or_else(|| nome.to_string()) } else { nome.to_string() };
        versao.itens.insert(versao.inicio.clone(), self.guardar(&dados, tipo)?);
        for p in pastas {
            let base = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            for (rel, caminho) in listar_recursivo(p, 4) {
                let d = fs::read(&caminho)?;
                let t = crate::pagina::tipo_do_arquivo(&caminho);
                versao.itens.insert(format!("importado/{base}/{rel}"), self.guardar(&d, t)?);
            }
        }
        let t = agora();
        let id = format!("importado-{}", &sha256_hex(format!("{nome}{t}").as_bytes())[..10]);
        self.salvar_site(&Site { id: id.clone(), url: String::new(), titulo, tipo: tipo_site.into(), criado: t, verificado: t, erro: String::new() })?;
        self.gravar_versao(&id, &versao)?;
        // keeps the originals in importados/ (never deletes the user's files)
        let destino = self.raiz.join("importar").join("importados").join(carimbo(t));
        fs::create_dir_all(&destino)?;
        fs::rename(arq, destino.join(nome))?;
        for p in pastas {
            let _ = fs::rename(p, destino.join(p.file_name().unwrap_or_default()));
        }
        Ok(id)
    }
}

fn listar_recursivo(p: &Path, profundidade: usize) -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    if profundidade == 0 {
        return v;
    }
    for e in fs::read_dir(p).into_iter().flatten().filter_map(Result::ok) {
        let caminho = e.path();
        let nome = e.file_name().to_string_lossy().to_string();
        if caminho.is_dir() {
            for (r, c) in listar_recursivo(&caminho, profundidade - 1) {
                v.push((format!("{nome}/{r}"), c));
            }
        } else if caminho.is_file() {
            v.push((nome, caminho));
        }
    }
    v
}

pub fn titulo_html(dados: &[u8]) -> Option<String> {
    let t = String::from_utf8_lossy(&dados[..dados.len().min(200_000)]);
    let minus = t.to_ascii_lowercase();
    let ini = minus.find("<title")?;
    let ini = ini + minus[ini..].find('>')? + 1;
    let fim = ini + minus[ini..].find("</title")?;
    let titulo = decodificar_entidades(t[ini..fim].trim());
    (!titulo.is_empty()).then(|| titulo.chars().take(200).collect())
}

pub fn decodificar_entidades(t: &str) -> String {
    t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#039;", "'").replace("&nbsp;", " ")
}

// ---------------- Dates (no libraries) ----------------

pub fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// (year, month, day, hour, minute, second) in UTC.
pub fn data(seg: u64) -> (i64, u32, u32, u32, u32, u32) {
    let dias = (seg / 86_400) as i64;
    let resto = seg % 86_400;
    // "civil_from_days" algorithm (Howard Hinnant)
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let a = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (a, m, d, (resto / 3600) as u32, (resto % 3600 / 60) as u32, (resto % 60) as u32)
}

pub fn carimbo(seg: u64) -> String {
    let (a, m, d, h, mi, s) = data(seg);
    format!("{a:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
}

#[allow(dead_code)] // used in tests
pub fn data_legivel(seg: u64) -> String {
    let (a, m, d, h, mi, _) = data(seg);
    format!("{d:02}/{m:02}/{a:04} {h:02}:{mi:02} UTC")
}

pub fn carimbo_legivel(ts: &str) -> String {
    if ts.len() != 15 || !ts.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return ts.to_string();
    }
    format!("{}/{}/{} {}:{} UTC", &ts[6..8], &ts[4..6], &ts[0..4], &ts[9..11], &ts[11..13])
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas() {
        assert_eq!(carimbo(0), "19700101-000000");
        assert_eq!(carimbo(1_794_255_000), "20261109-201000");
        assert_eq!(data_legivel(951_782_400), "29/02/2000 00:00 UTC");
        assert_eq!(carimbo_legivel("20261109-201000"), "09/11/2026 20:10 UTC");
    }

    #[test]
    fn guarda_sem_duplicar_e_versiona() {
        let raiz = std::env::temp_dir().join(format!("arq-teste-{}", std::process::id()));
        let _ = fs::remove_dir_all(&raiz);
        let a = Arquivo::novo(raiz.clone()).unwrap();
        let html = b"<html><title>Ola &amp; mundo</title><p>texto texto texto texto</p></html>".repeat(20);
        let i1 = a.guardar(&html, "text/html").unwrap();
        let i2 = a.guardar(&html, "text/html").unwrap();
        assert_eq!(i1, i2);
        assert!(i1.gz);
        assert_eq!(a.ler_objeto(&i1).unwrap()[..2], [0x1f, 0x8b]);
        let s = Site { id: "ex.com-1".into(), url: "https://ex.com/".into(), titulo: "T\nx".into(), tipo: "web".into(), criado: 1, verificado: 1, erro: String::new() };
        a.salvar_site(&s).unwrap();
        assert_eq!(a.site("ex.com-1").unwrap().titulo, "T x");
        let mut v = Versao { inicio: "ex.com/".into(), ..Default::default() };
        v.itens.insert("ex.com/".into(), i1.clone());
        assert!(a.gravar_versao("ex.com-1", &v).unwrap());
        assert!(!a.gravar_versao("ex.com-1", &v).unwrap()); // identical: not duplicated
        assert_eq!(a.versoes("ex.com-1").len(), 1);
        assert_eq!(a.versao("ex.com-1", &a.versoes("ex.com-1")[0]).unwrap().itens["ex.com/"], i1);
        assert_eq!(titulo_html(&html).as_deref(), Some("Ola & mundo"));
        assert!(a.site("../x").is_none());
        a.remover("ex.com-1").unwrap();
        assert!(a.sites().is_empty());
        assert_eq!(a.tamanho_total(), 0); // orphan object was deleted
        let _ = fs::remove_dir_all(&raiz);
    }
}
