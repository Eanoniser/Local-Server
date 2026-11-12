// Local Server — SPDX-License-Identifier: GPL-3.0-or-later
//! Compressão gzip sem perda (DEFLATE com LZ77 + Huffman fixo), escrita à mão.
//! O navegador descomprime sozinho (Content-Encoding: gzip): nada se perde.

const JANELA: usize = 32 * 1024;
const MIN_IGUAL: usize = 3;
const MAX_IGUAL: usize = 258;
const MAX_CADEIA: usize = 48;
const BITS_HASH: u32 = 15;

pub fn crc32(dados: &[u8]) -> u32 {
    static TABELA: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABELA.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *v = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in dados {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

struct Bits {
    saida: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn por(&mut self, valor: u32, bits: u32) {
        self.acc |= (valor as u64) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.saida.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Códigos de Huffman vão do bit mais significativo para o menos.
    fn codigo(&mut self, codigo: u32, bits: u32) {
        let mut r = 0;
        for i in 0..bits {
            r |= ((codigo >> i) & 1) << (bits - 1 - i);
        }
        self.por(r, bits);
    }
    fn terminar(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.saida.push(self.acc as u8);
        }
        self.saida
    }
}

fn literal(b: &mut Bits, simbolo: u32) {
    match simbolo {
        0..=143 => b.codigo(0x30 + simbolo, 8),
        144..=255 => b.codigo(0x190 + simbolo - 144, 9),
        256..=279 => b.codigo(simbolo - 256, 7),
        _ => b.codigo(0xC0 + simbolo - 280, 8),
    }
}

const BASE_COMP: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const EXTRA_COMP: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const BASE_DIST: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const EXTRA_DIST: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn par(b: &mut Bits, comp: usize, dist: usize) {
    let i = BASE_COMP.iter().rposition(|&x| x as usize <= comp).unwrap();
    literal(b, 257 + i as u32);
    b.por((comp - BASE_COMP[i] as usize) as u32, EXTRA_COMP[i] as u32);
    let j = BASE_DIST.iter().rposition(|&x| x as usize <= dist).unwrap();
    b.codigo(j as u32, 5);
    b.por((dist - BASE_DIST[j] as usize) as u32, EXTRA_DIST[j] as u32);
}

fn hash(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
    (v.wrapping_mul(2_654_435_761) >> (32 - BITS_HASH)) as usize
}

pub fn deflate(d: &[u8]) -> Vec<u8> {
    let mut b = Bits { saida: Vec::with_capacity(d.len() / 2 + 64), acc: 0, n: 0 };
    b.por(1, 1); // último bloco
    b.por(1, 2); // Huffman fixo
    let mut cabeca = vec![usize::MAX; 1 << BITS_HASH];
    let mut anterior = vec![usize::MAX; JANELA];
    let inserir = |cabeca: &mut Vec<usize>, anterior: &mut Vec<usize>, i: usize| {
        if i + MIN_IGUAL <= d.len() {
            let h = hash(d, i);
            anterior[i % JANELA] = cabeca[h];
            cabeca[h] = i;
        }
    };
    let mut i = 0;
    while i < d.len() {
        let mut melhor = (0usize, 0usize);
        if i + MIN_IGUAL <= d.len() {
            let mut cand = cabeca[hash(d, i)];
            let limite = (d.len() - i).min(MAX_IGUAL);
            let mut passos = 0;
            while cand != usize::MAX && i - cand <= JANELA && passos < MAX_CADEIA {
                let mut n = 0;
                while n < limite && d[cand + n] == d[i + n] {
                    n += 1;
                }
                if n > melhor.0 {
                    melhor = (n, i - cand);
                    if n == limite {
                        break;
                    }
                }
                let prox = anterior[cand % JANELA];
                if prox == usize::MAX || prox >= cand {
                    break;
                }
                cand = prox;
                passos += 1;
            }
        }
        if melhor.0 >= MIN_IGUAL {
            par(&mut b, melhor.0, melhor.1);
            for k in i..i + melhor.0 {
                inserir(&mut cabeca, &mut anterior, k);
            }
            i += melhor.0;
        } else {
            literal(&mut b, d[i] as u32);
            inserir(&mut cabeca, &mut anterior, i);
            i += 1;
        }
    }
    literal(&mut b, 256); // fim do bloco
    b.terminar()
}

pub fn gzip(dados: &[u8]) -> Vec<u8> {
    let mut s = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
    s.extend(deflate(dados));
    s.extend(crc32(dados).to_le_bytes());
    s.extend((dados.len() as u32).to_le_bytes());
    s
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn crc_conhecido() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// Confere com o gzip do sistema (se existir) que a volta é idêntica.
    #[test]
    fn ida_e_volta() {
        let mut exemplos: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"a".to_vec(),
            b"<html><body><p>ola ola ola ola ola</p></body></html>".repeat(200),
            (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect(),
        ];
        exemplos.push((0..5000u32).flat_map(|i| format!("linha {i} - texto repetido\n").into_bytes()).collect());
        for e in exemplos {
            let gz = gzip(&e);
            let Ok(mut p) = Command::new("gzip").arg("-dc").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() else {
                return;
            };
            p.stdin.take().unwrap().write_all(&gz).unwrap();
            let s = p.wait_with_output().unwrap();
            assert!(s.status.success());
            assert_eq!(s.stdout, e);
        }
        let texto = b"<div class=\"x\">conteudo</div>".repeat(1000);
        assert!(gzip(&texto).len() < texto.len() / 10);
    }
}
