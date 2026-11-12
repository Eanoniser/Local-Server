// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Server pages: home (sites + archive), zoom viewer,
//! versions, removal confirmation. No page has any script.

use std::fs;
use std::path::Path;

use crate::arquivo::{carimbo_legivel, Site};
use crate::http::segmentos_seguros;
use crate::textos::{self, t};
use crate::Servidor;

/// Server pages: no scripts at all; forms only to the server itself.
pub const CSP_PAGINAS: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:; frame-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'self'";

/// Stored PDFs open in the browser's PDF reader (already isolated).
pub const CSP_PDF: &str = "default-src 'none'; frame-ancestors 'self'";

/// Stored copies: sandbox, no scripts, nothing fetched from outside the server.
pub const CSP_GUARDADO: &str = "sandbox allow-popups allow-popups-to-escape-sandbox; default-src 'self' data:; script-src 'none'; object-src 'none'; style-src 'self' 'unsafe-inline' data:; img-src 'self' data:; font-src 'self' data:; media-src 'self'; frame-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'self'";

pub fn escapar_html(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for ch in t.chars() {
        match ch {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            '\'' => s.push_str("&#39;"),
            c => s.push(c),
        }
    }
    s
}

pub fn codificar_url(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for b in t.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// Encodes a stored key into a path (keeps "/", "?" and "%").
fn caminho_da_chave(chave: &str) -> String {
    let mut s = String::new();
    for b in chave.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/?=&%:+,;!$'()*@".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// Lists the folders inside sites/ (each one is a site).
pub fn listar_sites(raiz: &Path) -> Vec<String> {
    let mut nomes: Vec<String> = fs::read_dir(raiz)
        .map(|it| {
            it.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| segmentos_seguros(&format!("/{n}")).is_some())
                .collect()
        })
        .unwrap_or_default();
    nomes.sort_by_key(|n| n.to_lowercase());
    nomes
}

/// Local Server look: the same theme as the website (orange peel, caterpillar skin,
/// butterfly wing). Light and dark, following the system or the button choice.
/// Textures are embedded in the program (nothing comes from the internet).
const ESTILO: &str = r#"
:root{--fundo:#f6e3c8;--textura:url("@casca@");--sup:#fffaf2;--sup2:#fbefdc;--texto:#2a1d10;--suave:#6f5843;--laranja:#d9621a;--laranja-forte:#a8460b;--folha:#4f7d1f;--jade:#25805b;--borda:#ead2b0;--perigo:#b3361f;--campo:#fff;--sombra:0 1px 0 rgba(255,255,255,.7) inset,0 18px 40px -26px rgba(90,45,10,.55);--p:var(--sup);--le:var(--borda)}
@media (prefers-color-scheme:dark){:root:not([data-tema=claro]){--fundo:#1c2116;--textura:url("@casca-escura@");--sup:#23281b;--sup2:#2a3020;--texto:#f1ead9;--suave:#b4ab92;--laranja:#f08c2c;--laranja-forte:#ffa851;--folha:#a5cf5f;--jade:#57c79a;--borda:#3a4229;--perigo:#ff8a6e;--campo:#1a1e14;--sombra:0 1px 0 rgba(255,255,255,.04) inset,0 18px 40px -24px rgba(0,0,0,.8)}}
:root[data-tema=escuro]{--fundo:#1c2116;--textura:url("@casca-escura@");--sup:#23281b;--sup2:#2a3020;--texto:#f1ead9;--suave:#b4ab92;--laranja:#f08c2c;--laranja-forte:#ffa851;--folha:#a5cf5f;--jade:#57c79a;--borda:#3a4229;--perigo:#ff8a6e;--campo:#1a1e14;--sombra:0 1px 0 rgba(255,255,255,.04) inset,0 18px 40px -24px rgba(0,0,0,.8)}
:root[data-tema=claro]{color-scheme:light} :root[data-tema=escuro]{color-scheme:dark}
*{box-sizing:border-box}
html{scroll-behavior:smooth} @media (prefers-reduced-motion:reduce){html{scroll-behavior:auto}*{transition:none!important}}
body{margin:0;background:var(--fundo) var(--textura) repeat;background-size:260px;color:var(--texto);font:16px/1.6 system-ui,-apple-system,"Segoe UI Variable","Segoe UI",Roboto,Ubuntu,"Noto Sans",sans-serif;-webkit-font-smoothing:antialiased}
main{max-width:1120px;margin:0 auto;padding:0 18px 32px}
a{color:var(--laranja-forte)}
code{font:.88em ui-monospace,"Cascadia Code",Consolas,monospace;color:var(--laranja-forte);overflow-wrap:anywhere}
header{display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:12px;padding:18px 0}
.logo{display:flex;align-items:center;gap:10px;margin:0;font-size:1.15rem;font-weight:850;letter-spacing:-.01em;color:var(--texto);text-decoration:none}
.logo img{width:30px;height:30px}
.topo-dir{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.rede{display:inline-flex;align-items:center;min-height:34px;padding:4px 14px;border-radius:999px;background:var(--sup);border:1px solid var(--borda);color:var(--jade);font-size:.8rem;font-weight:700;overflow-wrap:anywhere}
.painel{position:relative;overflow:hidden;margin:18px 0 0;padding:30px 24px 22px;border-radius:24px;background:var(--sup);border:1px solid var(--borda);box-shadow:var(--sombra)}
.painel::before{content:"";position:absolute;inset:0 0 auto;height:10px;background:url("@pele@") repeat-x;background-size:auto 10px}
.painel.sobre::before,.painel.avisos::before{background:url("@asa@") repeat-x;background-size:auto 10px}
.painel h2{margin:0 0 14px;font-size:clamp(1.25rem,2.6vw,1.6rem);letter-spacing:-.02em;line-height:1.2}
.painel h2::first-letter{text-transform:uppercase}
.hero{padding-top:34px}
.hero h2{font-size:clamp(1.6rem,4vw,2.4rem);letter-spacing:-.03em}
.hero .ilustra{position:absolute;inset-inline-end:18px;top:22px;width:min(240px,32%);opacity:.95;pointer-events:none}
[dir=rtl] .hero .ilustra{transform:scaleX(-1)}
.hero .intro{max-width:56ch;margin:0 0 18px;color:var(--suave)}
.hero .intro b{color:var(--texto)}
ul.lista{list-style:none;margin:0;padding:0;display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:10px}
ul.lista a{display:flex;align-items:center;gap:10px;padding:14px 16px;border-radius:16px;background:var(--sup2);border:1px solid var(--borda);color:var(--texto);text-decoration:none;font-weight:750;transition:transform .15s ease,border-color .15s ease}
ul.lista a::before{content:"";flex:0 0 10px;height:10px;border-radius:50%;background:var(--folha)}
ul.lista a:hover,ul.lista a:focus-visible{transform:translateY(-2px);border-color:var(--laranja);outline:none}
a.botao,button,.botao{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:42px;padding:0 18px;border-radius:999px;border:1px solid var(--borda);background:var(--sup2);color:var(--texto);text-decoration:none;font:750 .9rem/1 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;cursor:pointer;transition:transform .15s ease,background-color .15s ease,border-color .15s ease}
a.botao:hover,button:hover{transform:translateY(-1px);border-color:var(--laranja)}
a.botao:focus-visible,button:focus-visible,summary:focus-visible,input:focus-visible{outline:3px solid var(--jade);outline-offset:2px}
.principal{background:var(--laranja)!important;border-color:var(--laranja)!important;color:#fff!important}
.principal:hover{background:var(--laranja-forte)!important;border-color:var(--laranja-forte)!important}
:root[data-tema=escuro] .principal{color:#1c1206!important}
@media (prefers-color-scheme:dark){:root:not([data-tema=claro]) .principal{color:#1c1206!important}}
.perigo{color:var(--perigo)!important;border-color:var(--perigo)!important;background:transparent!important}
.perigo:hover{background:var(--perigo)!important;color:#fff!important}
.vazio,.fraco{color:var(--suave);margin:0}
.fraco{font-size:.9rem}
form.guardar{display:flex;gap:10px;flex-wrap:wrap;margin:0 0 12px}
input[type=url]{flex:1 1 300px;min-height:50px;padding:0 20px;border-radius:999px;background:var(--campo);color:var(--texto);border:1px solid var(--borda);font:16px system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
form.guardar button{min-height:50px;padding:0 26px;font-size:1rem}
.msg{margin:14px 0 0;padding:12px 18px;border-radius:14px;background:var(--sup);border:1px solid var(--borda);color:var(--jade);font-weight:650}
.msg.erro{color:var(--perigo)}
.grade{display:grid;grid-template-columns:repeat(auto-fill,minmax(290px,1fr));gap:16px}
.cartao{display:flex;flex-direction:column;overflow:hidden;border-radius:20px;background:var(--sup2);border:1px solid var(--borda);transition:transform .15s ease,border-color .15s ease}
.cartao:hover{border-color:var(--laranja)}
.miniatura{position:relative;height:180px;overflow:hidden;background:#fff;border-bottom:1px solid var(--borda)}
.miniatura iframe{position:absolute;top:0;left:0;width:1280px;height:800px;border:0;transform:scale(.25);transform-origin:0 0;pointer-events:none}
[dir=rtl] .miniatura iframe{left:auto;right:0;transform-origin:100% 0}
.miniatura .icone{position:absolute;inset:0;display:grid;place-items:center;background:var(--sup) url("@pele@") repeat-x 0 0/auto 8px;color:var(--laranja);font-size:1.6rem;font-weight:850}
.cartao .info{padding:14px 16px 8px;display:grid;gap:3px;flex:1}
.cartao h3{margin:0;font-size:1rem;line-height:1.35;overflow-wrap:anywhere}
.cartao .url{font-size:.8rem;color:var(--suave);overflow-wrap:anywhere}
.cartao .estado{font-size:.82rem;font-weight:650;color:var(--jade)} .cartao .estado.erro{color:var(--perigo)}
.cartao .acoes{display:flex;flex-wrap:wrap;gap:6px;padding:8px 16px 16px}
.cartao .acoes form{display:contents}
.cartao .acoes .botao,.cartao .acoes button{min-height:36px;padding:0 14px;font-size:.82rem}
.duas{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.duas .painel{margin-top:18px}
.sobre p{margin:0 0 12px;color:var(--suave)} .sobre p:last-child{margin:0} .sobre b{color:var(--texto)}
dl{margin:0 0 14px;display:grid;gap:10px}
dt{font-weight:800;color:var(--laranja-forte)} dd{margin:2px 0 0;color:var(--suave)}
.avisos ul{margin:0;padding-inline-start:1.1em;display:grid;gap:9px;color:var(--suave)} .avisos b{color:var(--texto)}
.menu{position:relative}
.menu summary{list-style:none;min-height:34px;padding:0 14px;font-size:.8rem;letter-spacing:.04em}
.menu summary::-webkit-details-marker{display:none}
.menu[open] summary{border-color:var(--laranja)}
.menu nav{position:absolute;inset-inline-end:0;top:calc(100% + 8px);z-index:10;display:grid;gap:2px;min-width:170px;padding:8px;border-radius:16px;background:var(--sup);border:1px solid var(--borda);box-shadow:var(--sombra)}
.menu.idiomas nav{grid-template-columns:1fr 1fr;width:min(340px,calc(100vw - 36px))}
.menu nav a{padding:8px 12px;border-radius:10px;color:var(--texto);text-decoration:none;font-size:.92rem}
.menu nav a:hover,.menu nav a:focus-visible{background:var(--sup2);outline:none}
.menu nav a[aria-current]{color:var(--laranja-forte);font-weight:800}
footer{margin-top:36px;padding:34px 20px 30px;text-align:center;color:#fff4e6;font-size:.88rem;background:#1a120b url("@asa@") repeat-x 0 0/auto 20px;border-top:20px solid transparent}
.barra{display:flex;flex-wrap:wrap;gap:6px;align-items:center;padding:10px 14px;background:var(--sup);border-bottom:1px solid var(--borda);box-shadow:0 8px 24px -18px rgba(0,0,0,.5)}
.barra .titulo{flex:1 1 220px;font-weight:800;overflow-wrap:anywhere}
.barra .botao{min-height:36px;padding:0 14px;font-size:.82rem}
[dir=rtl] code{direction:ltr;unicode-bidi:isolate}
.ic{width:18px;height:18px;flex:0 0 18px;vertical-align:middle}
.menu nav a{display:flex;align-items:center;gap:8px}
.so-leitor{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.estreito{max-width:720px}
@media (max-width:760px){.duas{grid-template-columns:1fr}.hero .ilustra{display:none}.painel{padding:28px 16px 18px;border-radius:20px}
.topo-dir{width:100%}.menu{position:static}header{position:relative}.menu nav{left:0;right:0;width:auto!important}}
"#;

// Theme chosen for this page: "" (follows the system), "claro" (light) or "escuro" (dark).
thread_local! {
    static TEMA: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

pub fn definir_tema(cookie: Option<&str>) {
    let t = cookie
        .and_then(|c| c.split(';').find_map(|p| p.trim().strip_prefix("tema=")))
        .map(|v| match v {
            "claro" => "claro",
            "escuro" => "escuro",
            _ => "",
        })
        .unwrap_or("");
    TEMA.with(|x| x.set(t));
}

fn tema() -> &'static str {
    TEMA.with(std::cell::Cell::get)
}

/// SVG embedded as data: (the page fetches nothing from outside).
fn svg_dados(svg: &str) -> String {
    let mut s = String::from("data:image/svg+xml,");
    for c in svg.trim().chars() {
        match c {
            '"' => s.push('\''),
            '#' => s.push_str("%23"),
            '<' => s.push_str("%3C"),
            '>' => s.push_str("%3E"),
            '%' => s.push_str("%25"),
            '\n' | '\r' => s.push(' '),
            c => s.push(c),
        }
    }
    s
}

pub fn icone_dados() -> &'static str {
    static I: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    I.get_or_init(|| svg_dados(include_str!("visual/icone.svg")))
}

fn ilustracao() -> &'static str {
    static I: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    I.get_or_init(|| svg_dados(include_str!("visual/metamorfose.svg")))
}

fn estilo() -> &'static str {
    static E: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    E.get_or_init(|| {
        ESTILO
            .replace("@casca-escura@", &svg_dados(include_str!("visual/casca-escura.svg")))
            .replace("@casca@", &svg_dados(include_str!("visual/casca.svg")))
            .replace("@pele@", &svg_dados(include_str!("visual/pele.svg")))
            .replace("@asa@", &svg_dados(include_str!("visual/asa.svg")))
    })
}

/// Strips terminal decorations from texts ("// ", "&lt; ").
fn limpo(texto: &str) -> &str {
    texto.trim_start_matches("// ").trim_start_matches("&lt; ")
}

fn voltar() -> String {
    let seta = if textos::atual().direita_para_esquerda() { "→" } else { "←" };
    format!("{seta} {}", limpo(t("voltar")))
}

fn pagina(titulo: &str, extra_head: &str, corpo: &str) -> String {
    let tema = tema();
    let marca_tema = if tema.is_empty() { String::new() } else { format!(" data-tema=\"{tema}\"") };
    let esquema = match tema {
        "claro" => "light",
        "escuro" => "dark",
        _ => "light dark",
    };
    format!(
        "<!doctype html>\n<html lang=\"{lang}\"{dir}{marca_tema}><head><meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"color-scheme\" content=\"{esquema}\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<title>{}</title><link rel=\"icon\" href=\"{}\">{extra_head}<style>{}</style></head>\n<body>{corpo}</body></html>",
        escapar_html(titulo),
        icone_dados(),
        estilo(),
        lang = textos::atual().codigo(),
        dir = if textos::atual().direita_para_esquerda() { " dir=\"rtl\"" } else { "" }
    )
}

fn painel(rotulo: &str, classe: &str, conteudo: &str) -> String {
    format!("<section class=\"painel {classe}\"><h2>{rotulo}</h2>{conteudo}</section>")
}

// drawn icons (they do not depend on the system font)
const ICONE_SOL: &str = "<svg class=\"ic\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"4.5\" fill=\"currentColor\"/><g stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"><path d=\"M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8\"/></g></svg>";
const ICONE_LUA: &str = "<svg class=\"ic\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z\" fill=\"currentColor\"/></svg>";
const ICONE_AUTO: &str = "<svg class=\"ic\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"8.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\"/><path d=\"M12 3.5a8.5 8.5 0 0 1 0 17z\" fill=\"currentColor\"/></svg>";

/// Shared header: logo, network status, theme and language (all without scripts).
fn topo(rede: &str) -> String {
    let atual = textos::atual();
    let mut idiomas = String::new();
    for (l, nome) in textos::Lingua::todas() {
        let marca = if l == atual { " aria-current=\"true\"" } else { "" };
        idiomas.push_str(&format!("<a href=\"/idioma/{}\" lang=\"{}\"{marca}>{nome}</a>", l.curto(), l.codigo()));
    }
    let tema = tema();
    let mut temas = String::new();
    for (valor, icone, chave) in [("auto", ICONE_AUTO, "tema-auto"), ("claro", ICONE_SOL, "tema-claro"), ("escuro", ICONE_LUA, "tema-escuro")] {
        let atual = (valor == "auto" && tema.is_empty()) || valor == tema;
        let marca = if atual { " aria-current=\"true\"" } else { "" };
        temas.push_str(&format!("<a href=\"/tema/{valor}\"{marca}>{icone} {}</a>", t(chave)));
    }
    let icone_tema = match tema {
        "claro" => ICONE_SOL,
        "escuro" => ICONE_LUA,
        _ => ICONE_AUTO,
    };
    let rede = if rede.is_empty() { String::new() } else { format!("<span class=\"rede\">{rede}</span>") };
    format!(
        "<header><a class=\"logo\" href=\"/\"><img src=\"{}\" alt=\"\">Local Server</a><span class=\"topo-dir\">{rede}\
<details class=\"menu temas\"><summary class=\"botao\" title=\"{tit}\">{icone_tema}<span class=\"so-leitor\">{tit}</span></summary><nav aria-label=\"{tit}\">{temas}</nav></details>\
<details class=\"menu idiomas\"><summary class=\"botao\" title=\"{idi}\"><span aria-hidden=\"true\">🌐</span> {}<span class=\"so-leitor\"> · {idi}</span></summary><nav aria-label=\"{idi}\">{idiomas}</nav></details></span></header>",
        icone_dados(),
        atual.curto().to_uppercase(),
        tit = t("tema"),
        idi = t("idioma"),
    )
}

fn endereco_da_versao(id: &str, ts: &str, chave: &str) -> String {
    format!("/arquivo/{}/{}/{}", codificar_url(id), codificar_url(ts), caminho_da_chave(chave))
}

/// First version/start address of a stored site.
fn ultima(s: &Servidor, site: &Site) -> Option<(String, String)> {
    let arq = &s.gerente.arq;
    let ts = arq.versoes(&site.id).into_iter().next()?;
    let v = arq.versao(&site.id, &ts)?;
    Some((ts, v.inicio))
}

fn cartao(s: &Servidor, site: &Site, local: bool) -> String {
    let arq = &s.gerente.arq;
    let n_versoes = arq.versoes(&site.id).len();
    let ult = ultima(s, site);
    let titulo = if site.titulo.is_empty() { &site.url } else { &site.titulo };
    let miniatura = match (&ult, site.tipo.as_str()) {
        (Some((ts, ini)), "web" | "pagina") => format!(
            "<iframe src=\"{}\" sandbox=\"\" loading=\"lazy\" tabindex=\"-1\" aria-hidden=\"true\" title=\"\"></iframe>",
            escapar_html(&endereco_da_versao(&site.id, ts, ini))
        ),
        (Some(_), _) => "<div class=\"icone\">PDF</div>".to_string(),
        (None, _) => "<div class=\"icone\">…</div>".to_string(),
    };
    let progresso = s.gerente.progresso.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let estado = match &progresso {
        Some(p) if p.id == site.id => format!(
            "<span class=\"estado\">{}</span>",
            t("baixando").replace("{p}", &p.paginas.to_string()).replace("{a}", &p.recursos.to_string()).replace("{mb}", &format!("{:.1}", p.bytes as f64 / 1_048_576.0))
        ),
        _ if s.gerente.na_fila(&site.id) => format!("<span class=\"estado\">{}</span>", t("fila")),
        _ if !site.erro.is_empty() => format!("<span class=\"estado erro\">● {}</span>", escapar_html(&textos::erro(&site.erro))),
        _ => match &ult {
            Some((ts, _)) => format!(
                "<span class=\"estado\">● {} · {n_versoes} {}</span>",
                carimbo_legivel(ts),
                if n_versoes == 1 { t("versao") } else { t("versoes") }
            ),
            None => format!("<span class=\"estado\">{}</span>", t("sem-copia")),
        },
    };
    let origem = match site.tipo.as_str() {
        "web" => escapar_html(&site.url),
        "pagina" => t("pagina-salva").to_string(),
        _ => t("documento").to_string(),
    };
    let mut acoes = String::new();
    if let Some((ts, ini)) = &ult {
        acoes.push_str(&format!(
            "<a class=\"botao principal\" href=\"/ver/{id}\">{}</a><a class=\"botao\" href=\"{}\">{}</a>",
            t("ampliar"),
            escapar_html(&endereco_da_versao(&site.id, ts, ini)),
            t("abrir"),
            id = codificar_url(&site.id)
        ));
        if n_versoes > 1 {
            acoes.push_str(&format!("<a class=\"botao\" href=\"/versoes/{}\">{}</a>", codificar_url(&site.id), t("versoes")));
        }
    }
    if local {
        if site.tipo == "web" && s.gerente.curl.disponivel {
            acoes.push_str(&format!(
                "<form method=\"post\" action=\"/acao\"><input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"acao\" value=\"atualizar\"><input type=\"hidden\" name=\"id\" value=\"{}\"><button type=\"submit\">{}</button></form>",
                s.token,
                escapar_html(&site.id),
                t("atualizar")
            ));
        }
        acoes.push_str(&format!("<a class=\"botao perigo\" href=\"/remover/{}\">{}</a>", codificar_url(&site.id), t("remover")));
    }
    format!(
        "<article class=\"cartao\"><div class=\"miniatura\">{miniatura}</div><div class=\"info\"><h3>{}</h3><span class=\"url\">{origem}</span>{estado}</div><div class=\"acoes\">{acoes}</div></article>",
        escapar_html(titulo)
    )
}

pub fn inicial(s: &Servidor, local: bool, consulta: Option<&str>) -> String {
    let g = &s.gerente;
    let sites_pasta = listar_sites(&s.raiz);
    let mut lista = String::new();
    if sites_pasta.is_empty() {
        lista.push_str(&format!("<p class=\"vazio\">{}</p>", limpo(t("sem-sites"))));
    } else {
        lista.push_str("<ul class=\"lista\">");
        for n in &sites_pasta {
            lista.push_str(&format!("<li><a href=\"/{}/\">{}</a></li>", codificar_url(n), escapar_html(n)));
        }
        lista.push_str("</ul>");
    }
    let rede = if s.config.compartilhar {
        let ends: Vec<String> = s.enderecos.iter().map(|e| format!("<code>http://{}:{}/</code>", escapar_html(e), s.config.porta)).collect();
        if ends.is_empty() { t("wifi").to_string() } else { format!("{}: {}", t("wifi-celular"), ends.join(" ")) }
    } else {
        t("so-aqui").to_string()
    };

    let guardados = g.arq.sites();
    let ocupado = g.progresso.lock().unwrap_or_else(|e| e.into_inner()).is_some() || !g.fila.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    let mut arquivo = String::new();
    if local {
        if g.curl.disponivel {
            arquivo.push_str(&format!(
                "<form class=\"guardar\" method=\"post\" action=\"/acao\"><input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"acao\" value=\"adicionar\"><input type=\"url\" name=\"url\" required placeholder=\"{}\" aria-label=\"{}\"><button class=\"principal\" type=\"submit\">{}</button></form>",
                s.token,
                t("placeholder-url"),
                t("aria-url"),
                t("guardar")
            ));
        } else {
            arquivo.push_str(&format!("<p class=\"msg erro\">{}</p>", limpo(t("sem-curl"))));
        }
        arquivo.push_str(&format!(
            "<p class=\"fraco\">{}</p>",
            limpo(t("importar")).replace("{d}", &escapar_html(&g.arq.raiz.join("importar").display().to_string()))
        ));
    }
    if guardados.is_empty() {
        arquivo.push_str(&format!("<p class=\"vazio\" style=\"margin-top:16px\">{}</p>", limpo(t("nada"))));
    } else {
        arquivo.push_str("<div class=\"grade\" style=\"margin-top:16px\">");
        for site in &guardados {
            arquivo.push_str(&cartao(s, site, local));
        }
        arquivo.push_str("</div>");
        arquivo.push_str(&format!(
            "<p class=\"fraco\" style=\"margin-top:12px\">{}</p>",
            limpo(t("total")).replace("{n}", &guardados.len().to_string()).replace("{mb}", &format!("{:.1}", g.arq.tamanho_total() as f64 / 1_048_576.0))
        ));
    }

    let avisos = textos::avisos();
    let mensagem = match consulta.and_then(|q| q.split('&').find_map(|p| p.strip_prefix("m="))) {
        Some("ok") => format!("<p class=\"msg\">{}</p>", limpo(t("m-ok"))),
        Some("removido") => format!("<p class=\"msg\">{}</p>", limpo(t("m-removido"))),
        Some("invalido") => format!("<p class=\"msg erro\">{}</p>", limpo(t("m-invalido"))),
        _ => String::new(),
    };
    let hero = format!(
        "<section class=\"painel hero\"><img class=\"ilustra\" src=\"{}\" alt=\"\"><h2>{}</h2><p class=\"intro\">{}</p>{arquivo}</section>",
        ilustracao(),
        t("p-arquivo"),
        t("sobre-1"),
    );
    let corpo = format!(
        "<main>{}{mensagem}{hero}{}<div class=\"duas\">{}{}</div></main><footer>{}</footer>",
        topo(&rede),
        painel(t("p-sites"), "", &lista),
        painel(t("p-avisos"), "avisos", &avisos),
        painel(t("p-sobre"), "sobre", &textos::sobre()),
        limpo(t("rodape")),
    );
    let refresh = if ocupado { "<meta http-equiv=\"refresh\" content=\"5\">" } else { "" };
    pagina("Local Server", refresh, &corpo)
}

/// Viewer: the copy in full screen, with large zoom for people with low vision.
pub fn ver(s: &Servidor, id: &str, ts: Option<&str>, consulta: Option<&str>) -> String {
    let arq = &s.gerente.arq;
    let Some(site) = arq.site(id) else { return nao_existe() };
    let versoes = arq.versoes(id);
    let Some(ts) = ts.map(str::to_string).or_else(|| versoes.first().cloned()) else { return nao_existe() };
    let Some(v) = arq.versao(id, &ts) else { return nao_existe() };
    let zoom: u32 = consulta
        .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("z=")))
        .and_then(|z| z.parse().ok())
        .filter(|z| [100, 125, 150, 200, 300].contains(z))
        .unwrap_or(100);
    let alvo = endereco_da_versao(id, &ts, &v.inicio);
    let base_ver = format!("/ver/{}/{}", codificar_url(id), codificar_url(&ts));
    let zooms: String = [100, 125, 150, 200, 300]
        .iter()
        .map(|z| {
            format!(
                "<a class=\"botao{}\" href=\"{base_ver}?z={z}\" aria-label=\"{}\">{z}%</a>",
                if *z == zoom { " principal" } else { "" },
                t("zoom-aria").replace("{z}", &z.to_string())
            )
        })
        .collect();
    let escala = zoom as f64 / 100.0;
    let quadro = if site.tipo == "documento" {
        format!("<iframe src=\"{}\" title=\"{}\" style=\"width:100%;height:100%;border:0\"></iframe>", escapar_html(&alvo), escapar_html(&site.titulo))
    } else {
        format!(
            "<iframe src=\"{}\" sandbox=\"allow-popups allow-popups-to-escape-sandbox\" title=\"{}\" style=\"width:{:.4}%;height:{:.4}%;border:0;transform:scale({escala});transform-origin:0 0;background:#fff\"></iframe>",
            escapar_html(&alvo),
            escapar_html(&site.titulo),
            100.0 / escala,
            100.0 / escala
        )
    };
    let online = match crate::baixar::original(&site, &v.inicio) {
        Some(o) => format!("<a class=\"botao\" href=\"{}\" rel=\"noreferrer noopener\" target=\"_blank\">{}</a>", escapar_html(&o), t("ver-online")),
        None => String::new(),
    };
    let outras = if versoes.len() > 1 { format!("<a class=\"botao\" href=\"/versoes/{}\">{} ({})</a>", codificar_url(id), t("versoes"), versoes.len()) } else { String::new() };
    let corpo = format!(
        "<div style=\"position:fixed;inset:0;display:flex;flex-direction:column\">\
<nav class=\"barra\"><a class=\"botao\" href=\"/\">{}</a><span class=\"titulo\">{} <span class=\"fraco\" style=\"font-weight:500\">· {}</span></span>\
<span class=\"fraco\">zoom</span>{zooms}{outras}<a class=\"botao\" href=\"{}\">{}</a>{online}</nav>\
<div style=\"flex:1;overflow:hidden;position:relative;background:#fff\">{quadro}</div></div>",
        voltar(),
        escapar_html(if site.titulo.is_empty() { &site.url } else { &site.titulo }),
        carimbo_legivel(&ts),
        escapar_html(&alvo),
        t("tela-cheia")
    );
    pagina(&site.titulo, "", &corpo)
}

pub fn versoes(s: &Servidor, id: &str) -> String {
    let arq = &s.gerente.arq;
    let Some(site) = arq.site(id) else { return nao_existe() };
    let mut lista = String::from("<ul class=\"lista\">");
    for ts in arq.versoes(id) {
        lista.push_str(&format!("<li><a href=\"/ver/{}/{}\">{}</a></li>", codificar_url(id), codificar_url(&ts), carimbo_legivel(&ts)));
    }
    lista.push_str("</ul>");
    let corpo = format!(
        "<main>{}<p><a class=\"botao\" href=\"/\">{}</a></p>{}</main>",
        topo(""),
        voltar(),
        painel(&format!("{} · {}", t("versoes"), escapar_html(if site.titulo.is_empty() { &site.url } else { &site.titulo })), "", &lista)
    );
    pagina(t("t-versoes"), "", &corpo)
}

pub fn confirmar_remocao(s: &Servidor, id: &str) -> String {
    let Some(site) = s.gerente.arq.site(id) else { return nao_existe() };
    let conteudo = format!(
        "<p>{}</p>\
<form method=\"post\" action=\"/acao\" style=\"display:flex;gap:8px;flex-wrap:wrap\"><input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"acao\" value=\"remover\"><input type=\"hidden\" name=\"id\" value=\"{}\">\
<button class=\"perigo\" type=\"submit\">{}</button><a class=\"botao\" href=\"/\">{}</a></form>",
        t("remover-pergunta").replace("{t}", &escapar_html(if site.titulo.is_empty() { &site.url } else { &site.titulo })),
        s.token,
        escapar_html(id),
        t("sim-remover"),
        t("cancelar")
    );
    pagina(t("t-remover"), "", &format!("<main class=\"estreito\">{}{}</main>", topo(""), painel(t("t-remover"), "sobre", &conteudo)))
}

pub fn nao_guardado(site: &Site, chave: &str, id: &str, ts: &str) -> String {
    let online = crate::baixar::original(site, chave)
        .map(|o| format!("<p><a class=\"botao\" href=\"{}\" rel=\"noreferrer noopener\" target=\"_blank\">{}</a></p>", escapar_html(&o), t("abrir-online")))
        .unwrap_or_default();
    let conteudo = format!(
        "<p>{}</p>{online}<p><a class=\"botao\" href=\"/ver/{}/{}\">{}</a></p>",
        t("nao-guardada"),
        codificar_url(id),
        codificar_url(ts),
        t("voltar-copia")
    );
    pagina(t("t-nao-guardada"), "", &format!("<main class=\"estreito\">{}{}</main>", topo(""), painel(t("t-nao-guardada"), "sobre", &conteudo)))
}

fn nao_existe() -> String {
    let conteudo = format!("<p>{}</p><p><a class=\"botao\" href=\"/\">{}</a></p>", t("nao-existe"), voltar());
    pagina(t("t-nao-existe"), "", &format!("<main class=\"estreito\">{}{}</main>", topo(""), painel(t("t-nao-existe"), "sobre", &conteudo)))
}


pub fn tipo_do_arquivo(caminho: &Path) -> &'static str {
    let ext = caminho.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "vtt" => "text/vtt; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn escapa_nomes_de_pastas() {
        assert_eq!(escapar_html("<script>\"x\"&'"), "&lt;script&gt;&quot;x&quot;&amp;&#39;");
        assert_eq!(codificar_url("Bíblia Sagrada"), "B%C3%ADblia%20Sagrada");
    }
}
