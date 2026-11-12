// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Reescreve HTML e CSS guardados para que os links apontem para a cópia local.
//! Também tira scripts (as cópias abrem sem scripts, por segurança) e
//! revela o conteúdo de <noscript> e imagens "preguiçosas" (data-src).

use crate::url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tipo {
    Pagina,
    Recurso,
}

/// Decide como cada link vira texto no arquivo guardado e anota o que baixar.
pub trait Mapa {
    /// Devolve o novo valor do link (local ou endereço original).
    fn mapear(&mut self, alvo: &Url, tipo: Tipo) -> String;
}

/// Caminho relativo da página atual até a raiz da versão (ex.: "../../").
pub fn ate_raiz(chave: &str) -> String {
    let caminho = chave.split('?').next().unwrap_or(chave);
    "../".repeat(caminho.matches('/').count())
}

/// Escapa o que não pode ficar dentro de um atributo entre aspas.
pub fn para_atributo(t: &str) -> String {
    t.replace('&', "&amp;").replace('"', "%22").replace('<', "%3C").replace('>', "%3E").replace(' ', "%20")
}

struct Tag {
    nome: String,
    fechamento: bool,
    attrs: Vec<(String, Option<String>)>,
    auto: bool,
}

fn ler_tag(t: &str) -> Option<(Tag, usize)> {
    // t começa com '<'
    let b = t.as_bytes();
    let mut i = 1;
    let fechamento = b.get(1) == Some(&b'/');
    if fechamento {
        i += 1;
    }
    let ini = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b':') {
        i += 1;
    }
    if i == ini {
        return None;
    }
    let nome = t[ini..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut auto = false;
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            if b[i] == b'/' {
                auto = true;
            }
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        if b[i] == b'>' {
            return Some((Tag { nome, fechamento, attrs, auto }, i + 1));
        }
        auto = false;
        let ini = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let nome_attr = t[ini..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let valor = if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let ini = i + 1;
                let fim = ini + t[ini..].bytes().position(|c| c == q)?;
                i = fim + 1;
                &t[ini..fim]
            } else {
                let ini = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                &t[ini..i]
            };
            attrs.push((nome_attr, Some(crate::arquivo::decodificar_entidades(valor))));
        } else if !nome_attr.is_empty() {
            attrs.push((nome_attr, None));
        }
    }
}

fn escrever_tag(tag: &Tag) -> String {
    if tag.fechamento {
        return format!("</{}>", tag.nome);
    }
    let mut s = format!("<{}", tag.nome);
    for (n, v) in &tag.attrs {
        match v {
            Some(v) => s.push_str(&format!(" {n}=\"{}\"", v.replace('&', "&amp;").replace('"', "&quot;"))),
            None => s.push_str(&format!(" {n}")),
        }
    }
    if tag.auto {
        s.push_str(" /");
    }
    s.push('>');
    s
}

fn achar(t: &str, de: usize, agulha: &str) -> Option<usize> {
    let minus = t[de..].to_ascii_lowercase();
    minus.find(agulha).map(|p| de + p)
}

fn reescrever_srcset(v: &str, base: &Url, mapa: &mut dyn Mapa) -> String {
    v.split(',')
        .filter_map(|parte| {
            let parte = parte.trim();
            let (u, desc) = parte.split_once(char::is_whitespace).unwrap_or((parte, ""));
            let alvo = base.juntar(u)?;
            Some(format!("{} {}", mapa.mapear(&alvo, Tipo::Recurso), desc.trim()).trim().to_string())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn reescrever_html(html: &str, base: &Url, mapa: &mut dyn Mapa) -> String {
    let mut s = String::with_capacity(html.len() + 1024);
    let mut i = 0;
    while i < html.len() {
        let Some(p) = html[i..].find('<') else {
            s.push_str(&html[i..]);
            break;
        };
        s.push_str(&html[i..i + p]);
        i += p;
        let resto = &html[i..];
        if resto.starts_with("<!--") {
            let fim = resto.find("-->").map(|f| f + 3).unwrap_or(resto.len());
            i += fim; // comentários são descartados
            continue;
        }
        if resto.starts_with("<!") || resto.starts_with("<?") {
            // <!doctype html> e parecidos passam como estão
            let fim = resto.find('>').map(|f| f + 1).unwrap_or(resto.len());
            s.push_str(&resto[..fim]);
            i += fim;
            continue;
        }
        let Some((mut tag, tam)) = ler_tag(resto) else {
            s.push_str("&lt;");
            i += 1;
            continue;
        };
        i += tam;
        match (tag.nome.as_str(), tag.fechamento) {
            ("script", false) => {
                // scripts não rodam nas cópias: tira tudo
                i = achar(html, i, "</script").and_then(|f| html[f..].find('>').map(|g| f + g + 1)).unwrap_or(html.len());
                continue;
            }
            ("noscript", _) | ("base", _) => continue,
            ("meta", false) if tag.attrs.iter().any(|(n, v)| n == "http-equiv" && v.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("refresh"))) => continue,
            ("style", false) => {
                s.push_str(&escrever_tag(&tag));
                let fim = achar(html, i, "</style").unwrap_or(html.len());
                s.push_str(&reescrever_css(&html[i..fim], base, mapa));
                i = fim;
                continue;
            }
            _ => {}
        }
        if !tag.fechamento {
            ajustar_attrs(&mut tag, base, mapa);
        }
        s.push_str(&escrever_tag(&tag));
    }
    s
}

fn ajustar_attrs(tag: &mut Tag, base: &Url, mapa: &mut dyn Mapa) {
    let nome = tag.nome.clone();
    let rel = tag.attrs.iter().find(|(n, _)| n == "rel").and_then(|(_, v)| v.clone()).unwrap_or_default().to_ascii_lowercase();
    // imagens "preguiçosas": usa data-src como src
    for (de, para) in [("data-src", "src"), ("data-srcset", "srcset")] {
        if let Some(v) = tag.attrs.iter().find(|(n, _)| n == de).and_then(|(_, v)| v.clone()) {
            match tag.attrs.iter_mut().find(|(n, _)| n == para) {
                Some((_, atual)) if atual.as_deref().is_some_and(|a| !a.trim_start().starts_with("data:") && !a.trim().is_empty()) => {}
                Some((_, atual)) => *atual = Some(v),
                None => tag.attrs.push((para.to_string(), Some(v))),
            }
        }
    }
    let mut novos = Vec::new();
    for (n, v) in tag.attrs.drain(..) {
        if n.starts_with("on") || n == "integrity" || n == "nonce" || n == "data-src" || n == "data-srcset" {
            continue; // eventos (scripts) e travas que quebram com a cópia local
        }
        let Some(v) = v else {
            novos.push((n, None));
            continue;
        };
        let novo = match (nome.as_str(), n.as_str()) {
            ("a" | "area", "href") => base.juntar(&v).map(|u| mapa.mapear(&u, Tipo::Pagina)),
            ("link", "href") if rel.contains("stylesheet") || rel.contains("icon") => base.juntar(&v).map(|u| mapa.mapear(&u, Tipo::Recurso)),
            ("link", "href") | ("form", "action") | ("iframe", "src") | ("frame", "src") | ("embed", "src") | ("object", "data") => {
                base.juntar(&v).map(|u| u.texto())
            }
            ("img" | "source" | "video" | "audio" | "track" | "input", "src") | ("video", "poster") | ("body" | "table" | "td", "background") => {
                base.juntar(&v).map(|u| mapa.mapear(&u, Tipo::Recurso))
            }
            (_, "srcset") => Some(reescrever_srcset(&v, base, mapa)),
            (_, "style") => Some(reescrever_css(&v, base, mapa)),
            _ => None,
        };
        novos.push((n, Some(novo.unwrap_or(v))));
    }
    tag.attrs = novos;
}

/// Reescreve url(...) e @import dentro de CSS.
pub fn reescrever_css(css: &str, base: &Url, mapa: &mut dyn Mapa) -> String {
    let mut s = String::with_capacity(css.len());
    let mut i = 0;
    let minus = css.to_ascii_lowercase();
    while i < css.len() {
        let prox_url = minus[i..].find("url(").map(|p| (p + i, true));
        let prox_imp = minus[i..].find("@import").map(|p| (p + i, false));
        let prox = match (prox_url, prox_imp) {
            (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
            (a, b) => a.or(b),
        };
        let Some((p, eh_url)) = prox else {
            s.push_str(&css[i..]);
            break;
        };
        if eh_url {
            s.push_str(&css[i..p + 4]);
            let ini = p + 4;
            let Some(fim) = css[ini..].find(')').map(|f| ini + f) else {
                s.push_str(&css[ini..]);
                break;
            };
            let bruto = css[ini..fim].trim().trim_matches(['"', '\'']);
            match base.juntar(bruto) {
                Some(u) if !bruto.starts_with("data:") => s.push_str(&format!("\"{}\"", mapa.mapear(&u, Tipo::Recurso).replace('"', "%22"))),
                _ => s.push_str(&css[ini..fim]),
            }
            s.push(')');
            i = fim + 1;
        } else {
            s.push_str("@import");
            let mut j = p + 7;
            while j < css.len() && css.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
            let q = css.as_bytes().get(j).copied();
            if q == Some(b'"') || q == Some(b'\'') {
                let ini = j + 1;
                if let Some(fim) = css[ini..].find(q.unwrap() as char).map(|f| ini + f) {
                    let novo = base.juntar(&css[ini..fim]).map(|u| mapa.mapear(&u, Tipo::Recurso)).unwrap_or_else(|| css[ini..fim].to_string());
                    s.push_str(&format!(" \"{}\"", novo.replace('"', "%22")));
                    i = fim + 1;
                    continue;
                }
            }
            i = p + 7;
        }
    }
    s
}

#[cfg(test)]
mod testes {
    use super::*;

    struct Teste {
        origem: String,
        vistos: Vec<(String, Tipo)>,
    }
    impl Mapa for Teste {
        fn mapear(&mut self, alvo: &Url, tipo: Tipo) -> String {
            self.vistos.push((alvo.texto(), tipo));
            if tipo == Tipo::Recurso || alvo.origem() == self.origem {
                format!("LOCAL/{}", alvo.chave())
            } else {
                alvo.texto()
            }
        }
    }

    #[test]
    fn reescreve_pagina() {
        let base = Url::ler("https://ex.com/blog/post.html").unwrap();
        let mut m = Teste { origem: base.origem(), vistos: vec![] };
        let html = r#"<!doctype html><html><head><base href="/x/"><script>alert(1)</script><link rel="stylesheet" href="../css/a.css" integrity="sha-x"><style>body{background:url('/img/f.png')}</style></head>
<body onload="mal()"><a href="outra.html">x</a> <a href="https://fora.org/">f</a> <img data-src="/i.jpg" src="data:image/gif;base64,AA" srcset="a.png 1x, b.png 2x"><!-- c --><noscript><img src="n.png"></noscript><p style="background:url(z.png)">t</p></body></html>"#;
        let r = reescrever_html(html, &base, &mut m);
        assert!(!r.contains("<script") && !r.contains("alert") && !r.contains("onload") && !r.contains("<base") && !r.contains("integrity"));
        assert!(r.contains(r#"href="LOCAL/ex.com/css/a.css""#), "{r}");
        assert!(r.contains("url(\"LOCAL/ex.com/img/f.png\")"));
        assert!(r.contains(r#"href="LOCAL/ex.com/blog/outra.html""#));
        assert!(r.contains(r#"href="https://fora.org/""#));
        assert!(r.contains(r#"src="LOCAL/ex.com/i.jpg""#));
        assert!(r.contains("LOCAL/ex.com/blog/a.png 1x, LOCAL/ex.com/blog/b.png 2x"));
        assert!(r.contains(r#"<img src="LOCAL/ex.com/blog/n.png">"#));
        assert!(r.contains("url(&quot;LOCAL/ex.com/blog/z.png&quot;)"), "{r}"); // aspas viram &quot; dentro do atributo
        assert!(!r.contains("<!--"));
        assert!(r.starts_with("<!doctype html><html>") && !r.contains("&lt;!doctype"));
    }

    #[test]
    fn caminho_ate_raiz() {
        assert_eq!(ate_raiz("ex.com/"), "../");
        assert_eq!(ate_raiz("ex.com/a/b.html?x=/y"), "../../");
        assert_eq!(para_atributo("a b\"<"), "a%20b%22%3C");
    }

    #[test]
    fn html_quebrado_nao_trava() {
        let base = Url::ler("https://ex.com/").unwrap();
        let mut m = Teste { origem: base.origem(), vistos: vec![] };
        for lixo in ["<", "<a href=\"x", "<<<>>>", "<script>sem fim", "<!-- aberto", "<img src='x' / >", "a < b > c"] {
            let _ = reescrever_html(lixo, &base, &mut m);
        }
    }
}
