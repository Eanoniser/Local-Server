// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Textos em 20 idiomas (um arquivo por idioma em src/idiomas, embutido no programa).
//! Páginas: idioma do cabeçalho Accept-Language que o navegador já manda (ou a
//! escolha feita no menu de idiomas, guardada num cookie só deste servidor).
//! Janela: idioma do sistema operacional. Nada sai do computador.
//! Se faltar um texto num idioma, aparece o inglês.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

/// (código curto, código para <html lang>, nome no próprio idioma, escrita da direita para a esquerda, textos)
pub const LINGUAS: &[(&str, &str, &str, bool, &str)] = &[
    ("pt", "pt-BR", "Português", false, include_str!("idiomas/pt.txt")),
    ("en", "en", "English", false, include_str!("idiomas/en.txt")),
    ("es", "es", "Español", false, include_str!("idiomas/es.txt")),
    ("fr", "fr", "Français", false, include_str!("idiomas/fr.txt")),
    ("de", "de", "Deutsch", false, include_str!("idiomas/de.txt")),
    ("it", "it", "Italiano", false, include_str!("idiomas/it.txt")),
    ("nl", "nl", "Nederlands", false, include_str!("idiomas/nl.txt")),
    ("pl", "pl", "Polski", false, include_str!("idiomas/pl.txt")),
    ("tr", "tr", "Türkçe", false, include_str!("idiomas/tr.txt")),
    ("ru", "ru", "Русский", false, include_str!("idiomas/ru.txt")),
    ("uk", "uk", "Українська", false, include_str!("idiomas/uk.txt")),
    ("ar", "ar", "العربية", true, include_str!("idiomas/ar.txt")),
    ("hi", "hi", "हिन्दी", false, include_str!("idiomas/hi.txt")),
    ("bn", "bn", "বাংলা", false, include_str!("idiomas/bn.txt")),
    ("zh", "zh-Hans", "中文", false, include_str!("idiomas/zh.txt")),
    ("ja", "ja", "日本語", false, include_str!("idiomas/ja.txt")),
    ("ko", "ko", "한국어", false, include_str!("idiomas/ko.txt")),
    ("id", "id", "Bahasa Indonesia", false, include_str!("idiomas/id.txt")),
    ("vi", "vi", "Tiếng Việt", false, include_str!("idiomas/vi.txt")),
    ("sw", "sw", "Kiswahili", false, include_str!("idiomas/sw.txt")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lingua(usize);

impl Lingua {
    pub const PT: Lingua = Lingua(0);
    pub const EN: Lingua = Lingua(1);

    pub fn curto(self) -> &'static str {
        LINGUAS[self.0].0
    }
    pub fn codigo(self) -> &'static str {
        LINGUAS[self.0].1
    }
    pub fn direita_para_esquerda(self) -> bool {
        LINGUAS[self.0].3
    }
    pub fn todas() -> impl Iterator<Item = (Lingua, &'static str)> {
        LINGUAS.iter().enumerate().map(|(i, l)| (Lingua(i), l.2))
    }
}

thread_local! {
    static ATUAL: Cell<Lingua> = const { Cell::new(Lingua::EN) };
}

pub fn definir(l: Lingua) {
    ATUAL.with(|a| a.set(l));
}

pub fn atual() -> Lingua {
    ATUAL.with(Cell::get)
}

/// "pt-BR", "ja_JP.UTF-8", "zh-Hant" → idioma conhecido (ou nenhum).
pub fn de_codigo(c: &str) -> Option<Lingua> {
    let c = c.trim().to_ascii_lowercase();
    let base = c.split(['-', '_', '.', '@']).next().unwrap_or("");
    let base = if base == "in" { "id" } else { base }; // código antigo do indonésio
    LINGUAS.iter().position(|l| l.0 == base).map(Lingua)
}

/// Escolha guardada (cookie) vence; senão, o primeiro idioma conhecido do navegador; senão inglês.
pub fn do_pedido(accept_language: Option<&str>, cookie: Option<&str>) -> Lingua {
    if let Some(c) = cookie {
        for par in c.split(';') {
            if let Some(v) = par.trim().strip_prefix("lingua=") {
                if let Some(l) = de_codigo(v) {
                    return l;
                }
            }
        }
    }
    if let Some(a) = accept_language {
        for parte in a.split(',') {
            if let Some(l) = de_codigo(parte.split(';').next().unwrap_or("")) {
                return l;
            }
        }
    }
    Lingua::EN
}

/// Idioma do sistema: LANG/LC_* (Linux, Mac) ou a configuração regional do Windows/Mac.
pub fn do_sistema() -> Lingua {
    for v in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(x) = std::env::var(v) {
            for parte in x.split(':') {
                if let Some(l) = de_codigo(parte) {
                    return l;
                }
            }
        }
    }
    if cfg!(target_os = "windows") {
        if let Ok(s) = std::process::Command::new("reg").args(["query", r"HKCU\Control Panel\International", "/v", "LocaleName"]).output() {
            // linha: "    LocaleName    REG_SZ    ja-JP"
            let t = String::from_utf8_lossy(&s.stdout);
            for linha in t.lines() {
                if linha.contains("LocaleName") {
                    if let Some(l) = linha.split_whitespace().last().and_then(de_codigo) {
                        return l;
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(s) = std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output() {
            if let Some(l) = de_codigo(&String::from_utf8_lossy(&s.stdout)) {
                return l;
            }
        }
    }
    Lingua::EN
}

fn ler(texto: &'static str) -> HashMap<&'static str, &'static str> {
    texto
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(" = "))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect()
}

fn tabelas() -> &'static Vec<HashMap<&'static str, &'static str>> {
    static T: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();
    T.get_or_init(|| LINGUAS.iter().map(|l| ler(l.4)).collect())
}

pub fn t(chave: &str) -> &'static str {
    let tab = tabelas();
    tab[atual().0].get(chave).or_else(|| tab[Lingua::EN.0].get(chave)).copied().unwrap_or("")
}

/// Avisos da página inicial, montados em lista.
pub fn avisos() -> String {
    let mut s = String::from("<ul>");
    for k in ["av-pc", "av-win7", "av-cel", "av-login", "av-js", "av-imagens", "av-sem-scripts", "av-robots", "av-resp"] {
        s.push_str("<li>");
        s.push_str(t(k));
        s.push_str("</li>");
    }
    s.push_str("</ul>");
    s
}

/// Texto "o que é isto".
pub fn sobre() -> String {
    format!(
        "<dl><dt>Cloudflare</dt><dd>{}</dd><dt>Local Server</dt><dd>{}</dd></dl><p>{}</p>",
        t("sobre-cf"),
        t("sobre-ls"),
        t("sobre-2")
    )
}

/// Erros do download (gravados em português) mostrados no idioma da página.
pub fn erro(msg: &str) -> String {
    if atual() == Lingua::PT {
        return msg.to_string();
    }
    const MAPA: &[(&str, &str)] = &[
        ("este sistema não tem curl", "e-curl"),
        ("endereço inválido", "e-invalido"),
        ("o site pede (robots.txt)", "e-robots"),
        ("nada foi baixado", "e-nada"),
        ("endereço da rede interna bloqueado", "e-interna"),
        ("não encontrei o endereço", "e-dns"),
        ("o site respondeu", "e-http"),
        ("redirecionamentos demais", "e-redir-muitos"),
        ("redirecionamento inválido", "e-redir-invalido"),
        ("falha ao baixar", "e-falha"),
        ("erro interno", "e-interno"),
    ];
    let chave = MAPA.iter().find(|(pt, _)| msg.starts_with(pt)).map(|(_, k)| *k).unwrap_or("e-falha");
    t(chave).to_string()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn marcas(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s.match_indices('{').map(|(i, _)| s[i..].split('}').next().unwrap_or("").to_string()).collect();
        v.sort();
        v
    }

    fn tags(s: &str) -> usize {
        s.matches('<').count()
    }

    #[test]
    fn todos_os_idiomas_completos() {
        let tab = tabelas();
        let en = &tab[Lingua::EN.0];
        assert!(en.len() > 80);
        for (i, l) in LINGUAS.iter().enumerate() {
            let mut chaves = std::collections::HashSet::new();
            for linha in l.4.lines().filter(|x| !x.trim_start().starts_with('#') && x.contains(" = ")) {
                let k = linha.split_once(" = ").unwrap().0.trim();
                assert!(chaves.insert(k), "{}: chave repetida {k}", l.0);
            }
            for (k, v) in en {
                let x = tab[i].get(k).unwrap_or_else(|| panic!("{}: falta {k}", l.0));
                assert!(!x.is_empty(), "{}: vazio {k}", l.0);
                assert_eq!(marcas(x), marcas(v), "{}: marcadores de {k}", l.0);
                assert_eq!(tags(x), tags(v), "{}: tags html de {k}", l.0);
            }
            assert_eq!(tab[i].len(), en.len(), "{}: chaves a mais", l.0);
        }
    }

    #[test]
    fn escolhe_idioma() {
        assert_eq!(do_pedido(Some("pt-BR,pt;q=0.9,en;q=0.8"), None), Lingua::PT);
        assert_eq!(do_pedido(Some("de-DE,de;q=0.9,en;q=0.5"), None).curto(), "de");
        assert_eq!(do_pedido(Some("xx-YY,es-ES;q=0.5"), None).curto(), "es");
        assert_eq!(do_pedido(Some("zh-TW"), None).curto(), "zh");
        assert_eq!(do_pedido(Some("eo"), None), Lingua::EN);
        assert_eq!(do_pedido(Some("pt-BR"), Some("x=1; lingua=ja")).curto(), "ja");
        assert_eq!(do_pedido(None, None), Lingua::EN);
        assert_eq!(de_codigo("ja_JP.UTF-8").map(Lingua::curto), Some("ja"));
        assert_eq!(de_codigo("C"), None);
        definir(Lingua::EN);
        assert_eq!(t("abrir"), "open");
        assert_eq!(erro("endereço da rede interna bloqueado"), "local network address blocked");
        definir(Lingua::PT);
        assert_eq!(t("abrir"), "abrir");
        definir(de_codigo("ar").unwrap());
        assert!(atual().direita_para_esquerda());
        definir(Lingua::EN);
    }
}
