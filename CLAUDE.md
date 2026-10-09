# D200 Deck

App para Windows que substitui o Ulanzi Studio no Ulanzi D200 (VID 2207, PID 0019), com teclas dinâmicas: regras por app ou site trocam só as teclas que sobrescrevem, no modo "aberto" ou "foco", e as teclas voltam ao layout padrão quando a regra deixa de valer.

## Decisões
- Roda nativo no Windows, **sem Docker**: USB HID, API Win32 de janelas e o Edge não existem num container. Exceção aprovada pelo usuário em 2026-10-07.
- Stack: Rust (driver e motor de regras), Tauri 2 + TypeScript (interface), extensão Manifest V3 para o Edge (sites).
- Protótipo da interface: https://claude.ai/artifact/H6LxuTWJd7RfxFkWPU4y1L
- Código em inglês; textos da interface em português.

## Estrutura
- `crates/d200/` — driver do protocolo (`protocol.rs`: pacotes; `layout.rs`: zip de layout; `device.rs`: acesso HID).
- `crates/d200/src/bin/probe.rs` — ferramenta de validação da Fase 0.
- `crates/engine/` — motor reaproveitado pelo app Tauri: `config.rs` (JSON em `%APPDATA%\D200Deck\config.json`, teclas numeradas de 1 a 14), `icons.rs` (ícones de traço do protótipo via resvg, ou PNG do usuário), `actions.rs` (atalho, abrir, comando, texto, mídia), `runtime.rs` (conexão, keep-alive de 1 s, recarga da config enviando só as teclas mudadas, reconexão).
- `crates/engine/src/bin/deckd.rs` — o app sem interface.
- `crates/engine/src/rules.rs` — quais regras valem e o que cada tecla mostra (lógica pura, testável). `context.rs` — o lado Windows: janela da frente por evento (`SetWinEventHook`), processos a cada 1 s (só os que regras "aberto" observam) e trazer um app para a frente.
- `crates/engine/src/browser.rs` — WebSocket em `127.0.0.1:47820` para a extensão; recusa qualquer Origin que não seja de extensão (`chrome-extension://`), então nenhuma página da web consegue falar com ele.
- `app/` — o app Tauri 2. `app/src/` é o frontend em Svelte 5 + TypeScript (`lib/types.ts` espelha o `runtime::Status`). `app/src-tauri/src/main.rs` sobe o motor (`runtime::spawn`), repassa cada `Status` para a janela (evento `status`) e para a bandeja, e expõe os comandos `get_status`, `set_paused`, `resend`, `simulate` e `open_config`. `tray.rs` é o menu da bandeja. Fechar a janela destrói o WebView e o motor continua na bandeja; só "Sair" encerra. O início automático passa `--hidden` (abre só na bandeja).
- Editores (Fase 5): `LayerEditor.svelte` edita o layout padrão e cada regra (teclas herdadas tracejadas, sobrescritas com borda âmbar), `KeyEditor.svelte` edita uma tecla, `NewRule.svelte` cria regras e `Settings.svelte` cuida de brilho, visor e estilo do texto. O `App.svelte` salva sozinho 350 ms depois da última mudança via `save_config`, que valida, grava de forma atômica e manda o motor recarregar (`Command::Reload`). Nada inválido chega ao arquivo; se o `config.json` mudar à mão, a revisão no `Status` faz o app recarregar. PNGs escolhidos são copiados para `%APPDATA%\D200Deck\icons`.
- Gravar atalho usa o `keydown` da janela (`app/src/lib/hotkey.ts`). Combinações com Win não chegam à janela e são digitadas no campo. Um gancho `WH_KEYBOARD_LL` foi testado e também não capturou o Win+G nesta máquina, por isso foi descartado.
- `extension-edge/` — extensão Manifest V3 (permissões `tabs` e `alarms`), carregada em `edge://extensions` → Modo de desenvolvedor → Carregar sem pacote. Depois de editar, clicar em recarregar na extensão. A porta está fixa nos dois lados (`PORT`).

## Regras de app (Fase 2)
- Formato: `{ "name", "when": { "process": "Discord.exe" }, "mode": "open" | "focus", "enabled", "keys": { ... } }`. O nome do processo é comparado sem diferenciar maiúsculas, e `.exe` é opcional.
- Camadas: padrão → regras "open" → regras "focus", cada grupo na ordem da lista. Foco ganha de aberto; no mesmo modo, a regra mais abaixo ganha.
- As regras ativas precisam ficar iguais por 200 ms antes de as teclas mudarem (Alt+Tab não pisca).
- `"front": true` numa tecla de regra traz o app para a frente (`AttachThreadInput`; um toque de Alt é o plano B), executa a ação e devolve o foco.
- Limitação conhecida: apps UWP (Calculadora, Configurações) aparecem como `ApplicationFrameHost.exe` na janela da frente.

## Teclas e visor (depois da Fase 5)
- **O texto das teclas é desenhado pelo app dentro da imagem** (`icons::render_key`, Segoe UI); o rótulo do aparelho fica desligado (`ShowTitle: false`). Isso permite cor por tecla: `color` (fundo), `icon_color`, `text_color`, `border`. Tamanho e cor padrão do texto vêm de `label` (tamanho × 2,4 px).
- Tecla de dois estados: `toggle` (um `KeyFace`) alterna a cada toque; o estado vive só na memória do motor, por (nome da regra, tecla da pasta, número). Com uma ação `system`, a segunda face segue o estado real do Windows (ver abaixo).
- Ação "abrir" de um `.exe` escurece a tecla enquanto o app não roda (`Action::watched_process`; `watch: false` desliga). Explorer não conta; `wt.exe` vira `windowsterminal.exe`.
- Ícones de app (`icons::app_icon`, via `IShellItemImageFactory`) e de site (favicon do cache do Edge pela extensão, permissão `favicon`) são salvos como PNG em `%APPDATA%\D200Deck\icons`.
- Visor (`screen.rs`): `config.screen` e `rule.screen`; conteúdos desenhados pelo aparelho (`device_clock`, `device_stats`) ou pelo app (relógio, uso do PC com GPU via PDH, os dois juntos, agora tocando com capa via `GlobalSystemMediaTransportControlsSessionManager`, cronômetro, imagem, texto). Redesenha só quando a assinatura muda. O campo antigo `window` ainda é lido.
- Toque e segurar no visor (`runtime.rs`): `screen.action` roda no toque; `screen.hold` roda ao segurar 700 ms, ainda com o dedo no visor, e é uma ação (`action`) ou uma sequência de conteúdos (`cycle`, avança um a cada vez e volta ao principal). O passo da sequência fica na memória, por regra. Sem `hold`, segurar conta como toque. No cronômetro, toque inicia/pausa e dois toques em até 450 ms zeram.
- `cargo run -p deck-engine --bin try-screen -- <pasta>` desenha exemplos de visor e de teclas em PNG para conferir.

## Ajustes do Windows e pastas
- Ação `{ "type": "system", "setting", "set", "value" }` (`system.rs`). Ligar/desligar (`set`: `toggle` padrão, `on`, `off`): `bluetooth`, `wifi` (`Windows.Devices.Radios`), `microphone` e `sound` (mudo no `IAudioEndpointVolume` do dispositivo padrão; o microfone também no de comunicações), `dark_theme` (registro `Personalize` + `WM_SETTINGCHANGE`), `keep_awake` (requisição de energia, solta quando o app fecha). Escolher (`value`): `audio_output` (nome do dispositivo), `projection` (`internal`/`clone`/`extend`/`external`, `SetDisplayConfig`), `power_mode` (`efficiency`/`balanced`/`performance`). Uma vez: `sleep`, `monitor_off`, `empty_recycle_bin`.
- APIs não documentadas: `IPolicyConfig` (trocar a saída de áudio, a mesma do EarTrumpet) e `PowerSetActiveOverlayScheme`/`PowerGetEffectiveOverlayScheme` (carregadas na hora; o modo de energia só vale com o plano Equilibrado).
- **Estado real na tecla:** o motor lê `system::state` a cada 1 s (a cada 200 ms por 3 s depois de um toque) só para os ajustes que alguma tecla visível ou de pasta usa (`Action::watched_setting`). Desligado (ou outra opção em uso) mostra a segunda face; sem segunda face, a tecla escurece. Ajuste que o PC não tem (sem Bluetooth, saída desligada) escurece. Nesse caso o toque não alterna a memória.
- `system::state` roda na thread do motor, que entra no MTA na primeira chamada; os rádios ficam em cache.
- Pasta: `"folder": { "keys": { "2": … }, "stay": false }` numa tecla, no lugar de `action`/`toggle`. Aberta, ela toma o lugar de todas as teclas (`rules::resolve_folder`); a tecla 1 é sempre "Voltar" (`config::BACK_KEY`, `Key::back`, espelhado em `types.ts`), então a pasta usa as teclas 2 a 13. Usar uma tecla com ação fecha a pasta (a menos que `stay`), tecla vazia não. Fecha sozinha após 30 s sem toque, ao reconectar o aparelho ou se ela sumir da config. Não há pasta dentro de pasta. As teclas de uma pasta de regra agem como teclas da regra (`front`).
- Uma pasta aberta fica até ser fechada, mesmo que a regra dela deixe de valer; ela é achada de novo pelo nome da regra a cada recarga da config.
- `cargo run -p deck-engine --bin try-system -- state` lê todos os ajustes; `-- set <ajuste> [toggle|on|off] [valor]` aplica um (muda o PC de verdade); `-- outputs` lista as saídas de áudio.

## Versões e atualizações
- A `main` só muda por PR: o ruleset "Proteger a main" exige PR (0 aprovações, quem trabalha sozinho não aprova o próprio), bloqueia push forçado e exclusão, não tem exceções e exige o job "Testes" do `.github/workflows/ci.yml` (testes, build da interface, `cargo check` do app e tipos, em Windows). A branch é apagada no merge. Varredura de segredos com bloqueio no push e alertas do Dependabot estão ligados. Tags não são afetadas.
- A versão fica em três lugares que precisam bater: `Cargo.toml` (raiz, `workspace.package`), `app/src-tauri/tauri.conf.json` e `app/package.json` (`npm version X.Y.Z --no-git-tag-version` atualiza o `package-lock.json` junto). A extensão do Edge tem a versão dela.
- Release: subir a versão num PR → merge → `git tag vX.Y.Z` na `main` e `git push origin vX.Y.Z`. O `.github/workflows/release.yml` (Windows) confere a tag contra as três versões, roda os testes, gera o instalador assinado e cria o release como **rascunho** com `latest.json`. Escrever as notas (em português, para quem usa) e publicar: só então os apps instalados enxergam a versão nova. Rodar o workflow à mão só gera o instalador como artefato.
- Atualização no app (`app/src-tauri/src/update.rs`, `tauri-plugin-updater`): procura 20 s depois de abrir e a cada 6 h em `https://github.com/argus-cs/d200-deck/releases/latest/download/latest.json` (por isso o repositório é público). Achou: aviso no topo da janela e "Atualizar para X e reiniciar" na bandeja; só instala quando a pessoa manda. No Windows o instalador (modo `passive`) fecha o app e abre de novo com os mesmos argumentos.
- **Chave de assinatura:** a privada fica em `%USERPROFILE%\.tauri\d200-deck.key` (senha em `d200-deck.password`) e nos secrets `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` do GitHub; a pública está em `tauri.conf.json` (`plugins.updater.pubkey`). **Sem a privada, os apps instalados não aceitam mais atualizações**: manter cópia fora deste PC. Build local assinado: `TAURI_SIGNING_PRIVATE_KEY=<caminho> TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<senha> npx tauri build`.
- O `generate_context!` precisa do crate `serde_json` no app quando o `tauri.conf.json` tem `plugins`.

## Interface (depois da Fase 5)
- Janela sem moldura (`decorations(false)`), barra de título própria com `data-tauri-drag-region`; posição/tamanho lembrados por `tauri-plugin-window-state`.
- `disable_drag_drop_handler()` é necessário para o arrastar e soltar do HTML funcionar no Windows.
- Tema claro/escuro por `prefers-color-scheme` (tokens em `app.css`); o D200 virtual é sempre escuro.
- Desfazer/refazer no `App.svelte` (pilha de snapshots da config, agrupando edições a menos de 800 ms).
- Pastas no editor: duplo clique numa tecla de pasta (ou "Editar a pasta" no `KeyEditor`) troca a grade do `LayerEditor` pelas teclas da pasta, com "Voltar" fixo na 1 e o visor travado. Escolher um ajuste do Windows sugere texto, ícone e segunda face (`settingLook`), sem apagar o que a pessoa já escolheu.

## Regras de site (Fase 3)
- `"when": { "site": "youtube.com" }` casa o domínio e os subdomínios; com `/` ou `*` vira curinga sobre "domínio/caminho" (`github.com/*/pulls`), valendo também para o que estiver abaixo. Esquema, `www.`, porta, query e fragmento são ignorados.
- Foco = Edge na frente **e** o site na aba selecionada da última janela do Edge usada. Aberto = qualquer aba do site.
- `front` numa tecla de site: a extensão seleciona a aba e foca a janela dela, o motor traz o Edge para a frente e executa a ação. Depois volta para a aba anterior (se o Edge já estava na frente) ou devolve o foco ao app anterior.
- Abas InPrivate não são vistas (a extensão não roda nelas, a menos que seja liberada).
- Quando a extensão se desconecta, as regras de site deixam de valer.

## Comandos
O Rust fica em `%USERPROFILE%\.cargo\bin`, que pode não estar no PATH do shell.
```
cargo test -p d200
cargo run -p d200 --bin probe -- list
cargo run -p d200 --bin probe -- test
cargo run -p deck-engine --bin deckd
cd app && npm install && npx tauri dev          # app com recarga do frontend
cd app && npx tauri build --debug --no-bundle   # target/debug/d200-deck.exe
cd app && npm run check                         # tipos do frontend
```
O app e o `deckd` não podem rodar juntos: os dois querem o aparelho e a porta 47820.
Feche o Ulanzi Studio (`UlanziDeck.exe`) antes de falar com o aparelho.

## Protocolo (resumo)
- Interface: usage page `0x000C` (consumer control, `MI_00` no Windows).
- Pacotes de 1024 bytes: `7C 7C` + comando (u16 big-endian) + tamanho total (u32 little-endian) + 1016 bytes; payloads maiores seguem em pacotes crus de 1024.
- No Windows, cada write leva o byte de report ID `00` na frente (1025 bytes).
- Layout: zip com `dummy.txt`, `Images/*.png` (196×196), `manifest.json` (chaves `"col_row"`) e `sentinel.txt`. Nenhum pacote de continuação pode começar com `0x00` ou `0x7C`.
- 14 posições: 13 teclas + o visor largo (índice 13).

### Validado no Windows (Fase 0, 2026-10-07)
- O report descriptor da interface 0 tem entrada e saída de 1024 bytes e nenhum report ID: o prefixo `00` nos writes é obrigatório.
- Layout completo (`0x0001`) e atualização parcial (`0x000D`) funcionam; a parcial muda só as teclas do manifest e mantém as outras.
- Eventos de tecla (`0x0101`) chegam com apertar e soltar; índices 0–12 são as teclas e 13 é o visor.
- O visor mostra a imagem da posição 13 no modo `Background`, mas não mostra o texto.
- Tocar no visor troca o modo dele no próprio aparelho; o `state` do evento segue a sequência `100 → 1 → 200 → 201 → 202 → 203 → 0 → 2`. O app precisa respeitar essa troca ou reimpor o modo escolhido.
- Sem keep-alive (`0x0006`), o aparelho entra em proteção de tela. Ao voltar o keep-alive ele acorda com o layout que já tinha: não é preciso reenviar o layout.
- A proteção de tela vem da falta do PC, não de inatividade: com keep-alive a cada 5 s o aparelho ficou acordado mais de 4 minutos sem nenhum toque.
- Dois processos podem abrir o aparelho ao mesmo tempo no Windows.
- **O aparelho guarda as imagens em cache pelo nome do arquivo no zip.** Nome repetido (mesmo entre execuções) mostra a imagem antiga ou nada. Por isso os nomes levam um contador que começa no relógio (`device.rs::next_nonce`).
- O visor estica a imagem que recebe para o painel; a proporção medida é **458×196** (`screen::WIDTH/HEIGHT`). `probe screen-test <w> <h> --window image` manda um padrão de teste.

Referências: redphx/strmdck (MIT) e glmagalhaes.mail/rs-ulanzi-d-200-linux (GitLab).

## Regras
- Não atualizar o firmware pelo Ulanzi Studio sem retestar o protocolo.
- Atalhos simulados não chegam a apps rodando como administrador.
- Ação "abrir": nomes sem pasta (`wt.exe`, `notepad.exe`) são procurados no PATH antes do `ShellExecuteExW` (`actions::find_in_path`). Passando só o nome, o Windows consulta o registro App Paths, onde a Store aponta para o `.exe` dentro de `WindowsApps`; aberto por ali, o app da Store nasce fora do pacote (o Terminal não abre, o Bloco de notas acusa DLL faltando). O `SEE_MASK_NOASYNC` também é necessário, porque a ação roda numa thread que termina logo.
- `cargo run -p deck-engine --bin try-open -- <alvo>` testa a ação "abrir" isolada.
