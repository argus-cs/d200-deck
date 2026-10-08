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

## Comandos
O Rust fica em `%USERPROFILE%\.cargo\bin`, que pode não estar no PATH do shell.
```
cargo test -p d200
cargo run -p d200 --bin probe -- list
cargo run -p d200 --bin probe -- test
cargo run -p deck-engine --bin deckd
```
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

Referências: redphx/strmdck (MIT) e glmagalhaes.mail/rs-ulanzi-d-200-linux (GitLab).

## Regras
- Não atualizar o firmware pelo Ulanzi Studio sem retestar o protocolo.
- Atalhos simulados não chegam a apps rodando como administrador.
- Ação "abrir": nomes sem pasta (`wt.exe`, `notepad.exe`) são procurados no PATH antes do `ShellExecuteExW` (`actions::find_in_path`). Passando só o nome, o Windows consulta o registro App Paths, onde a Store aponta para o `.exe` dentro de `WindowsApps`; aberto por ali, o app da Store nasce fora do pacote (o Terminal não abre, o Bloco de notas acusa DLL faltando). O `SEE_MASK_NOASYNC` também é necessário, porque a ação roda numa thread que termina logo.
- `cargo run -p deck-engine --bin try-open -- <alvo>` testa a ação "abrir" isolada.
