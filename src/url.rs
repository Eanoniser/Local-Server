// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Endereços da web: leitura, resolução de links relativos (RFC 3986) e
//! conversão para o caminho local do arquivo.

#[derive(Clone, Debug, PartialEq)]
pub struct Url {
    pub esquema: String, // "http" ou "https"
    pub host: String,    // minúsculo, sem porta
    pub porta: Option<u16>,
    pub caminho: String, // sempre começa com "/"
    pub consulta: Option<String>,
}

impl Url {
    pub fn ler(texto: &str) -> Option<Url> {
        let t = texto.trim();
        let (esquema, resto) = t.split_once("://")?;
        let esquema = esquema.to_ascii_lowercase();
        if esquema != "http" && esquema != "https" {
            return None;
        }
        let resto = resto.split('#').next()?;
        let fim_auth = resto.find(['/', '?']).unwrap_or(resto.len());
        let (auth, cauda) = resto.split_at(fim_auth);
        if auth.is_empty() || auth.contains('@') || auth.contains(['\\', ' ', '\t', '\r', '\n']) {
            return None; // sem usuário:senha no endereço
        }
        let (host, porta) = if let Some(r) = auth.strip_prefix('[') {
            let (h, p) = r.split_once(']')?;
            let p = p.strip_prefix(':').map(|p| p.parse::<u16>().ok()).unwrap_or(Some(0))?;
            (format!("[{}]", h.to_ascii_lowercase()), if p == 0 { None } else { Some(p) })
        } else {
            match auth.rsplit_once(':') {
                Some((h, p)) => (h.to_ascii_lowercase(), Some(p.parse::<u16>().ok()?)),
                None => (auth.to_ascii_lowercase(), None),
            }
        };
        if host.is_empty() || host.len() > 253 {
            return None;
        }
        let porta = porta.filter(|&p| !((esquema == "http" && p == 80) || (esquema == "https" && p == 443)));
        let (caminho, consulta) = match cauda.split_once('?') {
            Some((c, q)) => (c, Some(q.to_string())),
            None => (cauda, None),
        };
        let caminho = if caminho.is_empty() { "/".to_string() } else { normalizar(caminho) };
        Some(Url { esquema, host, porta, caminho, consulta })
    }

    pub fn porta_real(&self) -> u16 {
        self.porta.unwrap_or(if self.esquema == "https" { 443 } else { 80 })
    }

    pub fn origem(&self) -> String {
        match self.porta {
            Some(p) => format!("{}://{}:{p}", self.esquema, self.host),
            None => format!("{}://{}", self.esquema, self.host),
        }
    }

    pub fn texto(&self) -> String {
        let mut s = format!("{}{}", self.origem(), self.caminho);
        if let Some(q) = &self.consulta {
            s.push('?');
            s.push_str(q);
        }
        s
    }

    /// Resolve um link encontrado nesta página (relativo ou absoluto).
    pub fn juntar(&self, link: &str) -> Option<Url> {
        let l = link.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        let minus = l.to_ascii_lowercase();
        if minus.starts_with("http://") || minus.starts_with("https://") {
            return Url::ler(l);
        }
        if let Some(r) = l.strip_prefix("//") {
            return Url::ler(&format!("{}://{r}", self.esquema));
        }
        // outros esquemas (mailto:, javascript:, data:, tel:) ficam de fora
        if let Some(pos) = l.find(':') {
            if !l[..pos].contains(['/', '?', '#']) {
                return None;
            }
        }
        let l = l.split('#').next()?;
        let (cam, consulta) = match l.split_once('?') {
            Some((c, q)) => (c, Some(q.to_string())),
            None => (l, None),
        };
        let caminho = if cam.is_empty() {
            self.caminho.clone()
        } else if cam.starts_with('/') {
            normalizar(cam)
        } else {
            let base = &self.caminho[..=self.caminho.rfind('/').unwrap_or(0)];
            normalizar(&format!("{base}{cam}"))
        };
        let consulta = if cam.is_empty() && consulta.is_none() { self.consulta.clone() } else { consulta };
        Some(Url { caminho, consulta, ..self.clone() })
    }

    /// Chave local: "host[:porta]/caminho[?consulta]".
    pub fn chave(&self) -> String {
        let mut s = match self.porta {
            Some(p) => format!("{}:{p}{}", self.host, self.caminho),
            None => format!("{}{}", self.host, self.caminho),
        };
        if let Some(q) = &self.consulta {
            s.push('?');
            s.push_str(q);
        }
        s
    }
}

/// Remove "." e ".." do caminho.
pub fn normalizar(caminho: &str) -> String {
    let mut partes: Vec<&str> = Vec::new();
    let segs: Vec<&str> = caminho.split('/').collect();
    for (i, s) in segs.iter().enumerate() {
        match *s {
            "." => {
                if i == segs.len() - 1 {
                    partes.push("");
                }
            }
            ".." => {
                if partes.len() > 1 {
                    partes.pop();
                }
                if i == segs.len() - 1 {
                    partes.push("");
                }
            }
            s => partes.push(s),
        }
    }
    let mut r = partes.join("/");
    if !r.starts_with('/') {
        r.insert(0, '/');
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ler_e_juntar() {
        let u = Url::ler("HTTPS://Exemplo.com:443/a/b/pagina.html?x=1#topo").unwrap();
        assert_eq!(u.texto(), "https://exemplo.com/a/b/pagina.html?x=1");
        assert_eq!(u.juntar("../c.css").unwrap().texto(), "https://exemplo.com/a/c.css");
        assert_eq!(u.juntar("/img/l.png").unwrap().texto(), "https://exemplo.com/img/l.png");
        assert_eq!(u.juntar("//cdn.ex.org/f.woff2").unwrap().texto(), "https://cdn.ex.org/f.woff2");
        assert_eq!(u.juntar("?p=2").unwrap().texto(), "https://exemplo.com/a/b/pagina.html?p=2");
        assert_eq!(u.juntar("#x"), None);
        assert_eq!(u.juntar("mailto:a@b.c"), None);
        assert_eq!(u.juntar("javascript:alert(1)"), None);
        assert_eq!(u.juntar("../../../../x").unwrap().caminho, "/x");
        assert_eq!(u.chave(), "exemplo.com/a/b/pagina.html?x=1");
    }

    #[test]
    fn recusa_estranhos() {
        assert!(Url::ler("ftp://x.com/").is_none());
        assert!(Url::ler("file:///etc/passwd").is_none());
        assert!(Url::ler("https://user:senha@x.com/").is_none());
        assert!(Url::ler("https:///x").is_none());
        assert_eq!(Url::ler("http://x.com:8080").unwrap().chave(), "x.com:8080/");
        assert_eq!(Url::ler("http://[::1]:80/").unwrap().host, "[::1]");
    }
}
