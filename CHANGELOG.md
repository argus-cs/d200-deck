# Novidades

O que mudou em cada versão do D200 Deck, para quem usa o app. O PR que sobe a versão escreve a seção dela (`## X.Y.Z`): o "Testes" do PR exige a seção da versão atual, e o release no GitHub usa esse texto.

## 0.4.0 — 2026-10-19

### Novidades
- **Addons:** programas de terceiros agora podem trazer ações novas para as teclas. Cada addon é uma pasta em `%APPDATA%\D200Deck\addons`, e a nova página **Addons**, na barra lateral, lista os que estão lá, liga e desliga cada um e mostra quando algum parou, com **Tentar de novo** e **Abrir o log**. Addons novos chegam desligados: na primeira vez que você liga um, o app mostra quem o fez e o que ele vai rodar e pede confirmação, porque um addon roda com as suas permissões do Windows
- No editor de tecla, as ações dos addons ligados ficam no grupo **Addons** de **Ação**, com os campos de cada uma. O addon pode acender e escurecer a tecla, trocar para o segundo estado e mudar o texto dela (para manter sempre o seu texto, desmarque **Deixar o addon mudar o texto da tecla**). O ícone, as cores e o resto do visual continuam sendo seus
- **Addon da Steam**, que já vem com o app: uma tecla abre um jogo instalado e ganha a capa dele, acesa enquanto o jogo está aberto; com o jogo aberto, um toque traz a janela dele para a frente. Outra tecla abre a biblioteca, os amigos, os downloads, a loja ou as configurações da Steam. Ele começa ligado se a Steam estiver instalada; para desligar, use a página Addons
- Para quem quer fazer um addon: o guia [`docs/addons.md`](https://github.com/argus-cs/d200-deck/blob/main/docs/addons.md) explica o formato e traz um exemplo pronto. Dá para escrever em qualquer linguagem

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.11 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.11 — 2026-10-15

### Novidades
- Novo ajuste do Windows: "Desligar o PC", em "Fazer uma vez". A tecla desliga o PC como o Iniciar → Desligar: se algum app tiver trabalho não salvo, o Windows pergunta antes. É um desligamento completo, sem a inicialização rápida do Windows. Para não desligar com um toque sem querer, ponha a tecla numa pasta
- Segurar o visor agora pode trazer o layout padrão de volta às teclas: no editor do visor, em "Ao segurar o visor", escolha "Mostrar o layout padrão". Com uma regra valendo (o Spotify na frente, por exemplo), segure o visor, use uma tecla do padrão e as teclas da regra voltam. Para voltar sem usar nenhuma tecla, segure de novo ou espere 30 segundos. Enquanto isso, a tela Ao vivo mostra "Layout padrão"

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.10 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.10 — 2026-10-13

### Novidades
- Novo ajuste do Windows: "Entrada de áudio (microfone)". Uma tecla torna o microfone escolhido o padrão em todos os apps, inclusive nas chamadas. Como na saída de áudio, ela fica acesa enquanto esse microfone está em uso e escurece quando é outro, mesmo se você trocar pelo próprio Windows. O editor lista os microfones ligados para você escolher

### Correções
- Ao trocar o dispositivo de uma tecla de saída ou de entrada de áudio, o texto da tecla passa a ser o nome do novo; antes ficava o nome do primeiro escolhido. Um texto que você mesmo digitou continua como está

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.9 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.9 — 2026-10-11

### Segurança
- O app agora só aceita a conexão da extensão vinda do próprio Edge, rodando com o seu usuário do Windows. Antes, qualquer programa no PC, inclusive de outro usuário conectado, podia se passar pela extensão: mandar abas falsas, ligar e desligar regras de site e tirar da extensão a função de trazer a aba para a frente. Para você nada muda: a extensão continua como está

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.8 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.
- Se algum antivírus intercepta as conexões dentro do próprio PC, a extensão pode aparecer como desconectada; o motivo fica no arquivo de log (`%APPDATA%\D200Deck\d200deck.log`).

## 0.3.8 — 2026-10-11

### Melhorias
- Ao abrir o app, o D200 recebe as teclas cerca de 1 segundo antes: preparar a leitura do uso do PC levava esse tempo e agora acontece à parte, e só quando o visor mostra o uso
- Os toques no D200 não esperam mais pela leitura do uso do PC nem pelo menu da bandeja, que podia demorar enquanto a janela abria

### Correções
- Se o Windows não liberar o acesso ao USB, o app continua funcionando (regras, janela e extensão) e tenta de novo a cada 2 segundos, e a janela e a bandeja mostram "Sem acesso ao USB"; antes, o app inteiro parava, sem aviso
- As janelas de escolher arquivo (imagem PNG, programa .exe e o arquivo da ação "abrir") ficam presas à janela do app, como em qualquer programa: não somem mais atrás dela, e o app espera você escolher ou cancelar

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.7 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.7 — 2026-10-11

### Correções
- Pedir a atualização duas vezes (no aviso da janela e na bandeja, ou recarregando a janela no meio) não baixa mais o instalador de novo: enquanto ele baixa, a bandeja mostra "Baixando a versão…" e a janela mostra o progresso, não importa de onde veio o pedido
- Clicar no visor do layout padrão não conta mais como uma edição: numa instalação nova ou numa config antiga, o botão Desfazer acendia sozinho, e desfazer até ali prendia o histórico
- Desfazer não faz mais a edição seguinte se perder quando volta para uma versão igual à atual (por exemplo, depois de mudar algo e voltar atrás muito rápido); antes, o Refazer passava a desfazer essa edição
- A marca "valendo" não aparece mais na regra errada logo depois de arrastar ou apagar uma regra
- Um ícone de app ou de site, uma imagem PNG ou o arquivo da ação "abrir" vão para a tecla em que foram escolhidos, mesmo se você clicar em outra tecla enquanto eles chegam; antes iam para a tecla que estivesse aberta. Trocar de tecla também limpa a busca de ícones e as listas abertas
- A lista de coleções do "Mais ícones" continua aparecendo se o Iconify passar a mandar coleções sem categoria; antes, ela ficaria vazia

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.6 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.6 — 2026-10-10

### Melhorias
- O app ficou menor: o executável tem cerca de 12% a menos, sem mudar nada no que ele faz

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.5 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.5 — 2026-10-10

### Correções
- O arquivo de log não cresce mais sem limite com o app aberto por dias: com o D200 conectado e o visor mostrando relógio ou uso do PC, ele ganhava uma linha a cada 1 ou 2 segundos. Essas atualizações do visor não vão mais para o arquivo, e ele recomeça ao passar de 2 MB mesmo com o app aberto
- O app trabalha menos para manter o visor em dia: quando só o visor muda, ele manda só o visor ao D200 e à janela, em vez de todas as teclas

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.4 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.4 — 2026-10-10

### Novidades
- O app agora guarda um registro do que faz e dos erros que encontra em `%APPDATA%\D200Deck\d200deck.log`, para dar para descobrir por que algo não funcionou

### Correções
- Duas regras não podem mais ter o mesmo nome: com nomes iguais, a tecla de pasta de uma regra podia abrir a pasta da outra. Duplicar ou criar uma regra já escolhe um nome livre, e o editor avisa ao renomear para um nome que já existe. Regras repetidas de versões anteriores ganham um número no fim, como "Discord (2)"
- Uma mudança feita à mão no config.json não é mais sobrescrita por uma edição da janela que ainda ia ser salva, e o aviso de "mudança ainda não salva" some quando o arquivo é recarregado

### Segurança
- Um ícone SVG não consegue mais fazer o app abrir arquivos do computador ou de um servidor da rede, e imagens de tecla em pastas de rede são recusadas

### Ao atualizar
- Quem tem a 0.3.0 até a 0.3.3 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações. Quem vem da 0.3.2 ou anterior precisa recarregar a extensão do Edge uma vez, como explicado na 0.3.3.

## 0.3.3 — 2026-10-10

### Segurança
- O app agora só conversa com a extensão do D200 Deck: antes, qualquer extensão instalada no Edge podia se conectar a ele, mandar abas falsas e disparar regras de site. A extensão ganhou um ID fixo para isso
- A conexão com a extensão tem limites de tamanho e de tempo, para nada travar o app enviando mensagens enormes

### Correções
- A extensão do Edge não fica mais com uma conexão órfã quando o app reinicia, e não falha se uma aba fecha no meio de um envio

### Ao atualizar
- **Recarregue a extensão uma vez** em `edge://extensions` (o botão de recarregar no cartão do D200 Deck): sem isso, as regras de site param de funcionar até você recarregar. Se ela continuar desconectada, remova e carregue de novo pela pasta que o guia do app abre.
- Quem tem a 0.3.0, a 0.3.1 ou a 0.3.2 recebe o aviso no próprio app (no topo da janela e no menu da bandeja) em até 6 horas, ou na hora em Ajustes → Atualizações → Procurar atualizações.

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
