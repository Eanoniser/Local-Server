# Local Server

> **English summary:** a tiny offline "Cloudflare" for any computer, written in Rust
> using only the standard library. It serves your own sites/apps with no internet,
> optionally to phones on the same Wi-Fi, and keeps a personal, versioned archive of
> websites (like a private Wayback Machine). HTTPS uses the system's `curl`; pages that
> need JavaScript are rendered by the browser already installed (Edge/Chrome/Chromium),
> invisible, with an empty temporary profile and a privacy filter. Interface in 20 languages,
> picked from the browser/system language. Free software (GPL-3.0).

Uma "Cloudflare" pequena para cada computador: guarda sites e apps e os abre
**sem internet**. Pode também servir celulares e computadores pelo mesmo Wi-Fi.
Cada pessoa guarda o que for importante para ela.

- **Sites seus:** cada pasta dentro de `sites/` vira um site.
- **Arquivo de sites:** cole o endereço de um site e ele guarda uma cópia para ver
  offline, com versões datadas (como um museu pessoal). Confere de tempos em tempos
  se o site mudou.
- **Rust puro**, só com a biblioteca padrão. Programa de ~0,5 MB. Roda em computador antigo.
- **20 idiomas:** a página segue o idioma do navegador e a janela segue o idioma do
  sistema (tudo no próprio computador). Há um menu 🌐 para trocar.
- **Privado:** não guarda registro de quem acessa; as cópias abrem sem scripts e sem
  buscar nada na internet.

## Baixar e usar

Baixe o programa pronto na área **Releases** deste repositório:

| Computador | Arquivo |
|---|---|
| Windows 10 ou 11 | `local-server-windows.exe` |
| Windows 7 / 8 (64 bits) | `local-server-windows7-64.exe` |
| Windows 7 / 8 (32 bits) | `local-server-windows7-32.exe` |
| Mac com chip Apple (M1, M2…) | `local-server-mac-apple` |
| Mac com Intel | `local-server-mac-intel` |
| Linux (64 bits) | `local-server-linux-64` |
| Linux (32 bits) | `local-server-linux-32` |
| Raspberry Pi 3/4/5, Android (Termux) | `local-server-linux-arm64` |
| Raspberry Pi antigo (32 bits) | `local-server-linux-arm32` |

- **Windows:** dois cliques. Se aparecer "O Windows protegeu o computador", clique em
  **Mais informações → Executar assim mesmo** (o programa não tem assinatura paga).
  No Windows 7, se der erro de "api-ms-win-crt", instale a atualização KB2999226.
- **Mac:** no Terminal, `chmod +x local-server-mac-apple` e `./local-server-mac-apple`.
  Se o Mac bloquear, vá em **Ajustes do Sistema → Privacidade e Segurança → Abrir Mesmo Assim**.
- **Linux:** `chmod +x local-server-linux-64` e `./local-server-linux-64`.

Abre uma janela (o servidor ligado) e o navegador na página inicial
(`http://127.0.0.1:8080`). Para desligar, feche a janela. Depois de baixado,
**funciona sem internet** (a internet só é usada para guardar sites novos).

## Arquivo de sites

1. Na página inicial, cole o endereço e clique em **guardar site**.
2. Cada site vira um cartão com miniatura. **ampliar** abre em tela cheia com zoom de
   100% a 300% (para quem enxerga pouco). **versões** mostra as cópias antigas por data.
3. A cada 24 horas (ajustável) confere se o site mudou e guarda uma versão nova.
   Arquivos repetidos ocupam espaço uma vez só; textos são comprimidos sem perda.
4. As fotos guardadas perdem os dados escondidos (câmera, data, GPS) sem mudar nenhum
   pixel. Opcional: modo econômico, em que fotos grandes viram WebP (30% a 70% menores).

**Páginas que dependem de JavaScript:** o servidor usa o navegador que já está no
computador (Edge, Chrome, Chromium ou Brave), invisível. Não é extensão e não instala nada.
- Perfil novo e vazio a cada página, apagado no fim (sem seus logins e histórico).
- Todo o tráfego desse navegador passa por um filtro local que bloqueia a rede interna,
  os serviços do próprio navegador (contas, atualização, telemetria) e rastreadores comuns.
- Continuam sem funcionar: login, rolagem infinita e botões interativos (a cópia é uma
  "foto navegável").

**Assinaturas e páginas com login:** o servidor nunca usa sua senha. Salve a página com
**Ctrl+S** ("Página da Web, completa") ou em **PDF** e coloque em `arquivo/importar`.

**Limites:** até 200 páginas e 300 MB por site (ajustável). Respeita o `robots.txt` e
baixa devagar. Celulares veem tudo pelo Wi-Fi, mas não guardam sites sozinhos.
Windows 7 não tem curl: mostra e importa, mas não baixa sites.

**Responsabilidade:** a cópia fica só no seu computador, mas as leis continuam valendo
para quem guarda (direitos autorais, dados pessoais/LGPD). Guardar para uso pessoal
costuma ser aceito; compartilhar o site de outra pessoa pode ser redistribuição.

## Compartilhar pelo Wi-Fi

No `config.txt` (criado na primeira vez), troque para `compartilhar_na_rede = sim`
e abra o programa de novo. A janela mostra o endereço para o celular (mesmo Wi-Fi).
O roteador não precisa ter internet.

## Segurança

- Por padrão só o próprio computador acessa.
- Só entrega arquivos de `sites/` e do arquivo; bloqueia arquivos ocultos, atalhos
  para fora da pasta e truques de caminho (`../`).
- Botões (guardar, atualizar, remover) só funcionam no próprio computador, na própria
  página, com uma senha de uso único por execução. Outros sites não conseguem usá-los.
- Ao baixar, **nunca** acessa a rede interna (roteador, outros aparelhos), nem pelo
  curl nem pelo navegador invisível.
- Cópias abrem em "caixa de areia" (sem scripts, sem rede externa).

Achou um problema de segurança? Abra uma *issue* neste repositório.

## Configuração (`config.txt`)

```
porta = 8080
compartilhar_na_rede = nao
arquivo_paginas_max = 200
arquivo_mb_max = 300
arquivo_atualizar_horas = 24
arquivo_navegador = auto      # auto, sempre ou nunca
arquivo_economizar_imagens = nao   # sim: fotos grandes viram WebP (usa o navegador do PC, sem internet)
```

## Compilar

Precisa do [Rust](https://rustup.rs). A biblioteca padrão é a única dependência.

```
cargo test
cargo build --release
```

O programa fica em `target/release/local-server`.

## Licença

Software livre sob a **GNU General Public License v3.0 ou posterior** (veja `LICENSE`).
Você pode usar, copiar, estudar, modificar e distribuir, de graça, inclusive em
empresas. Se distribuir uma versão modificada, o código dela também precisa ser aberto
sob a mesma licença.
