# Novidades

O que mudou em cada versão do D200 Deck, para quem usa o app. O PR que sobe a versão escreve a seção dela (`## X.Y.Z`): o "Testes" do PR exige a seção da versão atual, e o release no GitHub usa esse texto.

## 0.3.2 — 2026-10-10

### Correções
- "Colar tecla" (o botão e o Ctrl+V) voltou a funcionar
- Desfazer não perde mais o histórico logo depois de uma edição: quando o app demorava um pouco para recarregar a config, ele achava que ela tinha mudado por fora e zerava o histórico
- Gravar um atalho e trocar de tecla no meio não prende mais o teclado: a próxima tecla digitada em qualquer campo era engolida e gravada na tecla anterior
- Uma tecla de site que traz a aba para a frente não executa mais a ação no app que estiver na frente quando o site não está aberto; agora ela não faz nada e a janela avisa
- O visor "uso do PC" desenhado pelo próprio D200 passa a mostrar o uso do GPU, que ficava sempre em 0
- Se o cabo sair com o dedo no visor, a ação de segurar não dispara mais sozinha ao reconectar
- Corrigida uma falha rara em que o app parava de controlar o D200 ao recarregar a config no mesmo instante em que a conexão caía
- Se a regra aberta no editor some porque o config.json foi editado à mão, a janela volta para "Ao vivo" em vez de ficar em branco

### Ao atualizar
- Quem tem a 0.3.0 ou a 0.3.1 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações.

## 0.3.1 — 2026-10-10

### Correções
- "Mais ícones": a busca em todas as coleções não mostra mais ícones quebrados. As prévias vinham uma a uma do site do Iconify, que passava a recusar depois de algumas buscas; agora o app baixa os ícones de cada coleção de uma vez, e as amostras da lista ficam guardadas no PC
- Alguns resultados da busca, que são outro nome para um ícone, davam "o ícone não existe" ao serem escolhidos; agora funcionam
- Enquanto busca, a janela mostra "Buscando…", e se o Iconify recusar pedidos aparece uma mensagem clara em vez de quadrados vazios

### Ao atualizar
- Quem tem a 0.3.0 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações.

## 0.3.0 — 2026-10-10

### Novidades
- Mais de 370 mil ícones: o botão "Mais ícones…" do seletor abre as coleções do Iconify (a mesma fonte do site Icônes), com busca de coleção, filtro, busca em todas e a opção de colar o nome copiado do Icônes
- Os ícones escolhidos ficam salvos no PC; o D200 não precisa de internet. Ícones de uma cor seguem a cor do ícone da tecla, e os coloridos mantêm as cores
- Funciona nas teclas, no segundo estado e no visor

### Ao atualizar
- Quem tem a 0.2.0 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações.

## 0.2.0 — 2026-10-09

### Novidades
- Ajustes do Windows nas teclas, com o estado real: a tecla muda mesmo se você mexer pelo Windows
  - Ligar e desligar: Bluetooth, Wi-Fi, microfone e som mudos no sistema todo, tema escuro e manter o PC acordado
  - Escolher: saída de áudio, modo de projeção (como o Win+P) e modo de energia
  - Fazer uma vez: suspender, desligar a tela e esvaziar a lixeira
- Pastas: uma tecla abre um grupo de até 12 teclas no lugar do layout. Usar uma delas volta ao normal, ou a pasta pode continuar aberta
- Atualização pelo próprio app: ele avisa quando sai uma versão nova e se atualiza quando você manda

### Correção
- O visor deixou de gerar avisos no log com barras em 0%

### Ao instalar
- Instale esta versão à mão, por cima da 0.1.0; as suas teclas e regras continuam. Daqui em diante, as atualizações chegam pelo app.

## 0.1.0 — 2026-10-08

Primeira versão.

- Teclas com ícones, cores e ações (atalho, abrir app/site, comando, texto, mídia)
- Regras por app ou por site no Edge, nos modos aberto e foco
- Teclas de dois estados e teclas que escurecem com o app fechado
- Visor com relógio, uso do PC, agora tocando, cronômetro, texto ou imagem, com ação ao tocar e ao segurar
- A extensão do Edge vai junto e é carregada pelo guia do app
