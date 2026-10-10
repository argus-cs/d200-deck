# D200 Deck

App para Windows que substitui o Ulanzi Studio no **Ulanzi D200**. As teclas mudam sozinhas conforme o app ou o site aberto: uma regra troca só as teclas que você escolheu e, quando ela deixa de valer, tudo volta ao layout padrão.

## O que faz

- **Teclas** com ícone (95 embutidos, mais de 370 mil do [Iconify](https://iconify.design), a mesma fonte do [Icônes](https://icones.js.org), ícone do app, favicon do site ou um PNG seu), texto e cores próprias de fundo, ícone, texto e borda. As coleções do Iconify são baixadas só quando abertas, e o ícone escolhido fica salvo no PC.
- **Ações:** atalho de teclado, abrir app, arquivo, pasta ou site, rodar comando, digitar texto e mídia (play/pause, faixa, volume).
- **Ajustes do Windows**, com o estado real na tecla (ela muda mesmo se você mexer pelo Windows):
  - **Ligar e desligar:** Bluetooth, Wi-Fi, microfone e som mudos no sistema todo, tema escuro e manter o PC acordado.
  - **Escolher:** a saída de áudio (fone ou caixa), o modo de projeção (como o Win+P) e o modo de energia.
  - **Fazer uma vez:** suspender, desligar a tela e esvaziar a lixeira.
- **Pastas:** uma tecla abre um grupo de até 12 teclas no lugar do layout. Usar uma delas volta ao normal; a tecla 1 volta sem fazer nada. A pasta também pode ficar aberta (para volume, por exemplo).
- **Regras por app ou por site no Edge**, em dois modos:
  - **Aberto:** vale enquanto o app ou o site estiver aberto.
  - **Foco:** vale só com ele na frente.
  Cada regra sobrescreve só algumas teclas; as outras continuam com o padrão.
- **Teclas de dois estados:** cada toque alterna entre duas aparências, como microfone ligado e mutado.
- **Teclas "abrir app"** ficam escurecidas enquanto o app está fechado.
- **Visor (tecla 14):** relógio, uso do PC (CPU, memória, GPU e rede), agora tocando com a capa, cronômetro ou Pomodoro, texto ou imagem. Cada regra pode ter o seu. Tocar executa uma ação, e segurar executa outra ou alterna o que o visor mostra.
- **Interface:** D200 ao vivo, editor com arrastar e soltar, copiar e colar teclas, desfazer e refazer, tema claro ou escuro. O app fica na bandeja e pode iniciar com o Windows.
- **Atualizações:** a partir da 0.2.0, o app avisa quando sai uma versão nova e se atualiza quando você manda (no aviso da janela, nos Ajustes ou na bandeja).

## Requisitos

- Windows 10 ou 11 e um Ulanzi D200 (USB `VID 2207`, `PID 0019`).
- Para compilar: [Rust](https://rustup.rs) estável e Node.js 20.19 ou mais novo.
- Microsoft Edge, se for usar regras de site.

**Feche o Ulanzi Studio** (`UlanziDeck.exe`) antes de abrir o D200 Deck, e tire o Ulanzi Studio da inicialização do Windows: os dois disputam o aparelho.

## Compilar

```
cd app
npm install
npx tauri dev                          # desenvolvimento, com recarga da interface
npx tauri build                        # instalador em target/release/bundle/nsis
npx tauri build --debug --no-bundle    # só o executável, em target/debug/d200-deck.exe
```

Fechar a janela deixa o app rodando na bandeja; para encerrar, use **Sair** no menu da bandeja. O executável não pode ser substituído com o app aberto.

O `tauri build` assina o instalador para as atualizações e precisa da chave privada nas variáveis `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Sem ela, use `npx tauri build --debug --no-bundle`.

## Publicar uma versão

1. Num PR, suba a versão em `Cargo.toml`, `app/src-tauri/tauri.conf.json` e `app/package.json` (`npm version X.Y.Z --no-git-tag-version` na pasta `app`) e escreva a seção da versão no [`CHANGELOG.md`](CHANGELOG.md). Sem ela, o "Testes" do PR falha.
2. Depois do merge, crie a tag na `main`: `git tag vX.Y.Z` e `git push origin vX.Y.Z`.
3. O GitHub Actions gera o instalador e cria o release como rascunho, já com as notas do `CHANGELOG.md`. Publique: os apps instalados passam a oferecer a atualização.

## Extensão do Edge (regras de site)

1. Abra `edge://extensions` e ligue o **Modo de desenvolvedor**.
2. Clique em **Carregar sem pacote** e escolha a pasta `extension-edge`. No app instalado, ela fica na pasta de instalação, e o guia dentro do app abre essa pasta e a página de extensões.
3. Depois de mudar algo na extensão, clique em recarregar nela.

A extensão só conversa com o próprio computador (`127.0.0.1:47820`), e o app recusa qualquer conexão que não venha de uma extensão. Abas InPrivate não são vistas.

## Configuração

Tudo é salvo em `%APPDATA%\D200Deck\config.json`, e os ícones escolhidos são copiados para `%APPDATA%\D200Deck\icons`. O arquivo pode ser editado à mão: o app percebe a mudança e recarrega. Uma configuração inválida é ignorada, e o app continua com a anterior.

## Estrutura

| Pasta | O quê |
| --- | --- |
| `crates/d200` | Driver do protocolo USB do D200 e a ferramenta `probe` |
| `crates/engine` | Motor: regras, ações, desenho das teclas e do visor, laço com o aparelho, `deckd` (o app sem interface) |
| `app` | App Tauri 2: Rust em `src-tauri`, interface em Svelte 5 + TypeScript em `src` |
| `extension-edge` | Extensão Manifest V3 que conta ao app quais sites estão abertos |

O protocolo do aparelho, as decisões do projeto e as armadilhas encontradas estão em [`CLAUDE.md`](CLAUDE.md).

## Testes e ferramentas

```
cargo test -p d200 -p deck-engine
cd app && npm run check                                  # tipos da interface
cargo run -p d200 --bin probe -- list                    # o aparelho é encontrado?
cargo run -p deck-engine --bin try-screen -- <pasta>     # desenha exemplos de visor e teclas em PNG
cargo run -p deck-engine --bin try-open -- <alvo>        # testa a ação "abrir" isolada
cargo run -p deck-engine --bin try-system -- state       # lê os ajustes do Windows (set <ajuste> muda de verdade)
cargo run -p deck-engine --bin try-iconify -- <pasta> sets   # baixa e testa ícones do Iconify numa pasta qualquer
```

O app e o `deckd` não podem rodar juntos, porque os dois usam o aparelho e a porta 47820.

Todo PR para a `main` roda esses testes no GitHub Actions, e o merge só é liberado se eles passarem. A `main` não aceita push direto.

## Limitações conhecidas

- Atalhos simulados não chegam a apps rodando como administrador.
- Apps UWP (Calculadora, Configurações) aparecem como `ApplicationFrameHost.exe` na janela da frente.
- Combinações com a tecla Win não podem ser gravadas: digite-as no campo, por exemplo `Win+G`.
- Uma atualização de firmware pelo Ulanzi Studio pode mudar o protocolo.
- Trocar a saída de áudio e o modo de energia usa APIs não documentadas do Windows, que uma atualização pode mudar. O modo de energia só tem efeito com o plano Equilibrado.
- Escolher ícones do Iconify precisa de internet na primeira vez de cada coleção. Algumas coleções (licença CC BY) pedem crédito ao autor; a janela mostra a licença de cada uma.

## Referências

O protocolo foi aprendido com [redphx/strmdck](https://github.com/redphx/strmdck) (MIT) e com o rs-ulanzi-d-200-linux (GitLab), e validado no aparelho.
