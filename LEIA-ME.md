# Local Server

Uma "Cloudflare" pequena para cada computador: guarda sites e apps e os abre
**sem internet**. Pode também servir celulares e computadores pelo mesmo Wi-Fi,
sem precisar de internet. Cada pessoa guarda o que for importante para ela.

Tem também um **arquivo de sites**: você cola o endereço de um site e o
servidor guarda uma cópia para ver offline. Ele confere de tempos em tempos se o
site mudou e guarda uma versão nova com data, sem apagar as antigas.

- Feito em Rust, só com a biblioteca padrão (sem nada de terceiros).
- Para HTTPS usa o `curl` que já vem no sistema (Windows 10/11, Mac, Linux).
- Não guarda registro de quem acessa. Programa de ~0,5 MB. Roda em computador antigo.
- 20 idiomas: as páginas seguem o idioma do navegador e a janela segue o do sistema.
  O menu 🌐 no topo da página troca o idioma (a escolha fica só neste computador).
  Os textos ficam em `src/idiomas/`, um arquivo por idioma; correções são bem-vindas.

## Qual arquivo abrir

| Computador | Arquivo |
|---|---|
| Windows 10 ou 11 | `local-server-windows.exe` |
| Windows 7 / 8 (64 bits) | `local-server-windows7-64.exe` |
| Windows 7 / 8 (32 bits, bem antigo) | `local-server-windows7-32.exe` |
| Mac com chip Apple (M1, M2, M3…) | `local-server-mac-apple` |
| Mac com Intel | `local-server-mac-intel` |
| Linux (64 bits) | `local-server-linux-64` |
| Linux (32 bits, bem antigo) | `local-server-linux-32` |
| Raspberry Pi 3/4/5, Android (Termux) | `local-server-linux-arm64` |
| Raspberry Pi antigo (32 bits) | `local-server-linux-arm32` |

**Windows:** dê dois cliques. Na primeira vez pode aparecer "O Windows protegeu o
computador": clique em **Mais informações → Executar assim mesmo**.
No Windows 7, se der erro de "api-ms-win-crt", instale a atualização KB2999226
(Windows Update) uma vez.

**Mac:** no Terminal, dentro da pasta: `chmod +x local-server-mac-apple` e depois
`./local-server-mac-apple`. Se o Mac bloquear ("desenvolvedor não identificado"),
vá em **Ajustes do Sistema → Privacidade e Segurança → Abrir Mesmo Assim**.
(A versão de Mac foi compilada, mas ainda não foi testada num Mac de verdade.)

**Linux:** `chmod +x local-server-linux-64` e depois `./local-server-linux-64`.

Abre uma janela (o servidor ligado) e o navegador com a página inicial.
Para desligar, feche a janela.

## Arquivo de sites

1. Na página inicial, cole o endereço (ex.: `https://site.com`) e clique em
   **guardar site**. O download acontece aos poucos e a página se atualiza sozinha.
2. Cada site aparece num cartão com miniatura. **ampliar** abre em tela cheia
   com zoom de 100% a 300% (bom para quem enxerga pouco). **abrir** abre a cópia
   direto. **versões** mostra as cópias antigas, por data.
3. A cada 24 horas (dá para mudar no `config.txt`) o servidor confere se o site
   mudou. Se mudou, guarda uma versão nova; se não mudou, não ocupa espaço.

**Matérias de assinatura e páginas com login:** o servidor nunca usa sua senha.
Abra a matéria, salve com **Ctrl+S** ("Página da Web, completa") ou em **PDF**, e
coloque o arquivo (e a pasta `_files` que vem junto) em `arquivo/importar`.
Em alguns segundos ela aparece na lista. Os originais vão para
`arquivo/importar/importados`.

**Como guarda:** cada arquivo é guardado uma vez só, mesmo que se repita em
várias versões ou sites. Textos são comprimidos sem perda (gzip). Fotos e vídeos
já vêm comprimidos e ficam como estão.

**Fotos:** os dados escondidos das fotos (modelo da câmera, data, localização GPS,
programa usado, comentários) são sempre apagados, sem mudar nenhum pixel. Ficam só o
perfil de cor e a rotação da foto, que mudam a aparência.

**Modo econômico (opcional):** com `arquivo_economizar_imagens = sim` no `config.txt`,
fotos grandes (JPEG e PNG acima de 60 KB) viram WebP, normalmente de 30% a 70% menores,
com diferença quase invisível. A conversão usa o navegador que já está no computador
(Edge, Chrome ou Chromium), sem internet nenhuma. Só troca quando fica pelo menos 20%
menor; prints de tela com letras costumam ficar no original, para não borrar.
Vem desligado porque reduz um pouco a nitidez, e isso pesa para quem usa o zoom.
Vale para os sites guardados depois de ligar.

**Páginas que dependem de JavaScript:** o servidor usa o navegador que já está no
computador (Edge, Chrome, Chromium ou Brave), invisível. Não é extensão e não instala
nada. Cada página abre num perfil novo e vazio, apagado no fim (sem seus logins), e
todo o tráfego passa por um filtro que bloqueia a rede interna, os serviços do próprio
navegador (contas, atualização, telemetria) e rastreadores comuns. Para desligar:
`arquivo_navegador = nunca` no `config.txt`.

**Limites:**
- Login, rolagem infinita e botões interativos continuam sem funcionar: a cópia é uma
  "foto navegável" e abre **sem scripts**, por segurança.
- Por padrão guarda até 200 páginas e 300 MB por site (muda no `config.txt`).
- Respeita o `robots.txt` (quando o site pede para não ser copiado) e baixa
  devagar para não sobrecarregar ninguém.
- Só o próprio computador pode adicionar, atualizar ou remover. Pelo Wi-Fi as
  outras pessoas só **veem** o que foi guardado.
- Celulares (Android, iPhone e outros) veem tudo pelo Wi-Fi, mas não conseguem
  guardar sites sozinhos: os navegadores de celular não permitem.
- Windows 7 não tem curl: mostra e importa, mas não baixa sites.

**Responsabilidade:** a cópia fica só no seu computador e ninguém consegue
apagá-la de fora. Mas as leis continuam valendo para quem guarda (direitos
autorais, dados pessoais/LGPD). Guardar para uso pessoal costuma ser aceito;
compartilhar o site de outra pessoa pode ser redistribuição.

## Adicionar sites seus (pastas)

Coloque cada site numa pasta dentro de `sites`, com um `index.html`.
Exemplo: `sites/livros/index.html` abre em `http://127.0.0.1:8080/livros/`.
Serve também vídeos, áudios e PDFs (dá para pular partes do vídeo).

Já vem com o **Texto do Print** em `sites/texto-do-print`.

## Compartilhar pelo Wi-Fi

1. Abra `config.txt` (é criado na primeira vez que o programa abre).
2. Troque para `compartilhar_na_rede = sim`, salve, feche e abra o programa de novo.
3. A janela mostra o endereço para digitar no celular, por exemplo
   `http://192.168.0.15:8080/`. O celular precisa estar no mesmo Wi-Fi.
4. Se o Windows perguntar sobre o firewall, permita em **redes privadas**.

O roteador não precisa ter internet, só estar ligado.

## Segurança

- Com `compartilhar_na_rede = nao` (padrão), só este computador acessa.
- Só entrega arquivos de dentro de `sites` e do arquivo. Pastas e arquivos
  ocultos e atalhos para fora da pasta são bloqueados.
- O servidor grava só dentro da pasta `arquivo` (e o `config.txt`).
- Os botões (guardar, atualizar, remover) só funcionam no próprio computador,
  na própria página, com uma senha de uso único que muda a cada vez que o
  programa abre. Outros sites abertos no navegador não conseguem usá-los.
- Ao baixar sites, o servidor **nunca** acessa endereços da rede interna
  (roteador, outros computadores, o próprio computador), para ninguém usá-lo
  para espionar aparelhos da casa.
- As cópias abrem em "caixa de areia": sem scripts e sem buscar nada na
  internet (sem rastreadores).
- Use o compartilhamento só em redes de confiança: qualquer pessoa no mesmo
  Wi-Fi consegue ver o que está guardado.

## Em caso de apagão

- Os arquivos não se perdem quando a luz cai: ficam gravados no disco.
- O risco é queimar o aparelho quando a energia volta: use filtro de linha
  com proteção ou nobreak.
- Para funcionar durante o apagão: notebook com bateria, nobreak, bateria com
  inversor ou energia solar. O roteador também precisa de energia para o Wi-Fi.
- Faça cópias da pasta inteira em outros computadores, pendrives e HDs.
  Cada cópia é um servidor completo, com o arquivo de sites junto.

## Compilar

```
cargo build --release
```
